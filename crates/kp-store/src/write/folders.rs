//! Folder writes: signed folder versions, the folders shown, sibling order
//! and imports. The order among siblings is this device's own; it does not
//! travel.

use std::collections::{HashMap, HashSet};

use kryptopass_core::vault::{
    AppendItem, FolderContents, Hlc, IndexedFolder, SealedFolder, StoreError, VersionId,
};
use kryptopass_core::{Folder, FolderAppearance, FolderId, RecipientSet, SecretId, SecretLink};
use kryptopass_identity::DeviceIdentity;
use kryptopass_sync::{Clock, Fingerprint, RecordKind, Version, VersionDraft};
use rusqlite::{Connection, OptionalExtension, params};

use super::{collect_garbage, items, links, require_folder};
use crate::convert::{
    SqlResult, StoreResult, fingerprint_bytes, folder_id, id_bytes, item_id, text,
};
use crate::read;
use kryptopass_core::util::{Ancestry, ancestry, is_clean_line, unix_millis};

/// Sanity bound on a stored name; the vault applies the real, smaller limit.
const MAX_NAME_BYTES: usize = 1024;

pub(super) fn add(
    connection: &Connection,
    identity: &DeviceIdentity,
    folder: &Folder,
    sealed: SealedFolder,
) -> StoreResult<()> {
    check_name(&folder.name)?;
    if let Some(parent) = folder.parent_id {
        require_folder(connection, parent)?;
    }
    require_new(connection, folder.id)?;
    let head = write_version(connection, identity, folder, &[], sealed)?;
    let parent = folder.parent_id.map(|id| text(id.0));
    connection
        .execute(
            "INSERT INTO folders(id, head, parent_id, name, icon, color, sort_position) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, \
                 (SELECT COALESCE(MAX(sort_position), -1) + 1 FROM folders \
                  WHERE parent_id IS ?3))",
            params![
                text(folder.id.0),
                text(head.0),
                parent,
                folder.name,
                folder.appearance.icon(),
                folder.appearance.color(),
            ],
        )
        .sql()?;
    set_recipients(connection, folder.id, folder.default_recipients.as_ref())?;
    attach_items(connection, folder.id)
}

/// Writes a new version of a folder that joins every current one, so
/// concurrent edits from other devices end with this one.
pub(super) fn update(
    connection: &Connection,
    identity: &DeviceIdentity,
    folder: &Folder,
    sealed: SealedFolder,
) -> StoreResult<()> {
    check_name(&folder.name)?;
    require_folder(connection, folder.id)?;
    let heads = items::record_heads(connection, &text(folder.id.0))?;
    let head = write_version(connection, identity, folder, &heads, sealed)?;
    connection
        .execute(
            "UPDATE folders SET head = ?1, name = ?2, icon = ?3, color = ?4 WHERE id = ?5",
            params![
                text(head.0),
                folder.name,
                folder.appearance.icon(),
                folder.appearance.color(),
                text(folder.id.0),
            ],
        )
        .sql()?;
    set_recipients(connection, folder.id, folder.default_recipients.as_ref())
}

pub(super) fn move_to(
    connection: &Connection,
    identity: &DeviceIdentity,
    id: FolderId,
    destination: Option<FolderId>,
    before: Option<FolderId>,
    sealed: Option<SealedFolder>,
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
    match sealed {
        Some(sealed) => {
            let mut moved = all
                .iter()
                .find(|folder| folder.id == id)
                .cloned()
                .ok_or(StoreError::FolderNotFound)?;
            moved.parent_id = destination;
            let heads = items::record_heads(connection, &text(id.0))?;
            let head = write_version(connection, identity, &moved, &heads, sealed)?;
            connection
                .execute(
                    "UPDATE folders SET head = ?1, parent_id = ?2 WHERE id = ?3",
                    params![
                        text(head.0),
                        destination.map(|parent| text(parent.0)),
                        text(id.0)
                    ],
                )
                .sql()?;
        }
        None if former != destination => {
            return Err(StoreError::Rejected("a new parent needs a new version"));
        }
        None => {}
    }
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
    identity: &DeviceIdentity,
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
            // The vault moved items and subfolders out with new versions.
            if !items_in(connection, &HashSet::from([id]))?.is_empty()
                || all.iter().any(|folder| folder.parent_id == Some(id))
            {
                return Err(StoreError::Rejected("the folder is not empty"));
            }
            delete_with_mark(connection, identity, id)?;
            let left: Vec<FolderId> = all
                .iter()
                .filter(|folder| folder.parent_id == parent && folder.id != id)
                .map(|folder| folder.id)
                .collect();
            persist_order(connection, &left)?;
        }
        FolderContents::Delete => {
            let subtree = subtree(&all, id);
            let removed: HashSet<SecretId> =
                items_in(connection, &subtree.iter().copied().collect())?
                    .into_iter()
                    .collect();
            for item in &removed {
                items::delete_with_mark(connection, identity, *item)?;
            }
            // Deepest first: a folder cannot go while it has children.
            for folder in subtree.iter().rev() {
                delete_with_mark(connection, identity, *folder)?;
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
    identity: &DeviceIdentity,
    folders: Vec<(Folder, SealedFolder)>,
    items: Vec<AppendItem>,
    pairs: &[SecretLink],
) -> StoreResult<()> {
    for (folder, sealed) in folders {
        add(connection, identity, &folder, sealed)?;
    }
    for item in items {
        items::create_imported(connection, identity, item)?;
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

/// Indexes folder versions the vault opened, then shows each folder as its
/// newest readable version and places folders and items under parents that
/// are now known.
pub(super) fn index(connection: &Connection, versions: Vec<IndexedFolder>) -> StoreResult<()> {
    let mut touched = Vec::new();
    for IndexedFolder { id, folder, state } in versions {
        let key = text(id.0);
        let owner: Option<String> = connection
            .query_row(
                "SELECT record_id FROM versions WHERE id = ?1 AND record_kind = 2",
                [&key],
                |row| row.get(0),
            )
            .optional()
            .sql()?;
        let indexed: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM folder_index WHERE version_id = ?1)",
                [&key],
                |row| row.get(0),
            )
            .sql()?;
        if owner.as_deref() != Some(text(folder.0).as_str())
            || indexed
            || !is_clean_line(&state.name, MAX_NAME_BYTES)
        {
            continue;
        }
        insert_index(
            connection,
            id,
            &state.name,
            state.parent,
            &state.appearance,
            state.default_recipients.as_ref(),
        )?;
        if !touched.contains(&folder) {
            touched.push(folder);
        }
    }
    for folder in &touched {
        refresh(connection, *folder)?;
    }
    if !touched.is_empty() {
        place_folders(connection)?;
    }
    Ok(())
}

/// Shows a folder as its newest readable version. With none it leaves the
/// tree, and its items show at the root.
fn refresh(connection: &Connection, folder: FolderId) -> StoreResult<()> {
    let key = text(folder.0);
    let mut readable = Vec::new();
    for head in items::record_heads(connection, &key)? {
        let indexed: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM folder_index WHERE version_id = ?1)",
                [text(head.0)],
                |row| row.get(0),
            )
            .sql()?;
        if indexed {
            readable.push(head);
        }
    }
    let clocks = items::clocks_of(connection, &readable)?;
    let Some(head) = readable
        .into_iter()
        .zip(clocks)
        .max_by_key(|(version, clock)| (*clock, *version))
        .map(|(version, _)| version)
    else {
        return purge(connection, &key);
    };
    let (name, icon, color): (String, String, String) = connection
        .query_row(
            "SELECT name, icon, color FROM folder_index WHERE version_id = ?1",
            [text(head.0)],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .sql()?;
    let shown: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM folders WHERE id = ?1)",
            [&key],
            |row| row.get(0),
        )
        .sql()?;
    if shown {
        connection
            .execute(
                "UPDATE folders SET head = ?1, name = ?2, icon = ?3, color = ?4 WHERE id = ?5",
                params![text(head.0), name, icon, color, key],
            )
            .sql()?;
    } else {
        // It starts at the root; `place_folders` gives it its parent.
        connection
            .execute(
                "INSERT INTO folders(id, head, parent_id, name, icon, color, sort_position) \
                 VALUES (?1, ?2, NULL, ?3, ?4, ?5, \
                     (SELECT COALESCE(MAX(sort_position), -1) + 1 FROM folders \
                      WHERE parent_id IS NULL))",
                params![key, text(head.0), name, icon, color],
            )
            .sql()?;
    }
    connection
        .execute("DELETE FROM folder_recipients WHERE folder_id = ?1", [&key])
        .sql()?;
    connection
        .execute(
            "INSERT INTO folder_recipients(folder_id, fingerprint) \
             SELECT ?1, fingerprint FROM folder_index_recipients WHERE version_id = ?2",
            params![key, text(head.0)],
        )
        .sql()?;
    attach_items(connection, folder)
}

/// Gives each folder the parent its version names when that parent is known
/// here and would not close a cycle; otherwise it stays at the root. Two
/// devices can each move one folder into the other.
fn place_folders(connection: &Connection) -> StoreResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT f.id, f.parent_id, x.parent FROM folders f \
             JOIN folder_index x ON x.version_id = f.head ORDER BY f.id",
        )
        .sql()?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    let mut parents: HashMap<FolderId, Option<FolderId>> = HashMap::new();
    let mut wanted = Vec::new();
    for (id, current, chosen) in rows {
        let id = folder_id(&id)?;
        parents.insert(id, current.as_deref().map(folder_id).transpose()?);
        wanted.push((id, chosen.as_deref().map(folder_id).transpose()?));
    }
    for (id, chosen) in wanted {
        let target = chosen.filter(|parent| {
            parents.contains_key(parent)
                && *parent != id
                && ancestry(&parents, *parent, id) == Ancestry::Outside
        });
        if parents.get(&id) == Some(&target) {
            continue;
        }
        connection
            .execute(
                "UPDATE folders SET parent_id = ?1, sort_position = \
                     (SELECT COALESCE(MAX(sort_position), -1) + 1 FROM folders \
                      WHERE parent_id IS ?1) WHERE id = ?2",
                params![target.map(|parent| text(parent.0)), text(id.0)],
            )
            .sql()?;
        parents.insert(id, target);
    }
    Ok(())
}

/// Items whose state names `folder` go into it now that it is known.
fn attach_items(connection: &Connection, folder: FolderId) -> StoreResult<()> {
    connection
        .execute(
            "UPDATE item_index SET folder_id = ?1 WHERE wanted_folder = ?1 AND folder_id IS NULL",
            [text(folder.0)],
        )
        .sql()?;
    Ok(())
}

/// Signs a deletion mark for a folder and deletes everything else of it.
fn delete_with_mark(
    connection: &Connection,
    identity: &DeviceIdentity,
    id: FolderId,
) -> StoreResult<()> {
    let deletion = items::sign_deletion(connection, identity, RecordKind::Folder, id.0)?;
    purge(connection, &text(id.0))?;
    items::insert_deletion(connection, &deletion)
}

/// Deletes a folder's versions and its row. Its subfolders move to its
/// parent and its items to the root, so a deletion received from another
/// device never leaves anything pointing at it.
pub(crate) fn purge(connection: &Connection, record: &str) -> StoreResult<()> {
    let parent: Option<Option<String>> = connection
        .query_row(
            "SELECT parent_id FROM folders WHERE id = ?1",
            [record],
            |row| row.get(0),
        )
        .optional()
        .sql()?;
    if let Some(parent) = parent {
        connection
            .execute(
                "UPDATE folders SET parent_id = ?1 WHERE parent_id = ?2",
                params![parent, record],
            )
            .sql()?;
        connection
            .execute("DELETE FROM folders WHERE id = ?1", [record])
            .sql()?;
    }
    connection
        .execute("DELETE FROM versions WHERE record_id = ?1", [record])
        .sql()?;
    Ok(())
}

/// Signs and stores a version of `folder` as this device writes it, and
/// indexes it.
fn write_version(
    connection: &Connection,
    identity: &DeviceIdentity,
    folder: &Folder,
    parents: &[VersionId],
    sealed: SealedFolder,
) -> StoreResult<VersionId> {
    let version = VersionId::new();
    let clock = Hlc::next(unix_millis(), items::clocks_of(connection, parents)?);
    let recipients = sealed
        .recipients
        .as_slice()
        .iter()
        .map(|key| {
            Fingerprint::new(fingerprint_bytes(key.as_str())?)
                .map_err(|_| StoreError::Rejected("invalid recipient fingerprint"))
        })
        .collect::<StoreResult<Vec<_>>>()?;
    let signed = Version::sign(
        VersionDraft {
            record_kind: RecordKind::Folder,
            record_id: id_bytes(folder.id.0),
            version_id: id_bytes(version.0),
            parents: parents.iter().map(|parent| id_bytes(parent.0)).collect(),
            clock: Clock {
                millis: clock.millis,
                counter: clock.counter,
            },
            recipients,
            state: sealed.state,
            content_hash: None,
            attachments: Vec::new(),
        },
        identity,
    )
    .map_err(|_| StoreError::Rejected("version could not be signed"))?;
    items::insert_signed(connection, &signed, [])?;
    insert_index(
        connection,
        version,
        &folder.name,
        folder.parent_id,
        &folder.appearance,
        folder.default_recipients.as_ref(),
    )?;
    Ok(version)
}

fn insert_index(
    connection: &Connection,
    version: VersionId,
    name: &str,
    parent: Option<FolderId>,
    appearance: &FolderAppearance,
    recipients: Option<&RecipientSet>,
) -> StoreResult<()> {
    connection
        .execute(
            "INSERT INTO folder_index(version_id, name, parent, icon, color) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                text(version.0),
                name,
                parent.map(|parent| text(parent.0)),
                appearance.icon(),
                appearance.color(),
            ],
        )
        .sql()?;
    for fingerprint in recipients.map_or(&[][..], RecipientSet::as_slice) {
        connection
            .execute(
                "INSERT INTO folder_index_recipients(version_id, fingerprint) VALUES (?1, ?2)",
                params![text(version.0), fingerprint.as_str()],
            )
            .sql()?;
    }
    Ok(())
}

/// A new folder's id must be unused, and never one deleted for good.
fn require_new(connection: &Connection, id: FolderId) -> StoreResult<()> {
    let used: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM versions WHERE record_id = ?1) \
                 OR EXISTS(SELECT 1 FROM deletions WHERE record_id = ?1)",
            [text(id.0)],
            |row| row.get(0),
        )
        .sql()?;
    if used {
        Err(StoreError::Rejected("folder already exists"))
    } else {
        Ok(())
    }
}

/// Items whose head is in one of `folders`.
fn items_in(connection: &Connection, folders: &HashSet<FolderId>) -> StoreResult<Vec<SecretId>> {
    let mut statement = connection
        .prepare(
            "SELECT i.id, x.folder_id FROM items i JOIN item_index x ON x.version_id = i.head \
             WHERE x.folder_id IS NOT NULL",
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
