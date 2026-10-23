//! Vault application service: shared encrypted objects and atomic signed revisions.
//! No Qt, network transport, or GnuPG runtime is required; crypto is injected.

use kryptopass_core::{
    KeyFingerprint, MAX_VAULT_OBJECT_BYTES, OpenPgp, RecipientSet, Secret, SecretContent, SecretId,
    SecretLink, VaultObject, decode_content, decode_vault_object, encode_vault_object,
};
use kryptopass_identity::DeviceIdentity;
use kryptopass_sync::{
    Digest, Id, Ledger, Operation, Revision, SYNC_MAX_CIPHERTEXT_BYTES,
    SYNC_MAX_REVISIONS_PER_BATCH, SyncError, TrustSource, fresh_id,
};
use std::collections::BTreeSet;
use std::fmt;

const MAX_PROJECTED_PLAINTEXT_BYTES: usize = 8 * 1024 * 1024;

/// Payload-free service failures suitable for mapping to fixed UI messages.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VaultSyncError {
    InvalidInput,
    InvalidObject,
    ContentNotRepresentable,
    MetadataNotRepresentable,
    Crypto,
    Trust,
    Conflict,
    DestinationNotEmpty,
    LimitExceeded,
    Ledger,
}

impl fmt::Debug for VaultSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VaultSyncError([redacted])")
    }
}

impl fmt::Display for VaultSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "vault_sync_invalid_input",
            Self::InvalidObject => "vault_sync_invalid_object",
            Self::ContentNotRepresentable => "vault_sync_content_not_representable",
            Self::MetadataNotRepresentable => "vault_sync_metadata_not_representable",
            Self::Crypto => "vault_sync_crypto_failed",
            Self::Trust => "vault_sync_trust_failed",
            Self::Conflict => "vault_sync_heads_changed",
            Self::DestinationNotEmpty => "vault_sync_destination_not_empty",
            Self::LimitExceeded => "vault_sync_limit_exceeded",
            Self::Ledger => "vault_sync_ledger_failed",
        })
    }
}

impl std::error::Error for VaultSyncError {}

/// One current revision head. Every branch is returned, including conflicts
/// and tombstones; the service never elects a timestamp-based winner.
pub struct VaultHeadProjection {
    pub record_id: SecretId,
    pub revision_id: Id,
    pub revision_hash: Digest,
    pub parent_revision_ids: Vec<Id>,
    pub operation: Operation,
    /// Present for a write head, absent for a persistent delete tombstone.
    pub object: Option<VaultObject>,
}

impl fmt::Debug for VaultHeadProjection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultHeadProjection")
            .field("record_id", &self.record_id)
            .field("revision_id", &self.revision_id)
            .field("operation", &self.operation)
            .field("object", &self.object.as_ref().map(|_| "[redacted]"))
            .finish_non_exhaustive()
    }
}

/// Non-secret acknowledgement data for a committed local revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevisionReceipt {
    pub record_id: Id,
    pub revision_id: Id,
    pub revision_hash: Digest,
    pub operation: Operation,
}

/// Non-secret result of an explicit one-shot migration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MigrationReceipt {
    pub imported_records: usize,
}

/// Vault operations backed by the immutable signed ledger and the existing
/// local OpenPGP implementation.
pub struct SyncedVault<'a> {
    ledger: &'a mut Ledger,
    fingerprint: KeyFingerprint,
    identity: &'a DeviceIdentity,
    trust: &'a dyn TrustSource,
    crypto: &'a dyn OpenPgp,
}

impl<'a> SyncedVault<'a> {
    /// Binds an existing ledger to explicitly supplied local identity, trust
    /// source, key fingerprint and OpenPGP capability. It creates none of them.
    pub fn new(
        ledger: &'a mut Ledger,
        fingerprint: KeyFingerprint,
        identity: &'a DeviceIdentity,
        trust: &'a dyn TrustSource,
        crypto: &'a dyn OpenPgp,
    ) -> Result<Self, VaultSyncError> {
        let trusted_local = trust
            .lookup(identity.device_id())
            .map_err(map_sync_error)?
            .ok_or(VaultSyncError::Trust)?;
        if trusted_local != identity.public_identity() {
            return Err(VaultSyncError::Trust);
        }
        Ok(Self {
            ledger,
            fingerprint,
            identity,
            trust,
            crypto,
        })
    }

    /// Returns a decrypted projection for every current head in the ledger.
    /// Parallel heads are preserved as separate entries and tombstones remain
    /// visible to callers.
    pub fn list(&self) -> Result<Vec<VaultHeadProjection>, VaultSyncError> {
        let heads = self.ledger.manifest().map_err(map_sync_error)?;
        let mut result = Vec::with_capacity(heads.len());
        let mut plaintext_bytes = 0usize;
        for head in heads {
            let revision = self
                .ledger
                .get(head.revision_id)
                .map_err(map_sync_error)?
                .ok_or(VaultSyncError::Ledger)?;
            if revision.record_id != head.record_id
                || revision.hash().map_err(map_sync_error)? != head.revision_hash
            {
                return Err(VaultSyncError::Ledger);
            }
            let object = match revision.operation {
                Operation::Delete => None,
                Operation::Write => {
                    let plaintext = self
                        .crypto
                        .decrypt(&revision.ciphertext)
                        .map_err(|_| VaultSyncError::Crypto)?;
                    if plaintext.len() > MAX_VAULT_OBJECT_BYTES {
                        return Err(VaultSyncError::LimitExceeded);
                    }
                    plaintext_bytes = plaintext_bytes
                        .checked_add(plaintext.len())
                        .filter(|total| *total <= MAX_PROJECTED_PLAINTEXT_BYTES)
                        .ok_or(VaultSyncError::LimitExceeded)?;
                    let record_id = SecretId(uuid::Uuid::from_bytes(revision.record_id));
                    Some(
                        decode_vault_object(plaintext.as_slice(), record_id, &self.fingerprint)
                            .map_err(map_object_error)?,
                    )
                }
            };
            result.push(VaultHeadProjection {
                record_id: SecretId(uuid::Uuid::from_bytes(head.record_id)),
                revision_id: head.revision_id,
                revision_hash: head.revision_hash,
                parent_revision_ids: revision.parent_revision_ids,
                operation: revision.operation,
                object,
            });
        }
        Ok(result)
    }

    /// Returns the local immutable revision count for truthful progress UI.
    pub fn ledger_revision_count(&self) -> Result<usize, VaultSyncError> {
        self.ledger.revision_count().map_err(map_sync_error)
    }

    /// Creates or updates one encrypted object if its full expected head set is
    /// still current. Passing an empty set creates a new record.
    pub fn save(
        &mut self,
        record_id: SecretId,
        expected_heads: &[Id],
        title: String,
        favorite: bool,
        trashed: bool,
        content: SecretContent,
    ) -> Result<RevisionReceipt, VaultSyncError> {
        let object = VaultObject {
            record_id,
            key_fingerprint: self.fingerprint.clone(),
            title,
            favorite,
            trashed,
            content,
        };
        let plaintext = encode_vault_object(&object).map_err(map_object_error)?;
        let recipients = self.encryption_recipients()?;
        let ciphertext = self
            .crypto
            .encrypt(plaintext.as_slice(), &recipients)
            .map_err(|_| VaultSyncError::Crypto)?;
        validate_ciphertext(&ciphertext)?;
        let revision = self
            .ledger
            .append_local(
                self.identity,
                id_bytes(record_id),
                expected_heads,
                Operation::Write,
                ciphertext,
                self.trust,
            )
            .map_err(map_sync_error)?;
        receipt(&revision)
    }

    /// Adds a persistent tombstone only when at least one current head was
    /// explicitly supplied and the complete expected head set still matches.
    pub fn delete(
        &mut self,
        record_id: SecretId,
        expected_heads: &[Id],
    ) -> Result<RevisionReceipt, VaultSyncError> {
        if expected_heads.is_empty() {
            return Err(VaultSyncError::InvalidInput);
        }
        let revision = self
            .ledger
            .append_local(
                self.identity,
                id_bytes(record_id),
                expected_heads,
                Operation::Delete,
                Vec::new(),
                self.trust,
            )
            .map_err(map_sync_error)?;
        receipt(&revision)
    }

    /// Explicitly migrates an in-memory snapshot of the mutable local vault.
    /// It decrypts and preflights every item before making one atomic initial
    /// ledger insertion. The source slice is never modified.
    pub fn migrate_from(
        &mut self,
        secrets: &[Secret],
        links: &[SecretLink],
    ) -> Result<MigrationReceipt, VaultSyncError> {
        if self.ledger.revision_count().map_err(map_sync_error)? != 0 {
            return Err(VaultSyncError::DestinationNotEmpty);
        }
        if secrets.len() > SYNC_MAX_REVISIONS_PER_BATCH {
            return Err(VaultSyncError::LimitExceeded);
        }
        let recipients = self.encryption_recipients()?;

        let record_ids = secrets
            .iter()
            .map(|secret| id_bytes(secret.id))
            .collect::<BTreeSet<_>>();
        if record_ids.len() != secrets.len() {
            return Err(VaultSyncError::InvalidInput);
        }
        if secrets.iter().any(|secret| {
            secret.folder_id.is_some() || secret.icon.is_some() || secret.recipients != recipients
        }) || links.iter().any(|link| {
            record_ids.contains(&id_bytes(link.first))
                || record_ids.contains(&id_bytes(link.second))
        }) {
            return Err(VaultSyncError::MetadataNotRepresentable);
        }

        let mut objects = Vec::with_capacity(secrets.len());
        for secret in secrets {
            if secret.ciphertext.len() > SYNC_MAX_CIPHERTEXT_BYTES {
                return Err(VaultSyncError::LimitExceeded);
            }
            let plaintext = self
                .crypto
                .decrypt(&secret.ciphertext)
                .map_err(|_| VaultSyncError::Crypto)?;
            let content = decode_content(plaintext.as_slice())
                .map_err(|_| VaultSyncError::ContentNotRepresentable)?;
            if content.kind() != secret.kind {
                return Err(VaultSyncError::InvalidInput);
            }
            ensure_android_content(&content)?;
            let object = VaultObject {
                record_id: secret.id,
                key_fingerprint: self.fingerprint.clone(),
                title: secret.title.clone(),
                favorite: secret.favorite,
                trashed: secret.trashed,
                content,
            };
            // Validate the complete wrapper, including outer byte bounds, before
            // any ciphertext is inserted into the destination ledger.
            let _ = encode_vault_object(&object).map_err(map_object_error)?;
            objects.push(object);
        }

        let mut revisions = Vec::with_capacity(objects.len());
        for object in &objects {
            let plaintext = encode_vault_object(object).map_err(map_object_error)?;
            let ciphertext = self
                .crypto
                .encrypt(plaintext.as_slice(), &recipients)
                .map_err(|_| VaultSyncError::Crypto)?;
            validate_ciphertext(&ciphertext)?;
            let revision = Revision::sign(
                self.ledger.vault_id(),
                id_bytes(object.record_id),
                fresh_id().map_err(map_sync_error)?,
                Vec::new(),
                Operation::Write,
                ciphertext,
                self.identity,
            )
            .map_err(map_sync_error)?;
            revisions.push(revision);
        }

        self.ledger
            .insert_initial_batch(&revisions, self.trust)
            .map_err(map_sync_error)?;
        Ok(MigrationReceipt {
            imported_records: revisions.len(),
        })
    }

    fn encryption_recipients(&self) -> Result<RecipientSet, VaultSyncError> {
        RecipientSet::new(vec![self.fingerprint.clone()]).map_err(|_| VaultSyncError::InvalidInput)
    }
}

fn ensure_android_content(content: &SecretContent) -> Result<(), VaultSyncError> {
    match content {
        SecretContent::Login(login) => {
            if login.totp.is_some()
                || !login.history.events().is_empty()
                || !login.history.version_metadata().is_empty()
                || login.urls.len() > 64
                || login.urls.iter().any(|url| url.len() > 4096)
                || login.username.len() > 65_536
                || login.password.len() > 65_536
                || login.notes.len() > 256 * 1024
            {
                return Err(VaultSyncError::ContentNotRepresentable);
            }
        }
        SecretContent::Note(note) => {
            if note.value.len() > 512 * 1024 || note.notes.len() > 256 * 1024 {
                return Err(VaultSyncError::ContentNotRepresentable);
            }
        }
        SecretContent::Custom(_)
        | SecretContent::Card(_)
        | SecretContent::Totp(_)
        | SecretContent::Identity(_)
        | SecretContent::Wifi(_)
        | SecretContent::License(_)
        | SecretContent::SshKey(_) => return Err(VaultSyncError::ContentNotRepresentable),
    }
    Ok(())
}

fn validate_ciphertext(ciphertext: &[u8]) -> Result<(), VaultSyncError> {
    if ciphertext.is_empty() {
        return Err(VaultSyncError::Crypto);
    }
    if ciphertext.len() > SYNC_MAX_CIPHERTEXT_BYTES {
        return Err(VaultSyncError::LimitExceeded);
    }
    Ok(())
}

fn id_bytes(id: SecretId) -> Id {
    *id.0.as_bytes()
}

fn receipt(revision: &Revision) -> Result<RevisionReceipt, VaultSyncError> {
    Ok(RevisionReceipt {
        record_id: revision.record_id,
        revision_id: revision.revision_id,
        revision_hash: revision.hash().map_err(map_sync_error)?,
        operation: revision.operation,
    })
}

fn map_object_error(error: kryptopass_core::VaultObjectError) -> VaultSyncError {
    match error {
        kryptopass_core::VaultObjectError::LimitExceeded => VaultSyncError::LimitExceeded,
        kryptopass_core::VaultObjectError::Invalid => VaultSyncError::InvalidObject,
    }
}

fn map_sync_error(error: SyncError) -> VaultSyncError {
    match error {
        SyncError::HeadConflict => VaultSyncError::Conflict,
        SyncError::LedgerNotEmpty => VaultSyncError::DestinationNotEmpty,
        SyncError::LimitExceeded | SyncError::Allocation => VaultSyncError::LimitExceeded,
        SyncError::UnknownAuthor | SyncError::InvalidSignature => VaultSyncError::Trust,
        SyncError::Randomness => VaultSyncError::Crypto,
        _ => VaultSyncError::Ledger,
    }
}
