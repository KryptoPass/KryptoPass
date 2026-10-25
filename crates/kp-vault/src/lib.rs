//! Vault application service: shared encrypted objects and atomic signed revisions.
//! No Qt, network transport, or GnuPG runtime is required; crypto is injected.

use kryptopass_core::{
    IconState, KeyFingerprint, MAX_LEDGER_ASSET_CIPHERTEXT_BYTES, MAX_LEDGER_ASSETS,
    MAX_VAULT_ASSET_BYTES, MAX_VAULT_OBJECT_BYTES, OpenPgp, RecipientSet, SecretContent, SecretId,
    VaultObject, asset_record_id, decode_vault_asset, decode_vault_object, encode_vault_asset,
    encode_vault_object, is_asset_record_id, png_sha256,
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

/// Result of publishing one image asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetWrite {
    /// The asset was appended to the ledger.
    Written(RevisionReceipt),
    /// The ledger already holds this exact asset; nothing was written.
    Present,
    /// Adding it would exceed the ledger's asset budget; nothing was written.
    OverBudget,
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
            // Image assets are separate records, read on demand by hash.
            if is_asset_record_id(&uuid::Uuid::from_bytes(head.record_id)) {
                continue;
            }
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
    #[allow(clippy::too_many_arguments)]
    pub fn save(
        &mut self,
        record_id: SecretId,
        expected_heads: &[Id],
        title: String,
        favorite: bool,
        trashed: bool,
        folder: Vec<String>,
        icon: IconState,
        content: SecretContent,
    ) -> Result<RevisionReceipt, VaultSyncError> {
        if is_asset_record_id(&record_id.0) {
            return Err(VaultSyncError::InvalidInput);
        }
        let object = VaultObject {
            record_id,
            key_fingerprint: self.fingerprint.clone(),
            title,
            favorite,
            trashed,
            folder,
            icon,
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

    /// Publishes one normalized PNG as an immutable asset record, unless the
    /// ledger already holds it. A tombstoned asset is written again over its
    /// tombstone; the content is addressed by hash, so this is always safe.
    pub fn write_asset(&mut self, png: &[u8]) -> Result<AssetWrite, VaultSyncError> {
        let sha256 = png_sha256(png);
        let record_id = asset_record_id(&self.ledger.vault_id(), &sha256);
        let heads = self
            .ledger
            .heads(*record_id.as_bytes())
            .map_err(map_sync_error)?;
        let mut tombstones = Vec::with_capacity(heads.len());
        for head in &heads {
            let revision = self
                .ledger
                .get(head.revision_id)
                .map_err(map_sync_error)?
                .ok_or(VaultSyncError::Ledger)?;
            if revision.operation == Operation::Write {
                return Ok(AssetWrite::Present);
            }
            tombstones.push(head.revision_id);
        }
        let plaintext = encode_vault_asset(png).map_err(|error| match error {
            kryptopass_core::VaultAssetError::LimitExceeded => VaultSyncError::LimitExceeded,
            kryptopass_core::VaultAssetError::Invalid => VaultSyncError::InvalidObject,
        })?;
        let recipients = self.encryption_recipients()?;
        let ciphertext = self
            .crypto
            .encrypt(&plaintext, &recipients)
            .map_err(|_| VaultSyncError::Crypto)?;
        validate_ciphertext(&ciphertext)?;
        let (count, bytes) = self.asset_usage()?;
        if count >= MAX_LEDGER_ASSETS
            || bytes.saturating_add(ciphertext.len()) > MAX_LEDGER_ASSET_CIPHERTEXT_BYTES
        {
            return Ok(AssetWrite::OverBudget);
        }
        let revision = self
            .ledger
            .append_local(
                self.identity,
                *record_id.as_bytes(),
                &tombstones,
                Operation::Write,
                ciphertext,
                self.trust,
            )
            .map_err(map_sync_error)?;
        receipt(&revision).map(AssetWrite::Written)
    }

    /// Reads the asset with this hash; None while the ledger holds no copy.
    pub fn read_asset(&self, sha256: &[u8; 32]) -> Result<Option<Vec<u8>>, VaultSyncError> {
        read_vault_asset(self.ledger, self.crypto, sha256)
    }

    /// Live assets and their total ciphertext size.
    fn asset_usage(&self) -> Result<(usize, usize), VaultSyncError> {
        let mut count = 0_usize;
        let mut bytes = 0_usize;
        for head in self.ledger.manifest().map_err(map_sync_error)? {
            if !is_asset_record_id(&uuid::Uuid::from_bytes(head.record_id)) {
                continue;
            }
            let revision = self
                .ledger
                .get(head.revision_id)
                .map_err(map_sync_error)?
                .ok_or(VaultSyncError::Ledger)?;
            if revision.operation == Operation::Write {
                count += 1;
                bytes = bytes.saturating_add(revision.ciphertext.len());
            }
        }
        Ok((count, bytes))
    }

    fn encryption_recipients(&self) -> Result<RecipientSet, VaultSyncError> {
        RecipientSet::new(vec![self.fingerprint.clone()]).map_err(|_| VaultSyncError::InvalidInput)
    }
}

/// Reads and fully validates the asset with this hash from `ledger`. Returns
/// None while no write head holds it; a head that fails validation is skipped.
pub fn read_vault_asset(
    ledger: &Ledger,
    crypto: &dyn OpenPgp,
    sha256: &[u8; 32],
) -> Result<Option<Vec<u8>>, VaultSyncError> {
    let record_id = asset_record_id(&ledger.vault_id(), sha256);
    let mut last_error = None;
    for head in ledger
        .heads(*record_id.as_bytes())
        .map_err(map_sync_error)?
    {
        let revision = ledger
            .get(head.revision_id)
            .map_err(map_sync_error)?
            .ok_or(VaultSyncError::Ledger)?;
        if revision.operation != Operation::Write {
            continue;
        }
        let plaintext = match crypto.decrypt(&revision.ciphertext) {
            Ok(plaintext) => plaintext,
            Err(_) => {
                last_error = Some(VaultSyncError::Crypto);
                continue;
            }
        };
        if plaintext.len() > MAX_VAULT_ASSET_BYTES {
            last_error = Some(VaultSyncError::LimitExceeded);
            continue;
        }
        match decode_vault_asset(plaintext.as_slice(), sha256) {
            Ok(asset) => return Ok(Some(asset.png)),
            Err(_) => last_error = Some(VaultSyncError::InvalidObject),
        }
    }
    last_error.map_or(Ok(None), Err)
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
