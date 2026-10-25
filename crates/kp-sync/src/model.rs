//! Canonical signed revision objects and their identity trust boundary.

use kryptopass_identity::{
    DeviceId, DeviceIdentity, PublicIdentity, SqliteTrustStore, TrustStoreError, verify_signature,
};
use sha2::{Digest as _, Sha256};
use std::fmt;

/// Fixed-width vault, record and revision identifier.
pub type Id = [u8; 16];

/// Fixed-width SHA-256 digest.
pub type Digest = [u8; 32];

/// Maximum number of direct causal parents on one revision.
pub const SYNC_MAX_PARENTS: usize = 16;

/// Maximum number of current heads that can be merged into one revision.
pub const SYNC_MAX_HEADS: usize = 16;

/// Maximum heads in one streamed manifest page; 256 fixed-size heads fit one payload.
pub const SYNC_MANIFEST_PAGE_HEADS: usize = 256;

/// Maximum revisions in one protobuf batch.
pub const SYNC_MAX_REVISIONS_PER_BATCH: usize = 32;

/// Maximum encoded protobuf batch size.
pub const SYNC_MAX_BATCH_BYTES: usize = 8 * 1024 * 1024;

/// Maximum encrypted content carried by one revision.
pub const SYNC_MAX_CIPHERTEXT_BYTES: usize = 2 * 1024 * 1024;

/// Maximum revisions retained by an isolated ledger.
pub const SYNC_MAX_LEDGER_REVISIONS: usize = 10_000;

/// Maximum ciphertext bytes retained by an isolated ledger.
pub const SYNC_MAX_LEDGER_CIPHERTEXT_BYTES: usize = 64 * 1024 * 1024;

const REVISION_DOMAIN: &[u8] = b"org.kryptopass.sync-revision.v1\0";
const OBJECT_DOMAIN: &[u8] = b"org.kryptopass.sync-object.v1\0";
const REVISION_VERSION: u16 = 1;
const SIGNATURE_BYTES: usize = 64;

/// The signed operation represented by a revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Operation {
    /// Replace the complete encrypted record contents.
    Write = 1,
    /// Retain a tombstone and its causal history.
    Delete = 2,
}

impl Operation {
    pub(crate) const fn from_wire(value: i32) -> Option<Self> {
        match value {
            1 => Some(Self::Write),
            2 => Some(Self::Delete),
            _ => None,
        }
    }

    const fn as_byte(self) -> u8 {
        self as u8
    }
}

/// Immutable, signed encrypted revision and its causal metadata.
#[derive(Clone, PartialEq, Eq)]
pub struct Revision {
    /// Vault to which the revision belongs.
    pub vault_id: Id,
    /// Stable identifier of the logical record.
    pub record_id: Id,
    /// Stable identifier of this immutable revision.
    pub revision_id: Id,
    /// Unique parent IDs in unsigned byte lexicographic order.
    pub parent_revision_ids: Vec<Id>,
    /// Device that signed this revision.
    pub author_device_id: DeviceId,
    /// Write or tombstone operation.
    pub operation: Operation,
    /// Opaque encrypted record contents, or empty for a tombstone.
    pub ciphertext: Vec<u8>,
    /// SHA-256 of `ciphertext`, including SHA-256(empty) for deletes.
    pub content_hash: Digest,
    /// Ed25519 signature over [`Self::canonical_bytes`].
    pub signature: [u8; SIGNATURE_BYTES],
}

impl fmt::Debug for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Revision([redacted])")
    }
}

impl Revision {
    /// Creates a revision and signs the documented canonical byte sequence.
    ///
    /// Parent IDs are sorted before signing. Duplicate parents are rejected.
    /// A write must carry 1..=2 MiB of encrypted content; a delete carries none.
    pub fn sign(
        vault_id: Id,
        record_id: Id,
        revision_id: Id,
        mut parents: Vec<Id>,
        operation: Operation,
        ciphertext: Vec<u8>,
        author: &DeviceIdentity,
    ) -> Result<Self, SyncError> {
        if parents.len() > SYNC_MAX_PARENTS {
            return Err(SyncError::LimitExceeded);
        }
        parents.sort_unstable();
        if parents.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(SyncError::InvalidRevision);
        }
        if is_zero_id(&vault_id)
            || is_zero_id(&record_id)
            || is_zero_id(&revision_id)
            || parents.iter().any(is_zero_id)
        {
            return Err(SyncError::InvalidRevision);
        }
        if ciphertext.len() > SYNC_MAX_CIPHERTEXT_BYTES {
            return Err(SyncError::LimitExceeded);
        }
        match operation {
            Operation::Write if ciphertext.is_empty() => return Err(SyncError::InvalidRevision),
            Operation::Delete if !ciphertext.is_empty() => {
                return Err(SyncError::InvalidRevision);
            }
            Operation::Write | Operation::Delete => {}
        }

        let mut revision = Self {
            vault_id,
            record_id,
            revision_id,
            parent_revision_ids: parents,
            author_device_id: author.device_id(),
            operation,
            content_hash: Sha256::digest(&ciphertext).into(),
            ciphertext,
            signature: [0; SIGNATURE_BYTES],
        };
        let bytes = revision.canonical_bytes()?;
        revision.signature = author.sign(&bytes);
        Ok(revision)
    }

    /// Verifies the author binding, object fields, content digest and signature.
    pub fn verify(&self, author: &PublicIdentity) -> Result<(), SyncError> {
        self.validate_fields()?;
        author.validate().map_err(|_| SyncError::InvalidSignature)?;
        if author.device_id != self.author_device_id {
            return Err(SyncError::InvalidSignature);
        }
        let bytes = self.canonical_bytes()?;
        verify_signature(&author.signing_public_key, &bytes, &self.signature)
            .map_err(|_| SyncError::InvalidSignature)
    }

    /// Returns the exact bytes covered by the Ed25519 signature.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SyncError> {
        self.validate_fields()?;
        let parent_bytes = self
            .parent_revision_ids
            .len()
            .checked_mul(16)
            .ok_or(SyncError::LimitExceeded)?;
        let capacity = REVISION_DOMAIN
            .len()
            .checked_add(2 + 16 + 16 + 16 + 2)
            .and_then(|value| value.checked_add(parent_bytes))
            .and_then(|value| value.checked_add(32 + 1 + 4))
            .and_then(|value| value.checked_add(self.ciphertext.len()))
            .and_then(|value| value.checked_add(32))
            .ok_or(SyncError::LimitExceeded)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(capacity)
            .map_err(|_| SyncError::Allocation)?;
        bytes.extend_from_slice(REVISION_DOMAIN);
        bytes.extend_from_slice(&REVISION_VERSION.to_be_bytes());
        bytes.extend_from_slice(&self.vault_id);
        bytes.extend_from_slice(&self.record_id);
        bytes.extend_from_slice(&self.revision_id);
        bytes.extend_from_slice(&(self.parent_revision_ids.len() as u16).to_be_bytes());
        for parent in &self.parent_revision_ids {
            bytes.extend_from_slice(parent);
        }
        bytes.extend_from_slice(self.author_device_id.as_bytes());
        bytes.push(self.operation.as_byte());
        bytes.extend_from_slice(&(self.ciphertext.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.ciphertext);
        bytes.extend_from_slice(&self.content_hash);
        Ok(bytes)
    }

    pub(crate) fn validate(&self) -> Result<(), SyncError> {
        self.validate_fields()
    }

    /// Returns the immutable object digest, including the author's signature.
    pub fn hash(&self) -> Result<Digest, SyncError> {
        let canonical = self.canonical_bytes()?;
        let mut hasher = Sha256::new();
        hasher.update(OBJECT_DOMAIN);
        hasher.update(canonical);
        hasher.update(self.signature);
        Ok(hasher.finalize().into())
    }

    fn validate_fields(&self) -> Result<(), SyncError> {
        if is_zero_id(&self.vault_id)
            || is_zero_id(&self.record_id)
            || is_zero_id(&self.revision_id)
            || self.parent_revision_ids.iter().any(is_zero_id)
            || self
                .parent_revision_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err(SyncError::InvalidRevision);
        }
        if self.parent_revision_ids.len() > SYNC_MAX_PARENTS
            || self.ciphertext.len() > SYNC_MAX_CIPHERTEXT_BYTES
        {
            return Err(SyncError::LimitExceeded);
        }
        match self.operation {
            Operation::Write if self.ciphertext.is_empty() => {
                return Err(SyncError::InvalidRevision);
            }
            Operation::Delete if !self.ciphertext.is_empty() => {
                return Err(SyncError::InvalidRevision);
            }
            Operation::Write | Operation::Delete => {}
        }
        let expected_hash: Digest = Sha256::digest(&self.ciphertext).into();
        if self.content_hash != expected_hash {
            return Err(SyncError::InvalidRevision);
        }
        Ok(())
    }
}

/// One record head advertised by a manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevisionHead {
    /// Logical record identifier.
    pub record_id: Id,
    /// Immutable revision identifier.
    pub revision_id: Id,
    /// Digest committing to the complete signed revision.
    pub revision_hash: Digest,
}

/// Author lookup backed by explicit pairing trust or the local device identity.
pub trait TrustSource {
    /// Returns the currently trusted public identity for `device_id`, if any.
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError>;
}

/// Reads only the remote identities explicitly authorized in the trust store.
impl TrustSource for SqliteTrustStore {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        match self.lookup_trusted_peer(&device_id) {
            Ok(identity) if identity.device_id == device_id => Ok(Some(identity)),
            Ok(_) => Err(SyncError::InvalidLedger),
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
    peers: &'a SqliteTrustStore,
}

impl<'a> LocalAndTrustedPeers<'a> {
    /// Combines the supplied local identity with the existing remote trust store.
    pub fn new(local: &DeviceIdentity, peers: &'a SqliteTrustStore) -> Result<Self, SyncError> {
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
    /// Persistent ledger I/O failed.
    Storage,
    /// The persisted ledger failed its integrity checks.
    InvalidLedger,
    /// A batch or operation is bound to another vault.
    VaultMismatch,
    /// A revision has invalid fields or content hash.
    InvalidRevision,
    /// A trusted identity did not authenticate the revision signature.
    InvalidSignature,
    /// The author is not present in the applicable trust source.
    UnknownAuthor,
    /// A referenced parent revision is absent.
    MissingParent,
    /// A parent belongs to a different record.
    CrossRecordParent,
    /// A revision refers to itself as a parent.
    SelfParent,
    /// The revision graph contains a cycle.
    Cycle,
    /// An existing revision ID was reused for different immutable bytes.
    RevisionIdCollision,
    /// An append's expected current heads do not match the ledger.
    HeadConflict,
    /// A configured revision, byte or graph limit was exceeded.
    LimitExceeded,
    /// Protobuf encoding failed or could not represent the requested batch.
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
            Self::InvalidLedger => "sync ledger is invalid",
            Self::VaultMismatch => "sync vault identifier mismatch",
            Self::InvalidRevision => "sync revision is invalid",
            Self::InvalidSignature => "sync revision signature is invalid",
            Self::UnknownAuthor => "sync revision author is not trusted",
            Self::MissingParent => "sync revision parent is missing",
            Self::CrossRecordParent => "sync revision parent belongs to another record",
            Self::SelfParent => "sync revision cannot be its own parent",
            Self::Cycle => "sync revision graph contains a cycle",
            Self::RevisionIdCollision => "sync revision identifier collision",
            Self::HeadConflict => "sync revision heads changed",
            Self::LimitExceeded => "sync limit exceeded",
            Self::Encode => "sync batch encoding failed",
            Self::Decode => "sync batch decoding failed",
            Self::Allocation => "sync allocation failed",
            Self::Randomness => "sync random source failed",
        })
    }
}

impl std::error::Error for SyncError {}
