//! Folder hierarchy validation and sibling order persistence.

use anyhow::{Result, ensure};
use kryptopass_core::FolderId;
use rusqlite::{Connection, Transaction, params};

pub(super) fn persist_folder_order(
    transaction: &Transaction<'_>,
    siblings: &[FolderId],
) -> Result<()> {
    for (position, id) in siblings.iter().enumerate() {
        transaction.execute(
            "UPDATE folders SET sort_position = ?1 WHERE id = ?2",
            params![i64::try_from(position)?, id.0.to_string()],
        )?;
    }
    Ok(())
}

pub(super) fn ensure_folder_exists(connection: &Connection, id: FolderId) -> Result<()> {
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

pub(super) fn validate_parent(
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
