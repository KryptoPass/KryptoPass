//! Record invariants and bounded validation for backups and additive imports.

use crate::backup::{MAX_BACKUP_PLAINTEXT_BYTES, MAX_BACKUP_RECIPIENTS, MAX_BACKUP_ROWS};
use crate::infrastructure::MAX_CIPHERTEXT_BYTES;
use anyhow::{Context, Result, ensure};
use kryptopass_core::{Folder, Secret};
use rusqlite::Connection;
use std::collections::HashSet;

pub(super) fn validate_folder(folder: &Folder) -> Result<()> {
    ensure!(folder.sort_position >= 0, "folder order is invalid");
    ensure!(
        !folder.name.trim().is_empty(),
        "folder name cannot be empty"
    );
    Ok(())
}

pub(super) fn validate_secret(secret: &Secret) -> Result<()> {
    if let Some(icon) = &secret.icon {
        icon.validate()?;
    }
    ensure!(
        !secret.title.trim().is_empty(),
        "secret title cannot be empty"
    );
    ensure!(
        !secret.ciphertext.is_empty(),
        "encrypted payload cannot be empty"
    );
    ensure!(
        secret.ciphertext.len() <= MAX_CIPHERTEXT_BYTES,
        "encrypted payload exceeds the maximum size"
    );
    ensure!(
        !secret.recipients.as_slice().is_empty(),
        "at least one recipient is required"
    );
    Ok(())
}

pub(super) fn validate_backup_read_budget(connection: &Connection) -> Result<()> {
    let (rows, recipients, bytes): (i64, i64, i64) = connection
        .query_row(
            "SELECT
                 (SELECT COUNT(*) FROM folders) + (SELECT COUNT(*) FROM secrets),
                 (SELECT COUNT(*) FROM folder_recipients) + (SELECT COUNT(*) FROM secret_recipients),
                 (SELECT COALESCE(SUM(length(CAST(name AS BLOB)) + length(CAST(icon AS BLOB)) + length(CAST(color AS BLOB))), 0) FROM folders)
                 + (SELECT COALESCE(SUM(length(CAST(title AS BLOB)) + length(CAST(kind AS BLOB)) + length(ciphertext)), 0) FROM secrets)
                 + (SELECT COALESCE(SUM(length(CAST(fingerprint AS BLOB))), 0) FROM folder_recipients)
                 + (SELECT COALESCE(SUM(length(CAST(fingerprint AS BLOB))), 0) FROM secret_recipients)
                 + (SELECT COALESCE(SUM(length(png)), 0) FROM entry_icon_assets)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .context("could not measure backup size")?;
    ensure!(
        rows >= 0 && rows as usize <= MAX_BACKUP_ROWS,
        "store contains too many records for a backup"
    );
    ensure!(
        recipients >= 0 && recipients as usize <= MAX_BACKUP_RECIPIENTS,
        "store contains too many recipient records for a backup"
    );
    ensure!(
        bytes >= 0 && bytes as u64 <= MAX_BACKUP_PLAINTEXT_BYTES as u64,
        "store exceeds the maximum backup payload size"
    );
    Ok(())
}

pub(super) fn validate_backup_snapshot(folders: &[Folder], secrets: &[Secret]) -> Result<()> {
    validate_encrypted_batch(folders, secrets, false)
}

pub(super) fn validate_encrypted_batch(
    folders: &[Folder],
    secrets: &[Secret],
    allow_existing_folders: bool,
) -> Result<()> {
    ensure!(
        folders.len().saturating_add(secrets.len()) <= MAX_BACKUP_ROWS,
        "backup contains too many records"
    );

    let folder_ids = folders
        .iter()
        .map(|folder| folder.id)
        .collect::<HashSet<_>>();
    ensure!(
        folder_ids.len() == folders.len(),
        "backup has duplicate folder ids"
    );
    let secret_ids = secrets
        .iter()
        .map(|secret| secret.id)
        .collect::<HashSet<_>>();
    ensure!(
        secret_ids.len() == secrets.len(),
        "backup has duplicate secret ids"
    );

    let mut recipients = 0usize;
    let mut bytes = 0usize;
    for folder in folders {
        validate_folder(folder)?;
        ensure!(
            folder.name.len() <= 256,
            "backup folder name exceeds the maximum size"
        );
        if let Some(parent_id) = folder.parent_id {
            ensure!(
                allow_existing_folders || folder_ids.contains(&parent_id),
                "backup folder parent is missing"
            );
        }
        if let Some(default_recipients) = &folder.default_recipients {
            ensure!(
                !default_recipients.as_slice().is_empty(),
                "backup folder recipients are invalid"
            );
            recipients = recipients.saturating_add(default_recipients.as_slice().len());
            bytes = bytes.saturating_add(
                default_recipients
                    .as_slice()
                    .iter()
                    .map(|fingerprint| fingerprint.as_str().len())
                    .sum::<usize>(),
            );
        }
        bytes = bytes
            .saturating_add(folder.name.len())
            .saturating_add(folder.appearance.icon().len())
            .saturating_add(folder.appearance.color().len());
    }

    for secret in secrets {
        validate_secret(secret)?;
        ensure!(
            secret.title.len() <= 512,
            "backup secret title exceeds the maximum size"
        );
        ensure!(
            secret.revision <= i64::MAX as u64,
            "backup secret revision is invalid"
        );
        if let Some(folder_id) = secret.folder_id {
            ensure!(
                allow_existing_folders || folder_ids.contains(&folder_id),
                "backup secret folder is missing"
            );
        }
        recipients = recipients.saturating_add(secret.recipients.as_slice().len());
        bytes = bytes
            .saturating_add(secret.title.len())
            .saturating_add(secret.kind.as_str().len())
            .saturating_add(secret.ciphertext.len())
            .saturating_add(
                secret
                    .recipients
                    .as_slice()
                    .iter()
                    .map(|fingerprint| fingerprint.as_str().len())
                    .sum::<usize>(),
            );
    }
    ensure!(
        recipients <= MAX_BACKUP_RECIPIENTS,
        "backup contains too many recipients"
    );
    ensure!(
        bytes <= MAX_BACKUP_PLAINTEXT_BYTES,
        "backup exceeds the maximum payload size"
    );
    Ok(())
}
