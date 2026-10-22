//! Consistent metadata snapshots, encrypted backup restore, and additive imports.

use super::SqliteStorage;
use super::folders::{ensure_folder_exists, validate_parent};
use super::icons::{insert_icon_asset, read_icon_assets, set_secret_icon};
use super::links::{insert_imported_secret_links, read_secret_links, validate_backup_links};
use super::recipients::{replace_folder_recipients, replace_secret_recipients};
use super::records::{read_folders, read_secret_metadata, read_secrets};
use super::validation::{
    validate_backup_read_budget, validate_backup_snapshot, validate_encrypted_batch,
};
use anyhow::{Context, Result, ensure};
use kryptopass_core::{EntryIconAsset, Folder, ImportStorage, Secret, SecretLink, SecretMetadata};
use rusqlite::{Transaction, TransactionBehavior, params};
use std::collections::HashMap;

pub(crate) type EncryptedStoreSnapshot = (
    Vec<Folder>,
    Vec<Secret>,
    Vec<EntryIconAsset>,
    Vec<SecretLink>,
);

impl SqliteStorage {
    /// Reads folder and secret metadata from the same SQLite snapshot.
    #[allow(dead_code)] // Kept as the original metadata-only snapshot API.
    pub fn read_snapshot(&self) -> Result<(Vec<Folder>, Vec<SecretMetadata>)> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let folders = read_folders(&transaction)?;
        let secrets = read_secret_metadata(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading store metadata")?;
        Ok((folders, secrets))
    }

    /// Reads navigation metadata and public pair relationships from one SQLite
    /// snapshot without loading encrypted payloads.
    pub fn read_snapshot_with_links(
        &self,
    ) -> Result<(Vec<Folder>, Vec<SecretMetadata>, Vec<SecretLink>)> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let folders = read_folders(&transaction)?;
        let secrets = read_secret_metadata(&transaction)?;
        let links = read_secret_links(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading store metadata")?;
        Ok((folders, secrets, links))
    }

    /// Reads the complete encrypted store under one bounded SQLite snapshot.
    #[cfg(test)]
    pub(crate) fn backup_snapshot(
        &self,
    ) -> Result<(Vec<Folder>, Vec<Secret>, Vec<EntryIconAsset>)> {
        let (folders, secrets, assets, _) = self.backup_snapshot_with_links()?;
        Ok((folders, secrets, assets))
    }

    /// Reads encrypted records, local icon assets, and public pair metadata
    /// under one bounded SQLite snapshot.
    pub(crate) fn backup_snapshot_with_links(&self) -> Result<EncryptedStoreSnapshot> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store for backup")?;
        validate_backup_read_budget(&transaction)?;
        let folders = read_folders(&transaction)?;
        let secrets = read_secrets(&transaction)?;
        let assets = read_icon_assets(&transaction)?;
        let links = read_secret_links(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading the encrypted backup")?;
        Ok((folders, secrets, assets, links))
    }

    /// Imports a validated backup into a newly created, empty store atomically.
    #[cfg(test)]
    pub(crate) fn import_backup_snapshot(
        &mut self,
        folders: &[Folder],
        secrets: &[Secret],
        assets: &[EntryIconAsset],
    ) -> Result<()> {
        self.import_backup_snapshot_with_links(folders, secrets, assets, &[])
    }

    /// Restores a complete backup snapshot and all public relationships in one
    /// immediate transaction.
    pub(crate) fn import_backup_snapshot_with_links(
        &mut self,
        folders: &[Folder],
        secrets: &[Secret],
        assets: &[EntryIconAsset],
        links: &[SecretLink],
    ) -> Result<()> {
        validate_backup_snapshot(folders, secrets)?;
        validate_backup_links(links, secrets)?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin backup restore")?;
        let nonempty: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM folders) OR EXISTS(SELECT 1 FROM secrets) \
                 OR EXISTS(SELECT 1 FROM folder_recipients) OR EXISTS(SELECT 1 FROM secret_recipients) \
                 OR EXISTS(SELECT 1 FROM entry_icon_assets) OR EXISTS(SELECT 1 FROM secret_links)",
                [],
                |row| row.get(0),
            )
            .context("could not inspect the restore destination")?;
        ensure!(!nonempty, "backup restore destination is not empty");

        for asset in assets {
            insert_icon_asset(&transaction, asset)?;
        }
        insert_encrypted_batch(&transaction, folders, secrets)?;
        insert_imported_secret_links(&transaction, links)?;

        transaction
            .commit()
            .context("could not finish restoring the encrypted store")
    }

    /// Checkpoints and closes an imported store before its database file is published.
    pub(crate) fn finish_for_backup_publish(self) -> Result<()> {
        let checkpoint: (i64, i64, i64) = self
            .connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .context("could not checkpoint the restored store")?;
        ensure!(checkpoint.0 == 0, "could not checkpoint the restored store");
        self.connection
            .pragma_update(None, "journal_mode", "DELETE")
            .context("could not finalize the restored store")?;
        self.connection
            .pragma_update(None, "synchronous", "FULL")
            .context("could not configure the restored store")?;
        self.connection
            .close()
            .map_err(|_| anyhow::anyhow!("could not close the restored store"))
    }
}

fn insert_encrypted_batch(
    transaction: &Transaction<'_>,
    folders: &[Folder],
    secrets: &[Secret],
) -> Result<()> {
    // Insert folders as roots first so input ordering does not matter.
    for folder in folders {
        transaction
            .execute(
                "INSERT INTO folders(id, parent_id, name, icon, color, sort_position) \
                     VALUES (?1, NULL, ?2, ?3, ?4, ?5)",
                params![
                    folder.id.0.to_string(),
                    &folder.name,
                    folder.appearance.icon(),
                    folder.appearance.color(),
                    folder.sort_position,
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not restore folder metadata"))?;
        replace_folder_recipients(transaction, folder.id, folder.default_recipients.as_ref())?;
    }

    // Reuse the storage hierarchy check; every link update is rolled back
    // if a missing parent or cycle is encountered.
    for folder in folders {
        if let Some(parent_id) = folder.parent_id {
            validate_parent(transaction, folder.id, Some(parent_id))?;
            transaction
                .execute(
                    "UPDATE folders SET parent_id = ?1 WHERE id = ?2",
                    params![parent_id.0.to_string(), folder.id.0.to_string()],
                )
                .map_err(|_| anyhow::anyhow!("could not restore folder hierarchy"))?;
        }
    }

    for secret in secrets {
        if let Some(folder_id) = secret.folder_id {
            ensure_folder_exists(transaction, folder_id)?;
        }
        transaction
                .execute(
                    "INSERT INTO secrets(id, folder_id, title, kind, favorite, trashed, ciphertext, revision) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        secret.id.0.to_string(),
                        secret.folder_id.map(|id| id.0.to_string()),
                        &secret.title,
                        secret.kind.as_str(),
                        secret.favorite,
                        secret.trashed,
                        &secret.ciphertext,
                        i64::try_from(secret.revision)
                            .context("backup secret revision is invalid")?,
                    ],
                )
                .map_err(|_| anyhow::anyhow!("could not restore encrypted secret"))?;
        replace_secret_recipients(transaction, secret.id, &secret.recipients)?;
        set_secret_icon(transaction, secret.id, secret.icon.as_ref())?;
    }

    Ok(())
}

impl ImportStorage for SqliteStorage {
    fn append_import(
        &mut self,
        folders: &[Folder],
        secrets: &[Secret],
        links: &[SecretLink],
    ) -> Result<()> {
        validate_encrypted_batch(folders, secrets, true)?;
        validate_backup_links(links, secrets)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin additive import")?;
        let mut next_positions = HashMap::new();
        let mut ordered = folders.to_vec();
        for folder in &mut ordered {
            let next = match next_positions.entry(folder.parent_id) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let maximum: i64 = transaction.query_row(
                        "SELECT COALESCE(MAX(sort_position), -1) FROM folders WHERE parent_id IS ?1",
                        [folder.parent_id.map(|id| id.0.to_string())], |row| row.get(0),
                    )?;
                    entry.insert(maximum.checked_add(1).context("folder order overflow")?)
                }
            };
            folder.sort_position = *next;
            *next = next.checked_add(1).context("folder order overflow")?;
        }
        insert_encrypted_batch(&transaction, &ordered, secrets)?;
        insert_imported_secret_links(&transaction, links)?;
        transaction
            .commit()
            .context("could not commit additive import")
    }
}
