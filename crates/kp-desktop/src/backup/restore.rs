//! Validated restore into an isolated store with atomic publication.

use std::fmt;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use kryptopass_core::{EntryIconAsset, Folder, Secret, SecretLink};

use crate::infrastructure::GnuPgOpenPgp;
use crate::storage::SqliteStorage;

use super::filesystem::{
    create_private_staging_directory, ensure_private_directory, prepare_destination,
    prepare_existing_file, protect_store_target, read_bounded_file, sync_directory,
};
use super::format::deserialize_backup_with_links;
use super::{BackupPublishOutcome, MAX_BACKUP_CIPHERTEXT_BYTES, MAX_BACKUP_PLAINTEXT_BYTES};

const RESTORE_DATABASE_NAME: &str = "restored.sqlite3";

/// Recovery categories exposed to the UI without decrypted data or GnuPG output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreFailure {
    Input,
    Destination,
    Decryption,
    InvalidBackup,
}

impl fmt::Display for RestoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "restore failed: {self:?}")
    }
}

impl std::error::Error for RestoreFailure {}

/// Decrypts and validates a backup, then publishes it as a new private SQLite store.
pub fn restore_backup(
    input: &Path,
    output: &Path,
    crypto: &GnuPgOpenPgp,
) -> Result<BackupPublishOutcome> {
    let database_path = crate::storage::paths::database_path(crate::app_info::APP.desktop_file_id)?;
    let input = prepare_existing_file(input).context(RestoreFailure::Input)?;
    let destination = prepare_destination(output, false).context(RestoreFailure::Destination)?;
    protect_store_target(&database_path, &destination).context(RestoreFailure::Destination)?;
    ensure!(
        !destination.exists(),
        "restore output already exists; choose a new vault file"
    );
    let parent = destination
        .parent()
        .context("restore destination has no parent")?;
    ensure_private_directory(parent).context(RestoreFailure::Destination)?;

    let ciphertext =
        read_bounded_file(&input, MAX_BACKUP_CIPHERTEXT_BYTES).context(RestoreFailure::Input)?;
    let plaintext = crypto
        .decrypt_bounded(
            &ciphertext,
            MAX_BACKUP_CIPHERTEXT_BYTES,
            MAX_BACKUP_PLAINTEXT_BYTES,
        )
        .context(RestoreFailure::Decryption)?;
    let (folders, secrets, assets, links) = deserialize_backup_with_links(plaintext.as_slice())
        .context(RestoreFailure::InvalidBackup)?;

    publish_restore_snapshot(&destination, &folders, &secrets, &assets, &links)
}

/// Publishes a restored snapshot into an isolated, new private store.
fn publish_restore_snapshot(
    output: &Path,
    folders: &[Folder],
    secrets: &[Secret],
    assets: &[EntryIconAsset],
    links: &[SecretLink],
) -> Result<BackupPublishOutcome> {
    let database_path = crate::storage::paths::database_path(crate::app_info::APP.desktop_file_id)?;
    let destination = prepare_destination(output, false)?;
    protect_store_target(&database_path, &destination)?;
    ensure!(!destination.exists(), "import destination already exists");
    let parent = destination
        .parent()
        .context("import destination has no parent")?;
    ensure_private_directory(parent)?;
    let stage = create_private_staging_directory(parent)?;
    let mut stage_guard = StagingDirectory(stage.clone());
    let staged_database = stage.join(RESTORE_DATABASE_NAME);
    let mut storage = SqliteStorage::open(&staged_database)?;
    storage.import_backup_snapshot_with_links(folders, secrets, assets, links)?;
    storage.finish_for_backup_publish()?;
    File::open(&staged_database)
        .and_then(|file| file.sync_all())
        .context("could not sync the restored vault")?;

    // A hard link publishes the fully closed database atomically and fails if
    // another process created the selected destination in the meantime.
    fs::hard_link(&staged_database, &destination)
        .context("could not publish the restored vault")?;
    let directory_synced = sync_directory(parent).is_ok();
    let database_removed = fs::remove_file(&staged_database).is_ok();
    let stage_removed = fs::remove_dir(&stage).is_ok();
    if database_removed && stage_removed {
        stage_guard.disarm();
    }
    Ok(if directory_synced && database_removed && stage_removed {
        BackupPublishOutcome::Durable
    } else {
        BackupPublishOutcome::PublishedButDurabilityUnconfirmed
    })
}

struct StagingDirectory(PathBuf);

impl StagingDirectory {
    fn disarm(&mut self) {
        self.0.clear();
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
