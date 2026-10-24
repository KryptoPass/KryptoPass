//! Vault application service: shared encrypted objects and atomic signed revisions.
//! No Qt, network transport, or GnuPG runtime is required; crypto is injected.

use kryptopass_core::{
    KeyFingerprint, MAX_VAULT_OBJECT_BYTES, OpenPgp, RecipientSet, SecretContent, SecretId,
    VaultObject, decode_vault_object, encode_vault_object,
};
use kryptopass_identity::DeviceIdentity;
use kryptopass_sync::{
    Digest, Id, Ledger, Operation, Revision, SYNC_MAX_CIPHERTEXT_BYTES, SyncError, TrustSource,
};
use std::fmt;

const MAX_PROJECTED_PLAINTEXT_BYTES: usize = 8 * 1024 * 1024;

/// Payload-free service failures suitable for mapping to fixed UI messages.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VaultSyncError {
    InvalidInput,
    InvalidObject,
    Crypto,
    Trust,
    Conflict,
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
            Self::Crypto => "vault_sync_crypto_failed",
            Self::Trust => "vault_sync_trust_failed",
            Self::Conflict => "vault_sync_heads_changed",
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

    fn encryption_recipients(&self) -> Result<RecipientSet, VaultSyncError> {
        RecipientSet::new(vec![self.fingerprint.clone()]).map_err(|_| VaultSyncError::InvalidInput)
    }
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
        SyncError::LimitExceeded | SyncError::Allocation => VaultSyncError::LimitExceeded,
        SyncError::UnknownAuthor | SyncError::InvalidSignature => VaultSyncError::Trust,
        SyncError::Randomness => VaultSyncError::Crypto,
        _ => VaultSyncError::Ledger,
    }
}
