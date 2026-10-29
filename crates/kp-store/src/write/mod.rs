//! The write half of the port: one transaction per `Change`.
//!
//! Every precondition the vault already checked is checked again here, inside
//! an `IMMEDIATE` transaction, because another writer may have changed the
//! store in between. Any failure rolls the whole change back.

mod folders;
mod items;
mod links;

pub(crate) use items::belongs;

use kryptopass_core::vault::{Change, StoreError};
use kryptopass_core::{EntryIcon, EntryIconAsset, FolderId, SecretId};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::convert::{SqlResult, StoreResult, text};
use kryptopass_core::util::MAX_CIPHERTEXT_BYTES;

pub(crate) fn commit(connection: &mut Connection, change: Change) -> StoreResult<()> {
    let kind = change_kind(&change);
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .sql()?;
    match change {
        Change::PutItem(put) => items::put(&transaction, *put)?,
        Change::AddVersion {
            version,
            icon_asset,
        } => items::add_version(&transaction, *version, icon_asset)?,
        Change::RemoveItem { id, base } => items::remove(&transaction, id, base)?,
        Change::AddFolder(folder) => folders::add(&transaction, &folder)?,
        Change::UpdateFolder(folder) => folders::update(&transaction, &folder)?,
        Change::MoveFolder {
            id,
            destination,
            before,
        } => folders::move_to(&transaction, id, destination, before)?,
        Change::RemoveFolder { id, contents } => folders::remove(&transaction, id, contents)?,
        Change::Append {
            folders,
            items,
            links,
        } => folders::append(&transaction, &folders, items, &links)?,
    }
    transaction.commit().sql()?;
    tracing::debug!(change = kind, "store change committed");
    Ok(())
}

fn change_kind(change: &Change) -> &'static str {
    match change {
        Change::PutItem(_) => "put_item",
        Change::AddVersion { .. } => "add_version",
        Change::RemoveItem { .. } => "remove_item",
        Change::AddFolder(_) => "add_folder",
        Change::UpdateFolder(_) => "update_folder",
        Change::MoveFolder { .. } => "move_folder",
        Change::RemoveFolder { .. } => "remove_folder",
        Change::Append { .. } => "append",
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

/// Removes payloads and icon images no version refers to any more. Only
/// deletions orphan them, so only they call this.
pub(super) fn collect_garbage(connection: &Connection) -> StoreResult<()> {
    connection
        .execute_batch(
            "DELETE FROM item_payloads WHERE hash NOT IN (SELECT payload FROM item_versions);
             DELETE FROM entry_icon_assets WHERE id NOT IN
                 (SELECT icon_asset_id FROM item_versions WHERE icon_asset_id IS NOT NULL);",
        )
        .sql()
}
