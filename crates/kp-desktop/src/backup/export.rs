//! Snapshot export into a bounded OpenPGP container.

use std::path::Path;

use anyhow::{Result, ensure};
use kryptopass_core::{OpenPgp, RecipientSet};

use crate::infrastructure::GnuPgOpenPgp;
use crate::storage::SqliteStorage;

use super::filesystem::{prepare_destination, protect_store_target, write_backup_atomically};
use super::format::serialize_backup_with_links;
use super::recipients::MAX_SELECTED_RECIPIENTS;
use super::{BackupPublishOutcome, MAX_BACKUP_CIPHERTEXT_BYTES, MAX_BACKUP_PLAINTEXT_BYTES};

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

    let database_path = crate::storage::paths::database_path(crate::app_info::APP.desktop_file_id)?;
    let destination = prepare_destination(path, true)?;
    protect_store_target(&database_path, &destination)?;

    let storage = SqliteStorage::open_existing(database_path)?;
    let (folders, secrets, assets, links) = storage.backup_snapshot_with_links()?;
    let plaintext = serialize_backup_with_links(folders, secrets, assets, links)?;
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
