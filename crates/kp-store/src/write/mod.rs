//! The write half of the port: one transaction per `Change`.
//!
//! Every precondition the vault already checked is checked again here, inside
//! an `IMMEDIATE` transaction, because another writer may have changed the
//! store in between. Any failure rolls the whole change back.

mod folders;
mod items;
mod links;
mod settings;

pub(crate) use folders::purge as purge_folder;
pub(crate) use items::{belongs, insert_deletion, insert_signed, purge_record, refresh_item};

use kryptopass_core::vault::{Change, StoreError};
use kryptopass_core::{EntryIcon, EntryIconAsset, FolderId, SecretId};
use kryptopass_identity::DeviceIdentity;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::convert::{SqlResult, StoreResult, text};
use kryptopass_core::util::MAX_CIPHERTEXT_BYTES;

pub(crate) fn commit(
    connection: &mut Connection,
    identity: &DeviceIdentity,
    change: Change,
) -> StoreResult<()> {
    let kind = change_kind(&change);
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .sql()?;
    match change {
        Change::PutItem(put) => items::put(&transaction, identity, *put)?,
        Change::RemoveItem { id, base } => items::remove(&transaction, identity, id, base)?,
        Change::AddFolder { folder, sealed } => {
            folders::add(&transaction, identity, &folder, sealed)?;
        }
        Change::UpdateFolder { folder, sealed } => {
            folders::update(&transaction, identity, &folder, sealed)?;
        }
        Change::MoveFolder {
            id,
            destination,
            before,
            sealed,
        } => folders::move_to(&transaction, identity, id, destination, before, sealed)?,
        Change::RemoveFolder { id, contents } => {
            folders::remove(&transaction, identity, id, contents)?;
        }
        Change::Append {
            folders,
            items,
            links,
        } => folders::append(&transaction, identity, folders, items, &links)?,
        Change::Index(versions) => items::index(&transaction, versions)?,
        Change::IndexFolders(versions) => folders::index(&transaction, versions)?,
        Change::SetDefaultRecipients(recipients) => {
            settings::set_default_recipients(&transaction, recipients.as_ref())?;
        }
        Change::SetKeySettings(key) => settings::set_key_settings(&transaction, &key)?,
    }
    transaction.commit().sql()?;
    tracing::debug!(change = kind, "store change committed");
    Ok(())
}

fn change_kind(change: &Change) -> &'static str {
    match change {
        Change::PutItem(_) => "put_item",
        Change::RemoveItem { .. } => "remove_item",
        Change::AddFolder { .. } => "add_folder",
        Change::UpdateFolder { .. } => "update_folder",
        Change::MoveFolder { .. } => "move_folder",
        Change::RemoveFolder { .. } => "remove_folder",
        Change::Append { .. } => "append",
        Change::Index(_) => "index",
        Change::IndexFolders(_) => "index_folders",
        Change::SetDefaultRecipients(_) => "set_default_recipients",
        Change::SetKeySettings(_) => "set_key_settings",
    }
}

pub(super) fn require_folder(connection: &Connection, id: FolderId) -> StoreResult<()> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM folders WHERE id = ?1)",
            [text(id.0)],
            |row| row.get(0),
        )
        .sql()?;
    if exists {
        Ok(())
    } else {
        Err(StoreError::FolderNotFound)
    }
}

/// Bumps the revision of an item that changed indirectly (its link, its
/// heads), so an editor that opened it sees it as stale.
pub(super) fn bump_revision(connection: &Connection, id: SecretId) -> StoreResult<()> {
    let changed = connection
        .execute(
            "UPDATE items SET revision = revision + 1 \
             WHERE id = ?1 AND revision < 9223372036854775807",
            [text(id.0)],
        )
        .sql()?;
    match changed {
        1 => Ok(()),
        _ => Err(StoreError::Rejected("item revision exhausted")),
    }
}

pub(super) fn check_ciphertext(ciphertext: &[u8]) -> StoreResult<()> {
    if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
        return Err(StoreError::Rejected("ciphertext size out of bounds"));
    }
    Ok(())
}

/// Stores the image of a newly chosen picture icon; it must be the icon given.
pub(super) fn store_icon_asset(
    connection: &Connection,
    icon: Option<&EntryIcon>,
    asset: Option<&EntryIconAsset>,
) -> StoreResult<()> {
    let Some(asset) = asset else {
        return Ok(());
    };
    if icon != Some(&EntryIcon::Image { asset_id: asset.id }) || asset.validate().is_err() {
        return Err(StoreError::Rejected("icon image does not match the icon"));
    }
    let exists = connection
        .query_row(
            "SELECT 1 FROM entry_icon_assets WHERE id = ?1",
            [text(asset.id)],
            |_| Ok(()),
        )
        .optional()
        .sql()?
        .is_some();
    if !exists {
        connection
            .execute(
                "INSERT INTO entry_icon_assets(id, png) VALUES (?1, ?2)",
                params![text(asset.id), asset.png],
            )
            .sql()?;
    }
    Ok(())
}

/// Removes blobs and icon images no version refers to any more. Only
/// deletions orphan them, so only they call this.
pub(crate) fn collect_garbage(connection: &Connection) -> StoreResult<()> {
    connection
        .execute_batch(
            "DELETE FROM blobs WHERE hash NOT IN
                 (SELECT content_hash FROM versions WHERE content_hash IS NOT NULL)
                 AND hash NOT IN (SELECT hash FROM version_attachments);
             DELETE FROM entry_icon_assets WHERE id NOT IN
                 (SELECT icon_asset_id FROM item_index WHERE icon_asset_id IS NOT NULL);",
        )
        .sql()
}
