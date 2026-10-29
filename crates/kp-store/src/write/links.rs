//! Pairs of linked items. A link is not versioned: it relates two items, not
//! two states, so it lives beside their histories.

use kryptopass_core::vault::StoreError;
use kryptopass_core::{SecretId, SecretLink};
use rusqlite::{Connection, OptionalExtension, params};

use super::bump_revision;
use crate::convert::{SqlResult, StoreResult, flag, text};
use crate::read;

/// Pairs `source` with `target`, or unpairs it with `None`. Both endpoints
/// of every link that changes get a new revision.
pub(super) fn set(
    connection: &Connection,
    source: SecretId,
    target: Option<SecretId>,
) -> StoreResult<()> {
    let current = read::link_of(connection, source)?;
    if current == target {
        return Ok(());
    }
    if let Some(target) = target {
        let valid = target != source
            && trashed(connection, source)? == Some(false)
            && trashed(connection, target)? == Some(false);
        if !valid {
            return Err(StoreError::Rejected("link target is not valid"));
        }
        if read::link_of(connection, target)?.is_some_and(|companion| companion != source) {
            return Err(StoreError::Rejected("link target is already paired"));
        }
    }
    if let Some(previous) = current {
        bump_revision(connection, previous)?;
        connection
            .execute(
                "DELETE FROM item_links WHERE first_item_id = ?1 OR second_item_id = ?1",
                [text(source.0)],
            )
            .sql()?;
    }
    if let Some(target) = target {
        bump_revision(connection, target)?;
        insert(connection, source, target)?;
    }
    Ok(())
}

/// Stores a link with its endpoints in canonical order.
pub(super) fn insert(connection: &Connection, a: SecretId, b: SecretId) -> StoreResult<()> {
    let link =
        SecretLink::new(a, b).ok_or(StoreError::Rejected("an item cannot link to itself"))?;
    connection
        .execute(
            "INSERT INTO item_links(first_item_id, second_item_id) VALUES (?1, ?2)",
            params![text(link.first.0), text(link.second.0)],
        )
        .sql()?;
    Ok(())
}

/// Whether the item's head is in the trash; `None` when the item is missing.
fn trashed(connection: &Connection, id: SecretId) -> StoreResult<Option<bool>> {
    connection
        .query_row(
            "SELECT v.trashed FROM items i JOIN item_versions v ON v.id = i.head \
             WHERE i.id = ?1",
            [text(id.0)],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .sql()?
        .map(flag)
        .transpose()
}
