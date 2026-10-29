//! Folder writes, sibling order and imports.

use std::collections::{HashMap, HashSet};

use kryptopass_core::vault::{FolderContents, ItemRecord, StoreError};
use kryptopass_core::{Folder, FolderId, RecipientSet, SecretId, SecretLink};
use rusqlite::{Connection, params};

use super::{bump_revision, collect_garbage, items, links, require_folder};
use crate::convert::{SqlResult, StoreResult, folder_id, item_id, text};
use crate::read;
use kryptopass_core::util::{Ancestry, ancestry, is_clean_line, unix_now};

/// Sanity bound on a stored name; the vault applies the real, smaller limit.
const MAX_NAME_BYTES: usize = 1024;

pub(super) fn add(connection: &Connection, folder: &Folder) -> StoreResult<()> {
    check_name(&folder.name)?;
    if let Some(parent) = folder.parent_id {
        require_folder(connection, parent)?;
    }
    let parent = folder.parent_id.map(|id| text(id.0));
    connection
        .execute(
            "INSERT INTO folders(id, parent_id, name, icon, color, sort_position) \
             VALUES (?1, ?2, ?3, ?4, ?5, \
                 (SELECT COALESCE(MAX(sort_position), -1) + 1 FROM folders \
                  WHERE parent_id IS ?2))",
            params![
                text(folder.id.0),
                parent,
                folder.name,
                folder.appearance.icon(),
                folder.appearance.color(),
            ],
        )
        .sql()?;
    set_recipients(connection, folder.id, folder.default_recipients.as_ref())
}

pub(super) fn update(connection: &Connection, folder: &Folder) -> StoreResult<()> {
    check_name(&folder.name)?;
    let changed = connection
        .execute(
            "UPDATE folders SET name = ?1, icon = ?2, color = ?3 WHERE id = ?4",
            params![
                folder.name,
                folder.appearance.icon(),
                folder.appearance.color(),
                text(folder.id.0),
            ],
        )
        .sql()?;
    if changed != 1 {
        return Err(StoreError::FolderNotFound);
    }
    set_recipients(connection, folder.id, folder.default_recipients.as_ref())
}

pub(super) fn move_to(
    connection: &Connection,
    id: FolderId,
    destination: Option<FolderId>,
    before: Option<FolderId>,
) -> StoreResult<()> {
    let all = read::folders(connection)?;
    let parents: HashMap<FolderId, Option<FolderId>> = all
        .iter()
        .map(|folder| (folder.id, folder.parent_id))
        .collect();
    let former = *parents.get(&id).ok_or(StoreError::FolderNotFound)?;
    if let Some(destination) = destination {
        if !parents.contains_key(&destination) {
            return Err(StoreError::FolderNotFound);
        }
        match ancestry(&parents, destination, id) {
            Ancestry::Outside => {}
            Ancestry::Within => {
                return Err(StoreError::Rejected("folder move would create a cycle"));
            }
            Ancestry::Broken => return Err(StoreError::Rejected("stored folder tree is broken")),
        }
    }
    if let Some(anchor) = before
        && (anchor == id || parents.get(&anchor) != Some(&destination))
    {
        return Err(StoreError::Rejected("ordering anchor is not a sibling"));
    }

    // `all` is in display order, so siblings keep their relative order.
    let mut siblings: Vec<FolderId> = all
        .iter()
        .filter(|folder| folder.parent_id == destination && folder.id != id)
        .map(|folder| folder.id)
        .collect();
    let position = before
        .and_then(|anchor| siblings.iter().position(|sibling| *sibling == anchor))
        .unwrap_or(siblings.len());
    siblings.insert(position, id);
    connection
        .execute(
            "UPDATE folders SET parent_id = ?1 WHERE id = ?2",
            params![destination.map(|parent| text(parent.0)), text(id.0)],
        )
        .sql()?;
    persist_order(connection, &siblings)?;
    if former != destination {
        let left: Vec<FolderId> = all
            .iter()
            .filter(|folder| folder.parent_id == former && folder.id != id)
            .map(|folder| folder.id)
            .collect();
        persist_order(connection, &left)?;
    }
    Ok(())
}

pub(super) fn remove(
    connection: &Connection,
    id: FolderId,
    contents: FolderContents,
) -> StoreResult<()> {
    let all = read::folders(connection)?;
    let folder = all
        .iter()
        .find(|folder| folder.id == id)
        .ok_or(StoreError::FolderNotFound)?;
    let parent = folder.parent_id;
    match contents {
        FolderContents::Reparent => {
            for item in items_in(connection, &HashSet::from([id]))? {
                items::move_head(connection, item, parent)?;
            }
            // Subfolders take the removed folder's place among its siblings.
            let mut order = Vec::new();
            for sibling in all.iter().filter(|folder| folder.parent_id == parent) {
                if sibling.id == id {
                    order.extend(
                        all.iter()
                            .filter(|folder| folder.parent_id == Some(id))
                            .map(|folder| folder.id),
                    );
                } else {
                    order.push(sibling.id);
                }
            }
            connection
                .execute(
                    "UPDATE folders SET parent_id = ?1 WHERE parent_id = ?2",
                    params![parent.map(|parent| text(parent.0)), text(id.0)],
                )
                .sql()?;
            connection
                .execute("DELETE FROM folders WHERE id = ?1", [text(id.0)])
                .sql()?;
            persist_order(connection, &order)?;
        }
        FolderContents::Delete => {
            let subtree = subtree(&all, id);
            let removed: HashSet<SecretId> =
                items_in(connection, &subtree.iter().copied().collect())?
                    .into_iter()
                    .collect();
            for item in &removed {
                // A partner inside the subtree goes too; only survivors are bumped.
                if let Some(other) = read::link_of(connection, *item)?
                    && !removed.contains(&other)
                {
                    bump_revision(connection, other)?;
                }
                connection
                    .execute("DELETE FROM items WHERE id = ?1", [text(item.0)])
                    .sql()?;
            }
            // Deepest first: a folder cannot go while it has children.
            for folder in subtree.iter().rev() {
                connection
                    .execute("DELETE FROM folders WHERE id = ?1", [text(folder.0)])
                    .sql()?;
            }
            let left: Vec<FolderId> = all
                .iter()
                .filter(|folder| folder.parent_id == parent && folder.id != id)
                .map(|folder| folder.id)
                .collect();
            persist_order(connection, &left)?;
            collect_garbage(connection)?;
        }
    }
    Ok(())
}

/// Adds folders, items and links that must all be new. Folders come parents
/// first; each item becomes its record's first version.
pub(super) fn append(
    connection: &Connection,
    folders: &[Folder],
    records: Vec<ItemRecord>,
    pairs: &[SecretLink],
) -> StoreResult<()> {
    for folder in folders {
        add(connection, folder)?;
    }
    let now = unix_now();
    for record in &records {
        items::create_from_record(connection, record, now)?;
    }
    for link in pairs {
        if link.first.0 >= link.second.0 {
            return Err(StoreError::Rejected(
                "link endpoints are not in canonical order",
            ));
        }
        links::insert(connection, link.first, link.second)?;
    }
    Ok(())
}

/// Items whose head is in one of `folders`.
fn items_in(connection: &Connection, folders: &HashSet<FolderId>) -> StoreResult<Vec<SecretId>> {
    let mut statement = connection
        .prepare(
            "SELECT i.id, v.folder_id FROM items i JOIN item_versions v ON v.id = i.head \
             WHERE v.folder_id IS NOT NULL",
        )
        .sql()?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    let mut found = Vec::new();
    for (item, folder) in rows {
        if folders.contains(&folder_id(&folder)?) {
            found.push(item_id(&item)?);
        }
    }
    Ok(found)
}

/// `root` and every folder below it, parents before children.
fn subtree(all: &[Folder], root: FolderId) -> Vec<FolderId> {
    let mut order = vec![root];
    let mut index = 0;
    while let Some(&current) = order.get(index) {
        order.extend(
            all.iter()
                .filter(|folder| folder.parent_id == Some(current))
                .map(|folder| folder.id),
        );
        index += 1;
    }
    order
}

fn persist_order(connection: &Connection, siblings: &[FolderId]) -> StoreResult<()> {
    for (position, id) in siblings.iter().enumerate() {
        connection
            .execute(
                "UPDATE folders SET sort_position = ?1 WHERE id = ?2",
                params![
                    i64::try_from(position)
                        .map_err(|_| StoreError::Rejected("too many folders"))?,
                    text(id.0)
                ],
            )
            .sql()?;
    }
    Ok(())
}

fn set_recipients(
    connection: &Connection,
    id: FolderId,
    recipients: Option<&RecipientSet>,
) -> StoreResult<()> {
    connection
        .execute(
            "DELETE FROM folder_recipients WHERE folder_id = ?1",
            [text(id.0)],
        )
        .sql()?;
    for fingerprint in recipients.map_or(&[][..], RecipientSet::as_slice) {
        connection
            .execute(
                "INSERT INTO folder_recipients(folder_id, fingerprint) VALUES (?1, ?2)",
                params![text(id.0), fingerprint.as_str()],
            )
            .sql()?;
    }
    Ok(())
}

fn check_name(name: &str) -> StoreResult<()> {
    if !is_clean_line(name, MAX_NAME_BYTES) {
        return Err(StoreError::Rejected("invalid folder name"));
    }
    Ok(())
}
