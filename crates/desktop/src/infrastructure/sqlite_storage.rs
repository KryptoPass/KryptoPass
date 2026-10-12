//! SQLite storage for encrypted secret payloads and public organizing metadata.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use kryptopass_core::{
    DomainError, EntryIcon, EntryIconAsset, Folder, FolderAppearance, FolderId, ImportStorage,
    KeyFingerprint, MAX_ENTRY_ICON_BYTES, RecipientSet, Secret, SecretId, SecretKind, SecretLink,
    SecretLinkError, SecretMetadata, Storage,
};
use rusqlite::types::{Type, ValueRef};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Row, Transaction, TransactionBehavior, params,
};
use uuid::Uuid;

use super::MAX_CIPHERTEXT_BYTES;
use super::backup::{MAX_BACKUP_PLAINTEXT_BYTES, MAX_BACKUP_RECIPIENTS, MAX_BACKUP_ROWS};

const SCHEMA_VERSION: i64 = 5;
const APPLICATION_ID: i64 = 0x4b50_5331;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) type EncryptedStoreSnapshot = (
    Vec<Folder>,
    Vec<Secret>,
    Vec<EntryIconAsset>,
    Vec<SecretLink>,
);

/// Persistent storage whose secret payload column contains ciphertext only.
pub struct SqliteStorage {
    connection: Connection,
}

impl SqliteStorage {
    /// Opens the encrypted store, creating its schema if this is a new file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_internal(path.as_ref(), true)
    }

    /// Opens an existing encrypted store without creating the file or parent.
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_internal(path.as_ref(), false)
    }

    fn open_internal(path: &Path, create: bool) -> Result<Self> {
        if create {
            ensure_database_parent(path)?;
        } else {
            ensure_existing_database_parent(path)?;
        }
        ensure_database_file_is_safe(path)?;
        if !create {
            prevalidate_existing_database(path)?;
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let flags = if create {
            flags | OpenFlags::SQLITE_OPEN_CREATE
        } else {
            flags
        };
        let mut connection = Connection::open_with_flags(path, flags)
            .map_err(|_| anyhow::anyhow!("could not open the encrypted store"))?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .context("could not configure the encrypted store")?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA synchronous = FULL;
                 PRAGMA trusted_schema = OFF;
                 PRAGMA secure_delete = ON;",
            )
            .map_err(|_| anyhow::anyhow!("could not configure the encrypted store"))?;
        set_private_database_permissions(path)?;
        initialize_schema(&mut connection)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|_| anyhow::anyhow!("could not enable durable store journaling"))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|_| anyhow::anyhow!("could not configure durable store journaling"))?;

        Ok(Self { connection })
    }

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

    fn insert_secret_record(
        &mut self,
        secret: Secret,
        asset: Option<EntryIconAsset>,
        relationship: Option<Option<SecretId>>,
    ) -> Result<()> {
        validate_secret(&secret)?;
        ensure!(
            secret.revision == 0,
            "a new secret must start at revision zero"
        );

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret update")?;
        if let Some(asset) = &asset {
            ensure!(
                secret.icon == Some(EntryIcon::Image { asset_id: asset.id }),
                "icon resource does not match secret"
            );
            insert_icon_asset(&transaction, asset)?;
        }
        transaction
            .execute(
                "INSERT INTO secrets(id, folder_id, title, kind, favorite, trashed, ciphertext, revision) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                params![
                    secret.id.0.to_string(),
                    secret.folder_id.map(|id| id.0.to_string()),
                    secret.title,
                    secret.kind.as_str(),
                    secret.favorite,
                    secret.trashed,
                    secret.ciphertext,
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not save secret"))?;
        replace_secret_recipients(&transaction, secret.id, &secret.recipients)?;
        set_secret_icon(&transaction, secret.id, secret.icon.as_ref())?;
        if let Some(related) = relationship {
            apply_secret_link_change(&transaction, secret.id, related)?;
        }
        collect_unused_icons(&transaction)?;
        transaction
            .commit()
            .context("could not finish secret update")
    }

    fn replace_secret_record(
        &mut self,
        secret: Secret,
        expected_revision: u64,
        asset: Option<EntryIconAsset>,
        relationship: Option<Option<SecretId>>,
    ) -> Result<()> {
        validate_secret(&secret)?;
        ensure!(
            secret.revision == expected_revision,
            "secret revision does not match the expected revision",
        );
        ensure!(
            expected_revision < i64::MAX as u64,
            "secret revision is exhausted",
        );

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret update")?;
        if let Some(folder_id) = secret.folder_id {
            ensure_folder_exists(&transaction, folder_id)?;
        }

        if let Some(asset) = &asset {
            ensure!(
                secret.icon == Some(EntryIcon::Image { asset_id: asset.id }),
                "icon resource does not match secret"
            );
            insert_icon_asset(&transaction, asset)?;
        }
        let changed = transaction
            .execute(
                "UPDATE secrets \
                 SET folder_id = ?1, title = ?2, kind = ?3, favorite = ?4, trashed = ?5, \
                     ciphertext = ?6, revision = revision + 1 \
                 WHERE id = ?7 AND revision = ?8",
                params![
                    secret.folder_id.map(|id| id.0.to_string()),
                    secret.title,
                    secret.kind.as_str(),
                    secret.favorite,
                    secret.trashed,
                    secret.ciphertext,
                    secret.id.0.to_string(),
                    i64::try_from(expected_revision).context("secret revision is invalid")?,
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not save secret"))?;
        ensure!(
            changed == 1,
            "secret has changed or no longer exists; reload before saving",
        );
        replace_secret_recipients(&transaction, secret.id, &secret.recipients)?;
        set_secret_icon(&transaction, secret.id, secret.icon.as_ref())?;
        if let Some(related) = relationship {
            apply_secret_link_change(&transaction, secret.id, related)?;
        }
        collect_unused_icons(&transaction)?;
        transaction
            .commit()
            .context("could not finish secret update")
    }
}

impl Storage for SqliteStorage {
    fn find_folder(&self, id: FolderId) -> Result<Option<Folder>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let folder = load_folder(&transaction, id)?;
        transaction
            .commit()
            .context("could not finish reading folder metadata")?;
        Ok(folder)
    }

    fn find_secret(&self, id: SecretId) -> Result<Option<Secret>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let secret = load_secret(&transaction, id)?;
        transaction
            .commit()
            .context("could not finish reading secret metadata")?;
        Ok(secret)
    }

    fn list_folders(&self) -> Result<Vec<Folder>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let folders = read_folders(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading folder metadata")?;
        Ok(folders)
    }

    fn list_secrets(&self) -> Result<Vec<Secret>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let secrets = read_secrets(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading secret metadata")?;
        Ok(secrets)
    }

    fn list_secret_metadata(&self) -> Result<Vec<SecretMetadata>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let secrets = read_secret_metadata(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading secret list metadata")?;
        Ok(secrets)
    }

    fn list_secret_links(&self) -> Result<Vec<SecretLink>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let links = read_secret_links(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading linked item metadata")?;
        Ok(links)
    }

    fn insert_folder(&mut self, folder: Folder) -> Result<()> {
        validate_folder(&folder)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin folder update")?;
        validate_parent(&transaction, folder.id, folder.parent_id)?;

        transaction
            .execute(
                "INSERT INTO folders(id, parent_id, name, icon, color, sort_position) \
                 VALUES (?1, ?2, ?3, ?4, ?5, \
                    (SELECT COALESCE(MAX(sort_position), -1) + 1 FROM folders WHERE parent_id IS ?2))",
                params![
                    folder.id.0.to_string(),
                    folder.parent_id.map(|id| id.0.to_string()),
                    folder.name,
                    folder.appearance.icon(),
                    folder.appearance.color(),
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not save folder"))?;
        replace_folder_recipients(&transaction, folder.id, folder.default_recipients.as_ref())?;
        transaction
            .commit()
            .context("could not finish folder update")
    }

    fn insert_secret(&mut self, secret: Secret) -> Result<()> {
        self.insert_secret_with_icon(secret, None)
    }

    fn insert_secret_with_icon(
        &mut self,
        secret: Secret,
        asset: Option<EntryIconAsset>,
    ) -> Result<()> {
        self.insert_secret_record(secret, asset, None)
    }

    fn insert_secret_with_link(
        &mut self,
        secret: Secret,
        asset: Option<EntryIconAsset>,
        related: Option<SecretId>,
    ) -> Result<()> {
        self.insert_secret_record(secret, asset, Some(related))
    }

    fn move_secret(&mut self, id: SecretId, destination: Option<FolderId>) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret update")?;
        if let Some(folder_id) = destination {
            ensure_folder_exists(&transaction, folder_id)?;
        }

        let changed = transaction
            .execute(
                "UPDATE secrets SET folder_id = ?1, revision = revision + 1 \
                 WHERE id = ?2 AND revision < 9223372036854775807",
                params![destination.map(|id| id.0.to_string()), id.0.to_string()],
            )
            .map_err(|_| anyhow::anyhow!("could not move secret"))?;
        ensure!(
            changed == 1,
            "secret does not exist or its revision is exhausted"
        );
        transaction
            .commit()
            .context("could not finish secret update")
    }

    fn replace_encryption(
        &mut self,
        id: SecretId,
        ciphertext: Vec<u8>,
        recipients: RecipientSet,
        expected_revision: u64,
    ) -> Result<()> {
        ensure!(!ciphertext.is_empty(), "encrypted payload cannot be empty");
        ensure!(
            ciphertext.len() <= MAX_CIPHERTEXT_BYTES,
            "encrypted payload exceeds the maximum size"
        );
        ensure!(
            !recipients.as_slice().is_empty(),
            "at least one recipient is required"
        );
        ensure!(
            expected_revision < i64::MAX as u64,
            "secret revision is exhausted",
        );

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret update")?;
        let changed = transaction
            .execute(
                "UPDATE secrets SET ciphertext = ?1, revision = revision + 1 \
                 WHERE id = ?2 AND revision = ?3",
                params![
                    ciphertext,
                    id.0.to_string(),
                    i64::try_from(expected_revision).context("secret revision is invalid")?,
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not update secret encryption"))?;
        ensure!(
            changed == 1,
            "secret has changed or no longer exists; reload before updating encryption",
        );
        replace_secret_recipients(&transaction, id, &recipients)?;
        transaction
            .commit()
            .context("could not finish secret encryption update")
    }

    fn replace_secret(&mut self, secret: Secret, expected_revision: u64) -> Result<()> {
        self.replace_secret_with_icon(secret, expected_revision, None)
    }

    fn replace_secret_with_icon(
        &mut self,
        secret: Secret,
        expected_revision: u64,
        asset: Option<EntryIconAsset>,
    ) -> Result<()> {
        self.replace_secret_record(secret, expected_revision, asset, None)
    }

    fn replace_secret_with_link(
        &mut self,
        secret: Secret,
        expected_revision: u64,
        asset: Option<EntryIconAsset>,
        related: Option<SecretId>,
    ) -> Result<()> {
        self.replace_secret_record(secret, expected_revision, asset, Some(related))
    }

    fn replace_folder(&mut self, folder: Folder) -> Result<()> {
        validate_folder(&folder)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin folder update")?;
        validate_parent(&transaction, folder.id, folder.parent_id)?;

        let changed = transaction
            .execute(
                "UPDATE folders SET parent_id = ?1, name = ?2, icon = ?3, color = ?4 \
                 WHERE id = ?5",
                params![
                    folder.parent_id.map(|id| id.0.to_string()),
                    folder.name,
                    folder.appearance.icon(),
                    folder.appearance.color(),
                    folder.id.0.to_string(),
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not save folder"))?;
        ensure!(changed == 1, "folder does not exist");
        replace_folder_recipients(&transaction, folder.id, folder.default_recipients.as_ref())?;
        transaction
            .commit()
            .context("could not finish folder update")
    }

    fn move_folder(
        &mut self,
        id: FolderId,
        destination: Option<FolderId>,
        before: Option<FolderId>,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin folder move")?;
        let folder = load_folder(&transaction, id)?.context("folder does not exist")?;
        validate_parent(&transaction, id, destination)?;
        ensure!(
            before != Some(id),
            "folder cannot be its own ordering anchor"
        );
        if let Some(anchor) = before {
            let anchor =
                load_folder(&transaction, anchor)?.context("ordering anchor does not exist")?;
            ensure!(
                anchor.parent_id == destination,
                "ordering anchor has a different parent"
            );
        }
        let mut siblings = read_folders(&transaction)?
            .into_iter()
            .filter(|folder| folder.parent_id == destination && folder.id != id)
            .map(|folder| folder.id)
            .collect::<Vec<_>>();
        let position = match before {
            Some(anchor) => siblings
                .iter()
                .position(|candidate| *candidate == anchor)
                .context("ordering anchor does not exist")?,
            None => siblings.len(),
        };
        siblings.insert(position, id);
        transaction.execute(
            "UPDATE folders SET parent_id = ?1 WHERE id = ?2",
            params![
                destination.map(|parent| parent.0.to_string()),
                id.0.to_string()
            ],
        )?;
        persist_folder_order(&transaction, &siblings)?;
        if folder.parent_id != destination {
            let former_siblings = read_folders(&transaction)?
                .into_iter()
                .filter(|candidate| candidate.parent_id == folder.parent_id)
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>();
            persist_folder_order(&transaction, &former_siblings)?;
        }
        transaction.commit().context("could not finish folder move")
    }

    fn remove_secret(&mut self, id: SecretId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret deletion")?;
        if let Some(link) = stored_link_for_secret(&transaction, id)? {
            increment_secret_revision(
                &transaction,
                other_endpoint(link, id),
                "a linked item's revision is exhausted; deletion was cancelled",
            )?;
        }
        let changed = transaction
            .execute("DELETE FROM secrets WHERE id = ?1", [id.0.to_string()])
            .map_err(|_| anyhow::anyhow!("could not delete secret"))?;
        ensure!(changed == 1, "secret does not exist");
        collect_unused_icons(&transaction)?;
        transaction
            .commit()
            .context("could not finish secret deletion")
    }

    fn remove_folder(&mut self, id: FolderId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin folder deletion")?;
        let parent_id: Option<String> = transaction
            .query_row(
                "SELECT parent_id FROM folders WHERE id = ?1",
                [id.0.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| anyhow::anyhow!("could not read folder"))?
            .ok_or_else(|| anyhow::anyhow!("folder does not exist"))?;

        let folders = read_folders(&transaction)?;
        let parent = parent_id.as_deref().map(parse_folder_id).transpose()?;
        let mut remaining_order = Vec::new();
        for sibling in folders.iter().filter(|folder| folder.parent_id == parent) {
            if sibling.id == id {
                remaining_order.extend(
                    folders
                        .iter()
                        .filter(|folder| folder.parent_id == Some(id))
                        .map(|folder| folder.id),
                );
            } else {
                remaining_order.push(sibling.id);
            }
        }

        transaction
            .execute(
                "UPDATE folders SET parent_id = ?1 WHERE parent_id = ?2",
                params![parent_id, id.0.to_string()],
            )
            .map_err(|_| anyhow::anyhow!("could not reparent child folders"))?;
        transaction
            .execute(
                "UPDATE secrets SET folder_id = ?1, revision = revision + 1 \
                 WHERE folder_id = ?2 AND revision < 9223372036854775807",
                params![parent_id, id.0.to_string()],
            )
            .map_err(|_| anyhow::anyhow!("could not reparent secrets"))?;

        let unreparented_secrets: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM secrets WHERE folder_id = ?1)",
                [id.0.to_string()],
                |row| row.get(0),
            )
            .map_err(|_| anyhow::anyhow!("could not verify secret reparenting"))?;
        ensure!(
            !unreparented_secrets,
            "a secret revision is exhausted; folder deletion was cancelled"
        );

        let changed = transaction
            .execute("DELETE FROM folders WHERE id = ?1", [id.0.to_string()])
            .map_err(|_| anyhow::anyhow!("could not delete folder"))?;
        ensure!(changed == 1, "folder does not exist");
        persist_folder_order(&transaction, &remaining_order)?;
        transaction
            .commit()
            .context("could not finish folder deletion")
    }
}

fn persist_folder_order(transaction: &Transaction<'_>, siblings: &[FolderId]) -> Result<()> {
    for (position, id) in siblings.iter().enumerate() {
        transaction.execute(
            "UPDATE folders SET sort_position = ?1 WHERE id = ?2",
            params![i64::try_from(position)?, id.0.to_string()],
        )?;
    }
    Ok(())
}

fn read_folders(connection: &Connection) -> Result<Vec<Folder>> {
    let mut statement = connection
        .prepare(
            "SELECT id, parent_id, name, icon, color, sort_position \
             FROM folders ORDER BY sort_position, name COLLATE NOCASE, id",
        )
        .context("could not read folder metadata")?;
    let rows = statement
        .query_map([], |row| {
            Ok(RawFolder {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                icon: row.get(3)?,
                color: row.get(4)?,
                sort_position: row.get(5)?,
            })
        })
        .context("could not read folder metadata")?;
    let raw_folders = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("could not read folder metadata")?;
    drop(statement);

    let mut folders = Vec::with_capacity(raw_folders.len());
    for raw in raw_folders {
        let id = parse_folder_id(&raw.id)?;
        let parent_id = raw.parent_id.as_deref().map(parse_folder_id).transpose()?;
        let default_recipients = load_folder_recipients(connection, id)?;
        folders.push(Folder {
            id,
            parent_id,
            sort_position: raw.sort_position,
            name: raw.name,
            appearance: parse_folder_appearance(&raw.icon, &raw.color)?,
            default_recipients,
        });
    }

    Ok(folders)
}

fn read_secrets(connection: &Connection) -> Result<Vec<Secret>> {
    let mut statement = connection
        .prepare(
            "SELECT id, folder_id, title, kind, favorite, trashed, revision, \
                    length(ciphertext), ciphertext, icon_theme, icon_asset_id, icon_favicon_host \
             FROM secrets ORDER BY title COLLATE NOCASE, id",
        )
        .context("could not read secret metadata")?;
    let rows = statement
        .query_map([], |row| {
            Ok(RawSecret {
                id: row.get(0)?,
                folder_id: row.get(1)?,
                title: row.get(2)?,
                kind: row.get(3)?,
                favorite: row.get(4)?,
                trashed: row.get(5)?,
                revision: row.get(6)?,
                ciphertext: read_ciphertext(row, 7, 8)?,
                icon_theme: row.get(9)?,
                icon_asset_id: row.get(10)?,
                icon_favicon_host: row.get(11)?,
            })
        })
        .context("could not read secret metadata")?;
    let raw_secrets = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("could not read secret metadata")?;
    drop(statement);

    let mut secrets = Vec::with_capacity(raw_secrets.len());
    for raw in raw_secrets {
        let id = parse_secret_id(&raw.id)?;
        let folder_id = raw.folder_id.as_deref().map(parse_folder_id).transpose()?;
        let recipients = load_secret_recipients(connection, id)?;
        secrets.push(Secret {
            icon: parse_entry_icon(raw.icon_theme, raw.icon_asset_id, raw.icon_favicon_host)?,
            id,
            folder_id,
            title: raw.title,
            kind: parse_kind(&raw.kind)?,
            recipients,
            ciphertext: raw.ciphertext,
            favorite: parse_bool(raw.favorite)?,
            trashed: parse_bool(raw.trashed)?,
            revision: parse_revision(raw.revision)?,
        });
    }

    Ok(secrets)
}

fn read_secret_metadata(connection: &Connection) -> Result<Vec<SecretMetadata>> {
    let mut statement = connection
        .prepare(
            "SELECT id, folder_id, title, kind, favorite, trashed, revision, icon_theme, icon_asset_id, icon_favicon_host \
             FROM secrets ORDER BY title COLLATE NOCASE, id",
        )
        .context("could not read secret list metadata")?;
    let rows = statement
        .query_map([], |row| {
            Ok(RawSecretMetadata {
                id: row.get(0)?,
                folder_id: row.get(1)?,
                title: row.get(2)?,
                kind: row.get(3)?,
                favorite: row.get(4)?,
                trashed: row.get(5)?,
                revision: row.get(6)?,
                icon_theme: row.get(7)?,
                icon_asset_id: row.get(8)?,
                icon_favicon_host: row.get(9)?,
            })
        })
        .context("could not read secret list metadata")?;
    let raw_secrets = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("could not read secret list metadata")?;
    drop(statement);

    let mut secrets = Vec::with_capacity(raw_secrets.len());
    for raw in raw_secrets {
        let id = parse_secret_id(&raw.id)?;
        let folder_id = raw.folder_id.as_deref().map(parse_folder_id).transpose()?;
        let recipients = load_secret_recipients(connection, id)?;
        secrets.push(SecretMetadata {
            icon: parse_entry_icon(raw.icon_theme, raw.icon_asset_id, raw.icon_favicon_host)?,
            id,
            folder_id,
            title: raw.title,
            kind: parse_kind(&raw.kind)?,
            recipients,
            favorite: parse_bool(raw.favorite)?,
            trashed: parse_bool(raw.trashed)?,
            revision: parse_revision(raw.revision)?,
        });
    }

    Ok(secrets)
}

fn read_secret_links(connection: &Connection) -> Result<Vec<SecretLink>> {
    let mut statement = connection
        .prepare(
            "SELECT first_secret_id, second_secret_id FROM secret_links \
             ORDER BY first_secret_id, second_secret_id",
        )
        .context("could not read linked item metadata")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("could not read linked item metadata")?;
    let raw_links = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("could not read linked item metadata")?;
    drop(statement);

    raw_links
        .into_iter()
        .map(|(first, second)| parse_secret_link(&first, &second))
        .collect()
}

fn parse_secret_link(first: &str, second: &str) -> Result<SecretLink> {
    let first = parse_secret_id(first)?;
    let second = parse_secret_id(second)?;
    ensure!(
        first.0 < second.0,
        "stored linked item endpoints are not in canonical order"
    );
    Ok(SecretLink { first, second })
}

fn canonical_secret_link(first: SecretId, second: SecretId) -> SecretLink {
    if first.0 < second.0 {
        SecretLink { first, second }
    } else {
        SecretLink {
            first: second,
            second: first,
        }
    }
}

fn stored_link_for_secret(connection: &Connection, id: SecretId) -> Result<Option<SecretLink>> {
    let raw = connection
        .query_row(
            "SELECT first_secret_id, second_secret_id FROM secret_links \
             WHERE first_secret_id = ?1 OR second_secret_id = ?1",
            [id.0.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .context("could not read linked item metadata")?;
    raw.map(|(first, second)| parse_secret_link(&first, &second))
        .transpose()
}

fn other_endpoint(link: SecretLink, endpoint: SecretId) -> SecretId {
    if link.first == endpoint {
        link.second
    } else {
        link.first
    }
}

fn increment_secret_revision(connection: &Connection, id: SecretId, message: &str) -> Result<()> {
    let changed = connection.execute(
        "UPDATE secrets SET revision = revision + 1 \
         WHERE id = ?1 AND revision < 9223372036854775807",
        [id.0.to_string()],
    )?;
    ensure!(changed == 1, "{message}");
    Ok(())
}

fn apply_secret_link_change(
    transaction: &Transaction<'_>,
    source: SecretId,
    related: Option<SecretId>,
) -> Result<()> {
    let source_trashed = transaction
        .query_row(
            "SELECT trashed FROM secrets WHERE id = ?1",
            [source.0.to_string()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .context("could not inspect linked item")?
        .ok_or(SecretLinkError::InvalidTarget)?;
    let current =
        stored_link_for_secret(transaction, source)?.map(|link| other_endpoint(link, source));

    if current == related {
        return Ok(());
    }

    if let Some(target) = related {
        if parse_bool(source_trashed)? {
            return Err(SecretLinkError::InvalidTarget.into());
        }
        if target == source {
            return Err(SecretLinkError::InvalidTarget.into());
        }
        let target_trashed = transaction
            .query_row(
                "SELECT trashed FROM secrets WHERE id = ?1",
                [target.0.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .context("could not inspect linked item")?
            .ok_or(SecretLinkError::InvalidTarget)?;
        if parse_bool(target_trashed)? {
            return Err(SecretLinkError::InvalidTarget.into());
        }
        if let Some(target_link) = stored_link_for_secret(transaction, target)? {
            if other_endpoint(target_link, target) != source {
                return Err(SecretLinkError::Occupied.into());
            }
        }
    }

    if let Some(previous) = current {
        increment_secret_revision(
            transaction,
            previous,
            "a linked item's revision is exhausted; relationship update was cancelled",
        )?;
        let deleted = transaction.execute(
            "DELETE FROM secret_links WHERE first_secret_id = ?1 OR second_secret_id = ?1",
            [source.0.to_string()],
        )?;
        ensure!(
            deleted == 1,
            "linked item relationship changed unexpectedly"
        );
    }

    if let Some(target) = related {
        increment_secret_revision(
            transaction,
            target,
            "a linked item's revision is exhausted; relationship update was cancelled",
        )?;
        let link = canonical_secret_link(source, target);
        transaction
            .execute(
                "INSERT INTO secret_links(first_secret_id, second_secret_id) VALUES (?1, ?2)",
                params![link.first.0.to_string(), link.second.0.to_string()],
            )
            .context("could not save linked item metadata")?;
    }

    Ok(())
}

fn validate_backup_links(links: &[SecretLink], secrets: &[Secret]) -> Result<()> {
    let secret_ids = secrets
        .iter()
        .map(|secret| secret.id)
        .collect::<HashSet<_>>();
    let mut linked_ids = HashSet::with_capacity(links.len().saturating_mul(2));
    for link in links {
        ensure!(
            link.first != link.second && link.first.0 < link.second.0,
            "backup linked item endpoints are invalid"
        );
        ensure!(
            secret_ids.contains(&link.first) && secret_ids.contains(&link.second),
            "backup linked item target is missing"
        );
        ensure!(
            linked_ids.insert(link.first) && linked_ids.insert(link.second),
            "backup item is linked more than once"
        );
    }
    Ok(())
}

fn insert_imported_secret_links(transaction: &Transaction<'_>, links: &[SecretLink]) -> Result<()> {
    for link in links {
        transaction
            .execute(
                "INSERT INTO secret_links(first_secret_id, second_secret_id) VALUES (?1, ?2)",
                params![link.first.0.to_string(), link.second.0.to_string()],
            )
            .context("could not restore linked item metadata")?;
    }
    Ok(())
}

fn read_ciphertext(
    row: &Row<'_>,
    length_index: usize,
    blob_index: usize,
) -> rusqlite::Result<Vec<u8>> {
    let stored_length: i64 = row.get(length_index)?;
    let stored_length = usize::try_from(stored_length).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(length_index, Type::Integer, Box::new(error))
    })?;
    if stored_length > MAX_CIPHERTEXT_BYTES {
        return Err(column_data_error(
            length_index,
            Type::Integer,
            "encrypted payload exceeds the maximum size",
        ));
    }

    let bytes = match row.get_ref(blob_index)? {
        ValueRef::Blob(bytes) => bytes,
        value => {
            return Err(rusqlite::Error::InvalidColumnType(
                blob_index,
                "ciphertext".to_owned(),
                value.data_type(),
            ));
        }
    };
    if bytes.len() != stored_length || bytes.len() > MAX_CIPHERTEXT_BYTES {
        return Err(column_data_error(
            blob_index,
            Type::Blob,
            "encrypted payload length is invalid",
        ));
    }

    Ok(bytes.to_vec())
}

fn column_data_error(index: usize, column_type: Type, message: &'static str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        column_type,
        Box::new(io::Error::new(io::ErrorKind::InvalidData, message)),
    )
}

fn initialize_schema(connection: &mut Connection) -> Result<()> {
    let version = pragma_i64(connection, "user_version")?;
    if matches!(version, 0..=4) {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not initialize the encrypted store")?;
        let locked_version = pragma_i64(&transaction, "user_version")?;
        match locked_version {
            0 => {
                let existing_application_id = pragma_i64(&transaction, "application_id")?;
                ensure!(
                    existing_application_id == 0 || existing_application_id == APPLICATION_ID,
                    "database file belongs to another application"
                );
                let objects: i64 = transaction
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master \
                         WHERE type IN ('table', 'index', 'view', 'trigger') \
                           AND name NOT LIKE 'sqlite_%'",
                        [],
                        |row| row.get(0),
                    )
                    .context("could not inspect the encrypted store schema")?;
                ensure!(
                    objects == 0,
                    "store schema is unversioned; refusing to modify it",
                );
                create_schema(&transaction)?;
                transaction
                    .pragma_update(None, "application_id", APPLICATION_ID)
                    .context("could not mark the encrypted store")?;
                transaction
                    .pragma_update(None, "user_version", SCHEMA_VERSION)
                    .context("could not version the encrypted store")?;
            }
            1 => {
                let application_id = pragma_i64(&transaction, "application_id")?;
                ensure!(
                    application_id == APPLICATION_ID,
                    "database file is not a KryptoPass store",
                );
                migrate_schema_v1_to_v2(&transaction)?;
                migrate_schema_v2_to_v3(&transaction)?;
                migrate_schema_v3_to_v4(&transaction)?;
                migrate_schema_v4_to_v5(&transaction)?;
            }
            2 => {
                ensure!(
                    pragma_i64(&transaction, "application_id")? == APPLICATION_ID,
                    "database file is not a KryptoPass store"
                );
                migrate_schema_v2_to_v3(&transaction)?;
                migrate_schema_v3_to_v4(&transaction)?;
                migrate_schema_v4_to_v5(&transaction)?;
            }
            3 => {
                ensure!(
                    pragma_i64(&transaction, "application_id")? == APPLICATION_ID,
                    "database file is not a KryptoPass store"
                );
                migrate_schema_v3_to_v4(&transaction)?;
                migrate_schema_v4_to_v5(&transaction)?;
            }
            4 => {
                ensure!(
                    pragma_i64(&transaction, "application_id")? == APPLICATION_ID,
                    "database file is not a KryptoPass store"
                );
                migrate_schema_v4_to_v5(&transaction)?;
            }
            SCHEMA_VERSION => {}
            _ => bail!("store schema version is unsupported"),
        }
        transaction
            .commit()
            .context("could not finish encrypted store initialization")?;
    } else {
        ensure!(
            version == SCHEMA_VERSION,
            "store schema version is unsupported"
        );
    }

    let application_id = pragma_i64(connection, "application_id")?;
    ensure!(
        application_id == APPLICATION_ID,
        "database file is not a KryptoPass store",
    );
    ensure_schema_is_readable(connection)?;
    ensure_secret_links_schema_is_readable(connection)?;
    Ok(())
}

fn migrate_schema_v1_to_v2(transaction: &Transaction<'_>) -> Result<()> {
    transaction
        .execute_batch(
            "ALTER TABLE folders ADD COLUMN icon TEXT NOT NULL DEFAULT 'folder';
             ALTER TABLE folders ADD COLUMN color TEXT NOT NULL DEFAULT 'transparent';",
        )
        .map_err(|_| anyhow::anyhow!("could not migrate folder appearance metadata"))?;
    transaction
        .pragma_update(None, "user_version", 2)
        .context("could not version the encrypted store")
}

fn migrate_schema_v2_to_v3(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch(
        "ALTER TABLE folders ADD COLUMN sort_position INTEGER NOT NULL DEFAULT 0
             CHECK(sort_position >= 0 AND typeof(sort_position) = 'integer');
         WITH ordered AS (
             SELECT id, ROW_NUMBER() OVER (PARTITION BY parent_id ORDER BY name COLLATE NOCASE, id) - 1 AS position
             FROM folders
         )
         UPDATE folders SET sort_position = (SELECT position FROM ordered WHERE ordered.id = folders.id);"
    ).context("could not migrate folder order")?;
    transaction
        .pragma_update(None, "user_version", 3)
        .context("could not version the encrypted store")
}

fn migrate_schema_v3_to_v4(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch(
        "CREATE TABLE entry_icon_assets (id TEXT PRIMARY KEY NOT NULL, png BLOB NOT NULL
             CHECK(typeof(png) = 'blob' AND length(png) BETWEEN 45 AND 524288));
         ALTER TABLE secrets ADD COLUMN icon_theme TEXT;
         ALTER TABLE secrets ADD COLUMN icon_favicon_host TEXT;
         ALTER TABLE secrets ADD COLUMN icon_asset_id TEXT REFERENCES entry_icon_assets(id) ON DELETE RESTRICT;"
    ).context("could not migrate entry icons")?;
    transaction
        .pragma_update(None, "user_version", 4)
        .context("could not version entry icons")
}

fn migrate_schema_v4_to_v5(transaction: &Transaction<'_>) -> Result<()> {
    transaction
        .execute_batch(
            "CREATE TABLE secret_links (
                 first_secret_id TEXT PRIMARY KEY NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
                 second_secret_id TEXT NOT NULL UNIQUE REFERENCES secrets(id) ON DELETE CASCADE,
                 CHECK(first_secret_id < second_secret_id)
             );
             CREATE TRIGGER secret_links_unique_endpoints_insert
             BEFORE INSERT ON secret_links
             WHEN EXISTS (
                 SELECT 1 FROM secret_links
                 WHERE first_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
                    OR second_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
             )
             BEGIN
                 SELECT RAISE(ABORT, 'secret endpoint is already paired');
             END;
             CREATE TRIGGER secret_links_unique_endpoints_update
             BEFORE UPDATE ON secret_links
             WHEN EXISTS (
                 SELECT 1 FROM secret_links
                 WHERE (first_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
                    OR second_secret_id IN (NEW.first_secret_id, NEW.second_secret_id))
                   AND NOT (first_secret_id = OLD.first_secret_id
                        AND second_secret_id = OLD.second_secret_id)
             )
             BEGIN
                 SELECT RAISE(ABORT, 'secret endpoint is already paired');
             END;",
        )
        .context("could not migrate linked item metadata")?;
    transaction
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .context("could not version linked item metadata")
}

fn create_schema(transaction: &Transaction<'_>) -> Result<()> {
    transaction
        .execute_batch(
            "CREATE TABLE entry_icon_assets (id TEXT PRIMARY KEY NOT NULL, png BLOB NOT NULL
                 CHECK(typeof(png) = 'blob' AND length(png) BETWEEN 45 AND 524288));
             CREATE TABLE folders (
                 id TEXT PRIMARY KEY NOT NULL,
                 parent_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
                 name TEXT NOT NULL CHECK(length(trim(name)) > 0),
                 icon TEXT NOT NULL DEFAULT 'folder',
                 color TEXT NOT NULL DEFAULT 'transparent',
                 sort_position INTEGER NOT NULL DEFAULT 0 CHECK(sort_position >= 0 AND typeof(sort_position) = 'integer'),
                 CHECK(parent_id IS NULL OR parent_id <> id)
             );
             CREATE TABLE folder_recipients (
                 folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                 fingerprint TEXT NOT NULL,
                 PRIMARY KEY(folder_id, fingerprint)
             );
             CREATE TABLE secrets (
                 id TEXT PRIMARY KEY NOT NULL,
                 icon_theme TEXT,
                 icon_favicon_host TEXT,
                 icon_asset_id TEXT REFERENCES entry_icon_assets(id) ON DELETE RESTRICT,
                 folder_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
                 title TEXT NOT NULL CHECK(length(trim(title)) > 0),
                 kind TEXT NOT NULL,
                 favorite INTEGER NOT NULL CHECK(favorite IN (0, 1)),
                 trashed INTEGER NOT NULL CHECK(trashed IN (0, 1)),
                 ciphertext BLOB NOT NULL CHECK(length(ciphertext) > 0),
                 revision INTEGER NOT NULL CHECK(revision >= 0 AND typeof(revision) = 'integer')
             );
             CREATE TABLE secret_recipients (
                 secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
                 fingerprint TEXT NOT NULL,
                 PRIMARY KEY(secret_id, fingerprint)
             );
             CREATE TABLE secret_links (
                 first_secret_id TEXT PRIMARY KEY NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
                 second_secret_id TEXT NOT NULL UNIQUE REFERENCES secrets(id) ON DELETE CASCADE,
                 CHECK(first_secret_id < second_secret_id)
             );
             CREATE TRIGGER secret_links_unique_endpoints_insert
             BEFORE INSERT ON secret_links
             WHEN EXISTS (
                 SELECT 1 FROM secret_links
                 WHERE first_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
                    OR second_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
             )
             BEGIN
                 SELECT RAISE(ABORT, 'secret endpoint is already paired');
             END;
             CREATE TRIGGER secret_links_unique_endpoints_update
             BEFORE UPDATE ON secret_links
             WHEN EXISTS (
                 SELECT 1 FROM secret_links
                 WHERE (first_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
                    OR second_secret_id IN (NEW.first_secret_id, NEW.second_secret_id))
                   AND NOT (first_secret_id = OLD.first_secret_id
                        AND second_secret_id = OLD.second_secret_id)
             )
             BEGIN
                 SELECT RAISE(ABORT, 'secret endpoint is already paired');
             END;
             CREATE INDEX folders_by_parent ON folders(parent_id);
             CREATE INDEX secrets_by_folder ON secrets(folder_id);
             CREATE INDEX secrets_by_favorite ON secrets(favorite, trashed);
             CREATE INDEX secret_recipients_by_fingerprint ON secret_recipients(fingerprint);",
        )
        .map_err(|_| anyhow::anyhow!("could not create the encrypted store schema"))
}

fn ensure_schema_is_readable(connection: &Connection) -> Result<()> {
    connection
        .prepare("SELECT id, parent_id, name, icon, color, sort_position FROM folders LIMIT 0")
        .and_then(|_| {
            connection.prepare(
                "SELECT id, folder_id, title, kind, favorite, trashed, ciphertext, revision \
                 FROM secrets LIMIT 0",
            )
        })
        .and_then(|_| {
            connection.prepare("SELECT folder_id, fingerprint FROM folder_recipients LIMIT 0")
        })
        .and_then(|_| {
            connection.prepare("SELECT secret_id, fingerprint FROM secret_recipients LIMIT 0")
        })
        .map_err(|_| anyhow::anyhow!("store schema is corrupt or unsupported"))?;
    Ok(())
}

fn ensure_secret_links_schema_is_readable(connection: &Connection) -> Result<()> {
    connection
        .prepare("SELECT first_secret_id, second_secret_id FROM secret_links LIMIT 0")
        .map_err(|_| anyhow::anyhow!("store schema is corrupt or unsupported"))?;
    Ok(())
}

/// Checks an existing vault using a read-only connection before any writable
/// connection, pragma changes, migration, or permission adjustment occurs.
fn prevalidate_existing_database(path: &Path) -> Result<()> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| anyhow::anyhow!("existing file is not a readable KryptoPass store"))?;
    let application_id = pragma_i64(&connection, "application_id")?;
    let version = pragma_i64(&connection, "user_version")?;
    ensure!(
        application_id == APPLICATION_ID,
        "existing file is not a KryptoPass store"
    );
    ensure!(
        matches!(version, 1 | 2 | 3 | 4 | SCHEMA_VERSION),
        "existing store schema version is unsupported"
    );

    if version == 1 || version == 2 {
        connection
            .prepare("SELECT id, parent_id, name FROM folders LIMIT 0")
            .and_then(|_| {
                connection.prepare(
                    "SELECT id, folder_id, title, kind, favorite, trashed, ciphertext, revision \
                     FROM secrets LIMIT 0",
                )
            })
            .and_then(|_| {
                connection.prepare("SELECT folder_id, fingerprint FROM folder_recipients LIMIT 0")
            })
            .and_then(|_| {
                connection.prepare("SELECT secret_id, fingerprint FROM secret_recipients LIMIT 0")
            })
            .map_err(|_| anyhow::anyhow!("existing store schema is corrupt or unsupported"))?;
    } else {
        ensure_schema_is_readable(&connection)?;
        if version == SCHEMA_VERSION {
            ensure_secret_links_schema_is_readable(&connection)?;
        }
    }
    Ok(())
}

fn pragma_i64(connection: &Connection, name: &str) -> Result<i64> {
    connection
        .pragma_query_value(None, name, |row| row.get(0))
        .map_err(|_| anyhow::anyhow!("could not read encrypted store metadata"))
}

fn load_folder(connection: &Connection, id: FolderId) -> Result<Option<Folder>> {
    let raw = connection
        .query_row(
            "SELECT id, parent_id, name, icon, color, sort_position FROM folders WHERE id = ?1",
            [id.0.to_string()],
            |row| {
                Ok(RawFolder {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    name: row.get(2)?,
                    icon: row.get(3)?,
                    color: row.get(4)?,
                    sort_position: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|_| anyhow::anyhow!("could not read folder"))?;
    raw.map(|raw| {
        let id = parse_folder_id(&raw.id)?;
        let parent_id = raw.parent_id.as_deref().map(parse_folder_id).transpose()?;
        let default_recipients = load_folder_recipients(connection, id)?;
        Ok(Folder {
            id,
            parent_id,
            sort_position: raw.sort_position,
            name: raw.name,
            appearance: parse_folder_appearance(&raw.icon, &raw.color)?,
            default_recipients,
        })
    })
    .transpose()
}

fn load_secret(connection: &Connection, id: SecretId) -> Result<Option<Secret>> {
    let raw = connection
        .query_row(
            "SELECT id, folder_id, title, kind, favorite, trashed, revision, \
                    length(ciphertext), ciphertext, icon_theme, icon_asset_id, icon_favicon_host \
             FROM secrets WHERE id = ?1",
            [id.0.to_string()],
            |row| {
                Ok(RawSecret {
                    id: row.get(0)?,
                    folder_id: row.get(1)?,
                    title: row.get(2)?,
                    kind: row.get(3)?,
                    favorite: row.get(4)?,
                    trashed: row.get(5)?,
                    revision: row.get(6)?,
                    ciphertext: read_ciphertext(row, 7, 8)?,
                    icon_theme: row.get(9)?,
                    icon_asset_id: row.get(10)?,
                    icon_favicon_host: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(|_| anyhow::anyhow!("could not read secret"))?;
    raw.map(|raw| {
        let id = parse_secret_id(&raw.id)?;
        let folder_id = raw.folder_id.as_deref().map(parse_folder_id).transpose()?;
        Ok(Secret {
            icon: parse_entry_icon(raw.icon_theme, raw.icon_asset_id, raw.icon_favicon_host)?,
            id,
            folder_id,
            title: raw.title,
            kind: parse_kind(&raw.kind)?,
            recipients: load_secret_recipients(connection, id)?,
            ciphertext: raw.ciphertext,
            favorite: parse_bool(raw.favorite)?,
            trashed: parse_bool(raw.trashed)?,
            revision: parse_revision(raw.revision)?,
        })
    })
    .transpose()
}

fn load_folder_recipients(connection: &Connection, id: FolderId) -> Result<Option<RecipientSet>> {
    let fingerprints = connection
        .prepare(
            "SELECT fingerprint FROM folder_recipients WHERE folder_id = ?1 ORDER BY fingerprint",
        )
        .and_then(|mut statement| {
            let rows = statement.query_map([id.0.to_string()], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| anyhow::anyhow!("stored folder recipient metadata is invalid"))?;
    if fingerprints.is_empty() {
        return Ok(None);
    }

    parse_recipient_set(fingerprints).map(Some)
}

fn load_secret_recipients(connection: &Connection, id: SecretId) -> Result<RecipientSet> {
    let fingerprints = connection
        .prepare(
            "SELECT fingerprint FROM secret_recipients WHERE secret_id = ?1 ORDER BY fingerprint",
        )
        .and_then(|mut statement| {
            let rows = statement.query_map([id.0.to_string()], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| anyhow::anyhow!("stored secret recipient metadata is invalid"))?;
    ensure!(
        !fingerprints.is_empty(),
        "stored secret recipient metadata is invalid",
    );
    parse_recipient_set(fingerprints)
}

fn parse_recipient_set(values: Vec<String>) -> Result<RecipientSet> {
    let fingerprints = values
        .iter()
        .map(|value| {
            KeyFingerprint::parse(value)
                .map_err(|_| anyhow::anyhow!("stored recipient fingerprint is invalid"))
        })
        .collect::<Result<Vec<_>>>()?;
    RecipientSet::new(fingerprints)
        .map_err(|_| anyhow::anyhow!("stored recipient metadata is invalid"))
}

fn parse_folder_appearance(icon: &str, color: &str) -> Result<FolderAppearance> {
    FolderAppearance::new(icon.to_owned(), color.to_owned())
        .map_err(|_| anyhow::anyhow!("stored folder appearance is invalid"))
}

fn replace_folder_recipients(
    transaction: &Transaction<'_>,
    id: FolderId,
    recipients: Option<&RecipientSet>,
) -> Result<()> {
    transaction
        .execute(
            "DELETE FROM folder_recipients WHERE folder_id = ?1",
            [id.0.to_string()],
        )
        .map_err(|_| anyhow::anyhow!("could not update folder recipients"))?;
    if let Some(recipients) = recipients {
        for fingerprint in recipients.as_slice() {
            transaction
                .execute(
                    "INSERT INTO folder_recipients(folder_id, fingerprint) VALUES (?1, ?2)",
                    params![id.0.to_string(), fingerprint.as_str()],
                )
                .map_err(|_| anyhow::anyhow!("could not update folder recipients"))?;
        }
    }
    Ok(())
}

fn replace_secret_recipients(
    transaction: &Transaction<'_>,
    id: SecretId,
    recipients: &RecipientSet,
) -> Result<()> {
    transaction
        .execute(
            "DELETE FROM secret_recipients WHERE secret_id = ?1",
            [id.0.to_string()],
        )
        .map_err(|_| anyhow::anyhow!("could not update secret recipients"))?;
    for fingerprint in recipients.as_slice() {
        transaction
            .execute(
                "INSERT INTO secret_recipients(secret_id, fingerprint) VALUES (?1, ?2)",
                params![id.0.to_string(), fingerprint.as_str()],
            )
            .map_err(|_| anyhow::anyhow!("could not update secret recipients"))?;
    }
    Ok(())
}

fn ensure_folder_exists(connection: &Connection, id: FolderId) -> Result<()> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM folders WHERE id = ?1)",
            [id.0.to_string()],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("could not read folder"))?;
    ensure!(exists, "destination folder does not exist");
    Ok(())
}

fn validate_parent(
    connection: &Connection,
    folder_id: FolderId,
    parent_id: Option<FolderId>,
) -> Result<()> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    ensure!(parent_id != folder_id, "a folder cannot be its own parent");
    ensure_folder_exists(connection, parent_id)?;

    let parent_is_descendant: bool = connection
        .query_row(
            "WITH RECURSIVE ancestors(id) AS (
                 SELECT ?1
                 UNION
                 SELECT folders.parent_id
                 FROM folders JOIN ancestors ON folders.id = ancestors.id
                 WHERE folders.parent_id IS NOT NULL
             )
             SELECT EXISTS(SELECT 1 FROM ancestors WHERE id = ?2)",
            params![parent_id.0.to_string(), folder_id.0.to_string()],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("could not validate folder hierarchy"))?;
    ensure!(!parent_is_descendant, "folder move would create a cycle");
    Ok(())
}

fn validate_folder(folder: &Folder) -> Result<()> {
    ensure!(folder.sort_position >= 0, "folder order is invalid");
    ensure!(
        !folder.name.trim().is_empty(),
        "folder name cannot be empty"
    );
    Ok(())
}

fn validate_secret(secret: &Secret) -> Result<()> {
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

fn validate_backup_read_budget(connection: &Connection) -> Result<()> {
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

fn validate_backup_snapshot(folders: &[Folder], secrets: &[Secret]) -> Result<()> {
    validate_encrypted_batch(folders, secrets, false)
}

fn validate_encrypted_batch(
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

fn parse_kind(value: &str) -> Result<SecretKind> {
    SecretKind::parse(value).map_err(|error| match error {
        DomainError::UnsupportedSecretKind => {
            anyhow::anyhow!("stored secret kind is unsupported")
        }
        _ => anyhow::anyhow!("stored secret kind is invalid"),
    })
}

fn parse_bool(value: i64) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => bail!("stored boolean metadata is invalid"),
    }
}

fn parse_revision(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| anyhow::anyhow!("stored secret revision is invalid"))
}

fn parse_folder_id(value: &str) -> Result<FolderId> {
    parse_uuid(value).map(FolderId)
}

fn parse_secret_id(value: &str) -> Result<SecretId> {
    parse_uuid(value).map(SecretId)
}

fn parse_uuid(value: &str) -> Result<Uuid> {
    let id = Uuid::parse_str(value).map_err(|_| anyhow::anyhow!("stored identifier is invalid"))?;
    ensure!(
        id.to_string() == value,
        "stored identifier is not canonical"
    );
    Ok(id)
}

fn ensure_database_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let existed = match fs::symlink_metadata(parent) {
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(_) => bail!("could not inspect the store directory"),
    };
    if !existed {
        fs::create_dir_all(parent).context("could not create the store directory")?;
    }

    let directory = open_directory(parent)?;
    let metadata = directory
        .metadata()
        .context("could not inspect the store directory")?;
    ensure_owned_by_process(&metadata)?;
    if existed {
        ensure_private_directory(&metadata)?;
    } else {
        set_private_directory_permissions(&directory, &metadata)?;
    }
    Ok(())
}

fn ensure_existing_database_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let directory =
        open_directory(parent).context("existing store parent must be a real private directory")?;
    let metadata = directory
        .metadata()
        .context("could not inspect the existing store parent")?;
    ensure_owned_by_process(&metadata)?;
    ensure_private_directory(&metadata)
}

fn ensure_database_file_is_safe(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                !metadata.file_type().is_symlink() && metadata.is_file(),
                "store path must be a regular file"
            );
            let file = open_database_file(path)
                .map_err(|_| anyhow::anyhow!("could not inspect the store file"))?;
            let opened_metadata = file
                .metadata()
                .context("could not inspect the store file")?;
            validate_database_file(&opened_metadata)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => bail!("could not inspect the store file"),
    }
}

fn set_private_database_permissions(path: &Path) -> Result<()> {
    let file = open_database_file(path).context("could not inspect the store file")?;
    let metadata = file
        .metadata()
        .context("could not inspect the store file")?;
    validate_database_file(&metadata)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o7777 != 0o600 {
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .context("could not secure the store file")?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn ensure_private_directory(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    ensure!(
        metadata.permissions().mode() & 0o7777 == 0o700,
        "store directory must have private 0700 permissions"
    );
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_directory(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(directory: &File, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    ensure_owned_by_process(metadata)?;
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        directory
            .set_permissions(fs::Permissions::from_mode(0o700))
            .context("could not secure the store directory")?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_directory: &File, _metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

fn open_directory(path: &Path) -> Result<File> {
    #[cfg(unix)]
    let directory = {
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .context("could not open store directory without following links")?
    };

    #[cfg(not(unix))]
    let directory = OpenOptions::new()
        .read(true)
        .open(path)
        .context("could not open store directory")?;

    ensure!(
        directory
            .metadata()
            .context("could not inspect store directory")?
            .is_dir(),
        "store directory must be a real directory"
    );
    Ok(directory)
}

fn open_database_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)
    }

    #[cfg(not(unix))]
    {
        OpenOptions::new().read(true).open(path)
    }
}

fn validate_database_file(metadata: &fs::Metadata) -> Result<()> {
    ensure!(metadata.is_file(), "store path must be a regular file");
    ensure_single_link(metadata)?;
    ensure_owned_by_process(metadata)?;
    Ok(())
}

#[cfg(unix)]
fn ensure_owned_by_process(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let effective_uid = unsafe { libc::geteuid() };
    ensure!(
        metadata.uid() == effective_uid,
        "store path is not owned by the current user"
    );
    Ok(())
}

#[cfg(not(unix))]
fn ensure_owned_by_process(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn ensure_single_link(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    ensure!(metadata.nlink() == 1, "store file must not have hard links");
    Ok(())
}

#[cfg(not(unix))]
fn ensure_single_link(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[derive(Debug)]
struct RawFolder {
    id: String,
    parent_id: Option<String>,
    name: String,
    icon: String,
    color: String,
    sort_position: i64,
}

#[derive(Debug)]
struct RawSecret {
    icon_theme: Option<String>,
    icon_asset_id: Option<String>,
    icon_favicon_host: Option<String>,
    id: String,
    folder_id: Option<String>,
    title: String,
    kind: String,
    favorite: i64,
    trashed: i64,
    ciphertext: Vec<u8>,
    revision: i64,
}

#[derive(Debug)]
struct RawSecretMetadata {
    icon_theme: Option<String>,
    icon_asset_id: Option<String>,
    icon_favicon_host: Option<String>,
    id: String,
    folder_id: Option<String>,
    title: String,
    kind: String,
    favorite: i64,
    trashed: i64,
    revision: i64,
}

#[cfg(test)]
#[path = "folder_move_tests.rs"]
mod folder_move_tests;

#[cfg(test)]
#[path = "entry_links_tests.rs"]
mod entry_links_tests;

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
    fn append_import(&mut self, folders: &[Folder], secrets: &[Secret]) -> Result<()> {
        validate_encrypted_batch(folders, secrets, true)?;
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
        transaction
            .commit()
            .context("could not commit additive import")
    }
}

fn parse_entry_icon(
    theme: Option<String>,
    asset: Option<String>,
    host: Option<String>,
) -> Result<Option<EntryIcon>> {
    let icon = match (theme, asset, host) {
        (None, None, None) => None,
        (None, None, Some(host)) => Some(EntryIcon::Favicon { host }),
        (Some(name), None, None) => Some(EntryIcon::Theme { name }),
        (None, Some(id), None) => Some(EntryIcon::Image {
            asset_id: Uuid::parse_str(&id)?,
        }),
        _ => bail!("entry contains conflicting icon sources"),
    };
    if let Some(icon) = &icon {
        icon.validate()?;
    }
    Ok(icon)
}

fn insert_icon_asset(connection: &Connection, asset: &EntryIconAsset) -> Result<()> {
    asset.validate()?;
    connection
        .execute(
            "INSERT INTO entry_icon_assets(id, png) VALUES (?1, ?2)",
            params![asset.id.to_string(), &asset.png],
        )
        .context("could not save icon resource")?;
    Ok(())
}

fn set_secret_icon(connection: &Connection, id: SecretId, icon: Option<&EntryIcon>) -> Result<()> {
    let (theme, asset, host) = match icon {
        Some(EntryIcon::Favicon { host }) => (None, None, Some(host.clone())),
        Some(EntryIcon::Theme { name }) => (Some(name.clone()), None, None),
        Some(EntryIcon::Image { asset_id }) => (None, Some(asset_id.to_string()), None),
        None => (None, None, None),
    };
    connection
        .execute(
            "UPDATE secrets SET icon_theme = ?1, icon_asset_id = ?2, icon_favicon_host = ?3 WHERE id = ?4",
            params![theme, asset, host, id.0.to_string()],
        )
        .context("could not save entry icon")?;
    Ok(())
}

fn collect_unused_icons(connection: &Connection) -> Result<()> {
    connection
        .execute(
            "DELETE FROM entry_icon_assets WHERE id NOT IN
        (SELECT icon_asset_id FROM secrets WHERE icon_asset_id IS NOT NULL)",
            [],
        )
        .context("could not collect unused icons")?;
    Ok(())
}

fn read_icon_assets(connection: &Connection) -> Result<Vec<EntryIconAsset>> {
    let total: i64 = connection.query_row(
        "SELECT COALESCE(SUM(length(png)), 0) FROM entry_icon_assets",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        total >= 0 && total as u64 <= MAX_BACKUP_PLAINTEXT_BYTES as u64 / 2,
        "icon resources exceed backup budget"
    );
    let mut statement =
        connection.prepare("SELECT id, length(png), png FROM entry_icon_assets ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        let size: i64 = row.get(1)?;
        if !(45..=MAX_ENTRY_ICON_BYTES as i64).contains(&size) {
            return Err(column_data_error(
                1,
                Type::Integer,
                "invalid icon resource size",
            ));
        }
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(2)?))
    })?;
    rows.map(|row| {
        let (id, png) = row?;
        let asset = EntryIconAsset {
            id: Uuid::parse_str(&id)?,
            png,
        };
        asset.validate()?;
        Ok(asset)
    })
    .collect()
}
