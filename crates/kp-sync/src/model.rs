//! The signed objects Sync exchanges: versions, deletion marks and blobs.
//!
//! The canonical bytes, which the Ed25519 signature covers, are specified in
//! `docs/sync/contrato.md`; protobuf bytes are never signed. A version carries
//! its state encrypted and refers to its content and attachments by SHA-256,
//! so a change of title does not resend the content.

use kryptopass_identity::{
    DeviceId, DeviceIdentity, PublicIdentity, TrustStore, TrustStoreError, verify_signature,
};
use sha2::{Digest as _, Sha256};
use std::fmt;

/// Fixed-width record, version and transfer identifier.
pub type Id = [u8; 16];

/// Fixed-width SHA-256 digest.
pub type Digest = [u8; 32];

/// Maximum number of direct parents of one version.
pub const SYNC_MAX_PARENTS: usize = 16;

/// Maximum recipients of one version or deletion mark.
pub const SYNC_MAX_RECIPIENTS: usize = 32;

/// Maximum blobs a version refers to besides its content.
pub const SYNC_MAX_ATTACHMENTS: usize = 16;

/// Maximum encrypted state carried inside a version.
pub const SYNC_MAX_STATE_BYTES: usize = 256 * 1024;

/// Maximum size of one encrypted blob: item content or an icon image.
pub const SYNC_MAX_BLOB_BYTES: usize = 2 * 1024 * 1024 + 256 * 1024;

/// Maximum heads in one streamed manifest page.
pub const SYNC_MANIFEST_PAGE_HEADS: usize = 256;

/// Maximum versions and deletion marks in one batch.
pub const SYNC_MAX_OBJECTS_PER_BATCH: usize = 32;

/// Maximum encoded batch size.
pub const SYNC_MAX_BATCH_BYTES: usize = 8 * 1024 * 1024;

/// Maximum versions and deletion marks one store offers to a peer.
pub const SYNC_MAX_OBJECTS: usize = 10_000;

const VERSION_DOMAIN: &[u8] = b"org.kryptopass.sync.version\0";
const DELETION_DOMAIN: &[u8] = b"org.kryptopass.sync.deletion\0";
const OBJECT_DOMAIN: &[u8] = b"org.kryptopass.sync.object\0";
const FORMAT: u16 = 1;
const SIGNATURE_BYTES: usize = 64;

/// What a record is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum RecordKind {
    Item = 1,
    Folder = 2,
}

impl RecordKind {
    pub const fn from_wire(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Item),
            2 => Some(Self::Folder),
            _ => None,
        }
    }

    pub const fn as_wire(self) -> u32 {
        self as u32
    }
}

/// Hybrid logical clock: later than every parent's, even when the author's
/// wall clock is behind. Only a hint for preselecting a conflict side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Clock {
    pub millis: u64,
    pub counter: u32,
}

/// An OpenPGP primary key fingerprint: 20 bytes (v4) or 32 bytes (v5/v6).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(Vec<u8>);

impl Fingerprint {
    pub fn new(bytes: Vec<u8>) -> Result<Self, SyncError> {
        if matches!(bytes.len(), 20 | 32) && bytes.iter().any(|byte| *byte != 0) {
            Ok(Self(bytes))
        } else {
            Err(SyncError::InvalidObject)
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// The fields of a version before it is signed.
#[derive(Clone)]
pub struct VersionDraft {
    pub record_kind: RecordKind,
    pub record_id: Id,
    pub version_id: Id,
    pub parents: Vec<Id>,
    pub clock: Clock,
    pub recipients: Vec<Fingerprint>,
    /// The state encrypted to `recipients`.
    pub state: Vec<u8>,
    /// SHA-256 of the encrypted content; items only.
    pub content_hash: Option<Digest>,
    /// SHA-256 of other blobs the state refers to, such as an icon image.
    pub attachments: Vec<Digest>,
}

/// An immutable, signed version of an item or folder.
#[derive(Clone, PartialEq, Eq)]
pub struct Version {
    pub record_kind: RecordKind,
    pub record_id: Id,
    pub version_id: Id,
    /// Unique, in ascending byte order.
    pub parents: Vec<Id>,
    pub author: DeviceId,
    pub clock: Clock,
    /// Unique, in ascending byte order.
    pub recipients: Vec<Fingerprint>,
    pub state: Vec<u8>,
    pub content_hash: Option<Digest>,
    /// Unique, in ascending byte order.
    pub attachments: Vec<Digest>,
    pub signature: [u8; SIGNATURE_BYTES],
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Version([redacted])")
    }
}

impl Version {
    /// Sorts the lists, checks every field and signs with `author`.
    pub fn sign(draft: VersionDraft, author: &DeviceIdentity) -> Result<Self, SyncError> {
        let VersionDraft {
            record_kind,
            record_id,
            version_id,
            mut parents,
            clock,
            mut recipients,
            state,
            content_hash,
            mut attachments,
        } = draft;
        parents.sort_unstable();
        recipients.sort_unstable();
        attachments.sort_unstable();
        let mut version = Self {
            record_kind,
            record_id,
            version_id,
            parents,
            author: author.device_id(),
            clock,
            recipients,
            state,
            content_hash,
            attachments,
            signature: [0; SIGNATURE_BYTES],
        };
        version.signature = author.sign(&version.canonical_bytes()?);
        Ok(version)
    }

    /// Checks the fields and that `author` signed them.
    pub fn verify(&self, author: &PublicIdentity) -> Result<(), SyncError> {
        if author.validate().is_err() || author.device_id != self.author {
            return Err(SyncError::InvalidSignature);
        }
        verify_signature(
            &author.signing_public_key,
            &self.canonical_bytes()?,
            &self.signature,
        )
        .map_err(|_| SyncError::InvalidSignature)
    }

    /// The bytes the signature covers.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SyncError> {
        self.validate()?;
        let mut bytes = Canonical::new(VERSION_DOMAIN);
        bytes.u8(self.record_kind as u8);
        bytes.raw(&self.record_id);
        bytes.raw(&self.version_id);
        bytes.list16(self.parents.iter().map(|id| id.as_slice()), |out, id| {
            out.raw(id)
        })?;
        bytes.raw(self.author.as_bytes());
        bytes.clock(self.clock);
        bytes.list16(
            self.recipients.iter().map(Fingerprint::as_bytes),
            Canonical::bytes8,
        )?;
        bytes.bytes32(&self.state)?;
        bytes.bytes8(
            self.content_hash
                .as_ref()
                .map_or(&[][..], |hash| hash.as_slice()),
        );
        bytes.list16(
            self.attachments.iter().map(|hash| hash.as_slice()),
            |out, hash| out.raw(hash),
        )?;
        Ok(bytes.finish())
    }

    /// The digest that names this exact signed object.
    pub fn hash(&self) -> Result<Digest, SyncError> {
        Ok(object_hash(&self.canonical_bytes()?, &self.signature))
    }

    /// Every blob this version needs: its content, then its attachments.
    pub fn blobs(&self) -> impl Iterator<Item = &Digest> {
        self.content_hash.iter().chain(&self.attachments)
    }

    pub(crate) fn validate(&self) -> Result<(), SyncError> {
        let content_matches_kind = match self.record_kind {
            RecordKind::Item => self.content_hash.is_some(),
            RecordKind::Folder => self.content_hash.is_none(),
        };
        if is_zero_id(&self.record_id)
            || is_zero_id(&self.version_id)
            || self.parents.iter().any(is_zero_id)
            || self.parents.contains(&self.version_id)
            || !strictly_ascending(&self.parents)
            || !strictly_ascending(&self.recipients)
            || !strictly_ascending(&self.attachments)
            || self.recipients.is_empty()
            || self.state.is_empty()
            || !content_matches_kind
        {
            return Err(SyncError::InvalidObject);
        }
        if self.parents.len() > SYNC_MAX_PARENTS
            || self.recipients.len() > SYNC_MAX_RECIPIENTS
            || self.attachments.len() > SYNC_MAX_ATTACHMENTS
            || self.state.len() > SYNC_MAX_STATE_BYTES
        {
            return Err(SyncError::LimitExceeded);
        }
        Ok(())
    }
}

/// The fields of a deletion mark before it is signed.
#[derive(Clone)]
pub struct DeletionDraft {
    pub record_kind: RecordKind,
    pub record_id: Id,
    pub clock: Clock,
    /// Recipients of the record's last version, to route the mark like it.
    pub recipients: Vec<Fingerprint>,
}

/// A signed mark that a record was deleted for good. It carries no title,
/// content or history; it only stops old versions from bringing it back.
#[derive(Clone, PartialEq, Eq)]
pub struct Deletion {
    pub record_kind: RecordKind,
    pub record_id: Id,
    pub author: DeviceId,
    pub clock: Clock,
    pub recipients: Vec<Fingerprint>,
    pub signature: [u8; SIGNATURE_BYTES],
}

impl fmt::Debug for Deletion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Deletion([redacted])")
    }
}

impl Deletion {
    pub fn sign(draft: DeletionDraft, author: &DeviceIdentity) -> Result<Self, SyncError> {
        let DeletionDraft {
            record_kind,
            record_id,
            clock,
            mut recipients,
        } = draft;
        recipients.sort_unstable();
        let mut deletion = Self {
            record_kind,
            record_id,
            author: author.device_id(),
            clock,
            recipients,
            signature: [0; SIGNATURE_BYTES],
        };
        deletion.signature = author.sign(&deletion.canonical_bytes()?);
        Ok(deletion)
    }

    pub fn verify(&self, author: &PublicIdentity) -> Result<(), SyncError> {
        if author.validate().is_err() || author.device_id != self.author {
            return Err(SyncError::InvalidSignature);
        }
        verify_signature(
            &author.signing_public_key,
            &self.canonical_bytes()?,
            &self.signature,
        )
        .map_err(|_| SyncError::InvalidSignature)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SyncError> {
        self.validate()?;
        let mut bytes = Canonical::new(DELETION_DOMAIN);
        bytes.u8(self.record_kind as u8);
        bytes.raw(&self.record_id);
        bytes.raw(self.author.as_bytes());
        bytes.clock(self.clock);
        bytes.list16(
            self.recipients.iter().map(Fingerprint::as_bytes),
            Canonical::bytes8,
        )?;
        Ok(bytes.finish())
    }

    pub fn hash(&self) -> Result<Digest, SyncError> {
        Ok(object_hash(&self.canonical_bytes()?, &self.signature))
    }

    pub(crate) fn validate(&self) -> Result<(), SyncError> {
        if is_zero_id(&self.record_id)
            || self.recipients.is_empty()
            || !strictly_ascending(&self.recipients)
        {
            return Err(SyncError::InvalidObject);
        }
        if self.recipients.len() > SYNC_MAX_RECIPIENTS {
            return Err(SyncError::LimitExceeded);
        }
        Ok(())
    }
}

/// The name of a blob: the SHA-256 of its encrypted bytes.
pub fn blob_hash(blob: &[u8]) -> Digest {
    Sha256::digest(blob).into()
}

/// One entry of a manifest: a version head or a deletion mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Head {
    pub record_kind: RecordKind,
    pub record_id: Id,
    /// `None` for a deletion mark.
    pub version_id: Option<Id>,
    pub object_hash: Digest,
}

fn object_hash(canonical: &[u8], signature: &[u8; SIGNATURE_BYTES]) -> Digest {
    let mut hasher = Sha256::new();
    hasher.update(OBJECT_DOMAIN);
    hasher.update(canonical);
    hasher.update(signature);
    hasher.finalize().into()
}

fn strictly_ascending<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

/// Big-endian canonical encoding: lists carry a `u16` count, fingerprints and
/// optional hashes a `u8` length, the state a `u32` length.
struct Canonical(Vec<u8>);

impl Canonical {
    fn new(domain: &[u8]) -> Self {
        let mut bytes = Vec::with_capacity(512);
        bytes.extend_from_slice(domain);
        bytes.extend_from_slice(&FORMAT.to_be_bytes());
        Self(bytes)
    }

    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn raw(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }

    fn clock(&mut self, clock: Clock) {
        self.0.extend_from_slice(&clock.millis.to_be_bytes());
        self.0.extend_from_slice(&clock.counter.to_be_bytes());
    }

    fn bytes8(&mut self, value: &[u8]) {
        // Callers bound these to 32 bytes.
        self.0.push(value.len() as u8);
        self.0.extend_from_slice(value);
    }

    fn bytes32(&mut self, value: &[u8]) -> Result<(), SyncError> {
        let length = u32::try_from(value.len()).map_err(|_| SyncError::LimitExceeded)?;
        self.0.extend_from_slice(&length.to_be_bytes());
        self.0.extend_from_slice(value);
        Ok(())
    }

    fn list16<'a>(
        &mut self,
        items: impl ExactSizeIterator<Item = &'a [u8]>,
        mut write: impl FnMut(&mut Self, &'a [u8]),
    ) -> Result<(), SyncError> {
        let count = u16::try_from(items.len()).map_err(|_| SyncError::LimitExceeded)?;
        self.0.extend_from_slice(&count.to_be_bytes());
        for item in items {
            write(self, item);
        }
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }
}

/// Author lookup backed by explicit pairing trust or the local device identity.
pub trait TrustSource {
    /// Returns the currently trusted public identity for `device_id`, if any.
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError>;

    /// Returns the key that signed already stored history. A source may keep the
    /// public key of an author it no longer trusts, so old versions stay
    /// verifiable; such an author never passes [`TrustSource::lookup`], so it
    /// cannot sign or introduce anything new.
    fn lookup_history(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        self.lookup(device_id)
    }
}

/// Reads only the remote identities explicitly authorized in the trust store.
impl<S: TrustStore + ?Sized> TrustSource for S {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        match self.lookup_trusted_peer(&device_id) {
            Ok(identity) if identity.device_id == device_id => Ok(Some(identity)),
            Ok(_) => Err(SyncError::InvalidStore),
            Err(TrustStoreError::UnknownOrRevokedPeer) => Ok(None),
            Err(_) => Err(SyncError::Storage),
        }
    }
}

/// Trust adapter for local writes and explicitly trusted remote authors.
///
/// The local identity is supplied by the caller from its existing protected
/// identity store. It is never inserted into or authorized by the peer store.
pub struct LocalAndTrustedPeers<'a> {
    local: PublicIdentity,
    peers: &'a dyn TrustStore,
}

impl<'a> LocalAndTrustedPeers<'a> {
    /// Combines the supplied local identity with the existing remote trust store.
    pub fn new(local: &DeviceIdentity, peers: &'a dyn TrustStore) -> Result<Self, SyncError> {
        let local = local.public_identity();
        local.validate().map_err(|_| SyncError::InvalidSignature)?;
        Ok(Self { local, peers })
    }
}

impl TrustSource for LocalAndTrustedPeers<'_> {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        if device_id == self.local.device_id {
            return Ok(Some(self.local));
        }
        self.peers.lookup(device_id)
    }
}

/// Generates a nonzero random UUID v4 identifier.
pub fn fresh_id() -> Result<Id, SyncError> {
    let mut id = [0; 16];
    getrandom::fill(&mut id).map_err(|_| SyncError::Randomness)?;
    id[6] = (id[6] & 0x0f) | 0x40;
    id[8] = (id[8] & 0x3f) | 0x80;
    Ok(id)
}

pub(crate) fn is_zero_id(id: &Id) -> bool {
    id.iter().all(|byte| *byte == 0)
}

/// Payload-free errors for validation, storage, trust and bounded codecs.
///
/// Debug formatting is intentionally identical for every variant; callers
/// should use the stable Display description for non-sensitive diagnostics.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SyncError {
    /// The store failed to read or write.
    Storage,
    /// Stored data failed its integrity checks.
    InvalidStore,
    /// A version or deletion mark has invalid fields.
    InvalidObject,
    /// A trusted identity did not authenticate the signature.
    InvalidSignature,
    /// The author is not present in the applicable trust source.
    UnknownAuthor,
    /// A referenced parent version is absent.
    MissingParent,
    /// A parent belongs to another record.
    CrossRecordParent,
    /// The version graph contains a cycle.
    Cycle,
    /// A referenced blob is absent or does not match its hash.
    MissingBlob,
    /// An id names different signed bytes on each side.
    Collision,
    /// A configured object, byte or graph limit was exceeded.
    LimitExceeded,
    /// Protobuf encoding failed or could not represent the batch.
    Encode,
    /// Protobuf decoding or structural preflight failed.
    Decode,
    /// An allocation needed for a bounded object failed.
    Allocation,
    /// The operating system random source failed.
    Randomness,
}

impl fmt::Debug for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SyncError([redacted])")
    }
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Storage => "sync storage operation failed",
            Self::InvalidStore => "sync store is invalid",
            Self::InvalidObject => "sync object is invalid",
            Self::InvalidSignature => "sync object signature is invalid",
            Self::UnknownAuthor => "sync object author is not trusted",
            Self::MissingParent => "sync version parent is missing",
            Self::CrossRecordParent => "sync version parent belongs to another record",
            Self::Cycle => "sync version graph contains a cycle",
            Self::MissingBlob => "sync blob is missing",
            Self::Collision => "sync identifier collision",
            Self::LimitExceeded => "sync limit exceeded",
            Self::Encode => "sync batch encoding failed",
            Self::Decode => "sync batch decoding failed",
            Self::Allocation => "sync allocation failed",
            Self::Randomness => "sync random source failed",
        })
    }
}

impl std::error::Error for SyncError {}
