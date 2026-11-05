//! Snapshot export into a bounded OpenPGP container.

use std::path::Path;

use anyhow::{Context, Result, ensure};
use kryptopass_core::RecipientSet;
use zeroize::Zeroizing;

use crate::infrastructure::GnuPgOpenPgp;

use super::filesystem::{
    create_private_staging_directory, prepare_destination, protect_store_target,
    write_backup_atomically,
};
use super::recipients::MAX_SELECTED_RECIPIENTS;
use super::{
    BACKUP_MAGIC, BackupPublishOutcome, MAX_BACKUP_CIPHERTEXT_BYTES, MAX_BACKUP_PLAINTEXT_BYTES,
};

/// Writes a full store snapshot as one encrypted OpenPGP file.
pub fn export_backup(
    path: &Path,
    recipients: RecipientSet,
    crypto: &GnuPgOpenPgp,
) -> Result<BackupPublishOutcome> {
    ensure!(
        recipients.as_slice().len() <= MAX_SELECTED_RECIPIENTS,
        "too many backup recipients were selected"
    );
    crypto.validate_recipients(&recipients)?;

    let database_path = crate::vault_access::vault_path()?;
    let destination = prepare_destination(path, true)?;
    protect_store_target(&database_path, &destination)?;

    let plaintext = snapshot_bytes(&database_path)?;
    let ciphertext = crypto.encrypt_bounded(
        plaintext.as_slice(),
        &recipients,
        MAX_BACKUP_PLAINTEXT_BYTES,
        MAX_BACKUP_CIPHERTEXT_BYTES,
    )?;
    ensure!(
        !ciphertext.is_empty() && ciphertext.len() <= MAX_BACKUP_CIPHERTEXT_BYTES,
        "encrypted backup exceeds the maximum size"
    );
    write_backup_atomically(&destination, &ciphertext)
}

/// The marked bytes of a consistent copy, made beside the vault in a private
/// directory that is removed before returning.
fn snapshot_bytes(database: &Path) -> Result<Zeroizing<Vec<u8>>> {
    let parent = database
        .parent()
        .context("the vault has no parent directory")?;
    let stage = create_private_staging_directory(parent)?;
    let copy = stage.join("backup.sqlite");
    let result = (|| -> Result<Zeroizing<Vec<u8>>> {
        kryptopass_store::SqliteStore::open_existing(database, crate::vault_access::identity()?)?
            .export_copy(&copy)?;
        let bytes = Zeroizing::new(std::fs::read(&copy).context("could not read the vault copy")?);
        ensure!(
            bytes.len() + BACKUP_MAGIC.len() <= MAX_BACKUP_PLAINTEXT_BYTES,
            "the vault exceeds the maximum backup size"
        );
        let mut plaintext = Zeroizing::new(Vec::with_capacity(bytes.len() + BACKUP_MAGIC.len()));
        plaintext.extend_from_slice(BACKUP_MAGIC);
        plaintext.extend_from_slice(&bytes);
        Ok(plaintext)
    })();
    if let Err(error) = std::fs::remove_dir_all(&stage) {
        tracing::warn!(%error, "the backup staging directory was not removed");
    }
    result
}
