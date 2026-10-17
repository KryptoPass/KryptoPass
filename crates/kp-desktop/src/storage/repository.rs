//! Transactional implementation of the core storage port and secret writes.

use super::SqliteStorage;
use super::folders::{ensure_folder_exists, persist_folder_order, validate_parent};
use super::icons::{collect_unused_icons, insert_icon_asset, set_secret_icon};
use super::links::{
    apply_secret_link_change, increment_secret_revision, other_endpoint, read_secret_links,
    stored_link_for_secret,
};
use super::recipients::{replace_folder_recipients, replace_secret_recipients};
use super::records::{
    load_folder, load_secret, parse_folder_id, parse_secret_id, read_folders, read_secret_metadata,
    read_secrets,
};
use super::validation::{validate_folder, validate_secret};
use crate::infrastructure::MAX_CIPHERTEXT_BYTES;
use anyhow::{Context, Result, ensure};
use kryptopass_core::{
    EntryIcon, EntryIconAsset, Folder, FolderId, RecipientSet, Secret, SecretId, SecretLink,
    SecretMetadata, Storage,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use std::collections::HashSet;

impl SqliteStorage {
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

    fn remove_folder_tree(&mut self, id: FolderId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin recursive folder deletion")?;
        let folder = load_folder(&transaction, id)?.context("folder does not exist")?;
        let folder_ids = {
            let mut statement = transaction.prepare(
                "WITH RECURSIVE subtree(id, depth) AS (
                     SELECT id, 0 FROM folders WHERE id = ?1
                     UNION ALL
                     SELECT folders.id, subtree.depth + 1 FROM folders
                     JOIN subtree ON folders.parent_id = subtree.id
                 ) SELECT id FROM subtree ORDER BY depth DESC, id",
            )?;
            statement
                .query_map([id.0.to_string()], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let secret_ids = {
            let mut statement = transaction.prepare(
                "WITH RECURSIVE subtree(id) AS (
                     SELECT id FROM folders WHERE id = ?1
                     UNION ALL
                     SELECT folders.id FROM folders
                     JOIN subtree ON folders.parent_id = subtree.id
                 ) SELECT secrets.id FROM secrets
                 JOIN subtree ON secrets.folder_id = subtree.id",
            )?;
            let ids = statement
                .query_map([id.0.to_string()], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids.into_iter()
                .map(|raw| parse_secret_id(&raw))
                .collect::<Result<HashSet<_>>>()?
        };

        // Only surviving endpoints need a revision: internal pairs disappear
        // together, so exhausted revisions on deleted entries cannot block this.
        for link in read_secret_links(&transaction)? {
            let first_removed = secret_ids.contains(&link.first);
            let second_removed = secret_ids.contains(&link.second);
            if first_removed != second_removed {
                increment_secret_revision(
                    &transaction,
                    if first_removed {
                        link.second
                    } else {
                        link.first
                    },
                    "a linked item's revision is exhausted; folder deletion was cancelled",
                )?;
            }
        }

        // Foreign keys restrict parent deletion until its children are gone.
        for folder_id in folder_ids {
            transaction.execute("DELETE FROM secrets WHERE folder_id = ?1", [&folder_id])?;
            transaction.execute("DELETE FROM folders WHERE id = ?1", [&folder_id])?;
        }
        let siblings = read_folders(&transaction)?
            .into_iter()
            .filter(|candidate| candidate.parent_id == folder.parent_id)
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();
        persist_folder_order(&transaction, &siblings)?;
        collect_unused_icons(&transaction)?;
        transaction
            .commit()
            .context("could not finish recursive folder deletion")
    }
}
