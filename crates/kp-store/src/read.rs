//! Queries behind the read half of the port.
//!
//! Lists load recipients and parents with one query per table and join them
//! in memory, instead of one query per row.

use std::collections::HashMap;

use kryptopass_core::vault::{
    ItemMeta, ItemRecord, ItemState, ItemVersion, VersionId, VersionInfo,
};
use kryptopass_core::{
    EntryIconAsset, Folder, FolderAppearance, FolderId, MAX_ENTRY_ICON_BYTES, SecretId, SecretLink,
};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::convert::{
    self, SqlResult, StoreResult, corrupt, flag, folder_id, item_id, revision, text, version_id,
};
use kryptopass_core::util::MAX_CIPHERTEXT_BYTES;

pub(crate) fn folders(connection: &Connection) -> StoreResult<Vec<Folder>> {
    let mut recipients = fingerprints_by(
        connection,
        "SELECT folder_id, fingerprint FROM folder_recipients ORDER BY folder_id, fingerprint",
        [],
    )?;
    let mut statement = connection
        .prepare(
            "SELECT id, parent_id, name, icon, color, sort_position FROM folders \
             ORDER BY sort_position, name COLLATE NOCASE, id",
        )
        .sql()?;
    let rows = statement
        .query_map([], raw_folder)
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    rows.into_iter()
        .map(|raw| {
            let fingerprints = recipients.remove(&raw.0).unwrap_or_default();
            build_folder(raw, fingerprints)
        })
        .collect()
}

pub(crate) fn folder(connection: &Connection, id: FolderId) -> StoreResult<Option<Folder>> {
    let key = text(id.0);
    let Some(raw) = connection
        .query_row(
            "SELECT id, parent_id, name, icon, color, sort_position FROM folders WHERE id = ?1",
            [&key],
            raw_folder,
        )
        .optional()
        .sql()?
    else {
        return Ok(None);
    };
    let mut recipients = fingerprints_by(
        connection,
        "SELECT folder_id, fingerprint FROM folder_recipients WHERE folder_id = ?1 \
         ORDER BY fingerprint",
        [&key],
    )?;
    let fingerprints = recipients.remove(&key).unwrap_or_default();
    build_folder(raw, fingerprints).map(Some)
}

type RawFolder = (String, Option<String>, String, String, String, i64);

fn raw_folder(row: &Row<'_>) -> rusqlite::Result<RawFolder> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn build_folder(raw: RawFolder, fingerprints: Vec<String>) -> StoreResult<Folder> {
    let (id, parent, name, icon, color, sort_position) = raw;
    Ok(Folder {
        id: folder_id(&id)?,
        parent_id: parent.as_deref().map(folder_id).transpose()?,
        sort_position,
        name,
        appearance: FolderAppearance::new(icon, color).map_err(|_| corrupt("folder appearance"))?,
        default_recipients: if fingerprints.is_empty() {
            None
        } else {
            Some(convert::recipients(fingerprints)?)
        },
    })
}

/// Head state columns, shared by the item queries. `v` is the head version.
const HEAD_COLUMNS: &str = "i.id, i.head, i.revision, \
     (SELECT COUNT(*) FROM item_heads h WHERE h.item_id = i.id), \
     v.folder_id, v.title, v.template, v.favorite, v.trashed, \
     v.icon_theme, v.icon_asset_id, v.icon_favicon_host";

pub(crate) fn items(connection: &Connection) -> StoreResult<Vec<ItemMeta>> {
    let mut recipients = fingerprints_by(
        connection,
        "SELECT r.version_id, r.fingerprint FROM item_version_recipients r \
         JOIN items i ON i.head = r.version_id ORDER BY r.version_id, r.fingerprint",
        [],
    )?;
    let sql = format!(
        "SELECT {HEAD_COLUMNS} FROM items i JOIN item_versions v ON v.id = i.head \
         ORDER BY v.title COLLATE NOCASE, i.id"
    );
    let mut statement = connection.prepare(&sql).sql()?;
    let rows = statement
        .query_map([], raw_meta)
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    rows.into_iter()
        .map(|raw| {
            let fingerprints = recipients.remove(&raw.head).unwrap_or_default();
            meta(raw, fingerprints)
        })
        .collect()
}

pub(crate) fn item_meta(connection: &Connection, id: SecretId) -> StoreResult<Option<ItemMeta>> {
    let sql = format!(
        "SELECT {HEAD_COLUMNS} FROM items i JOIN item_versions v ON v.id = i.head \
         WHERE i.id = ?1"
    );
    let Some(raw) = connection
        .query_row(&sql, [text(id.0)], raw_meta)
        .optional()
        .sql()?
    else {
        return Ok(None);
    };
    let fingerprints = version_fingerprints(connection, &raw.head)?;
    meta(raw, fingerprints).map(Some)
}

pub(crate) fn item(connection: &Connection, id: SecretId) -> StoreResult<Option<ItemRecord>> {
    let Some(meta) = item_meta(connection, id)? else {
        return Ok(None);
    };
    let ciphertext = payload(connection, meta.head)?;
    Ok(Some(ItemRecord { meta, ciphertext }))
}

struct RawMeta {
    id: String,
    head: String,
    revision: i64,
    heads: i64,
    state: RawState,
}

struct RawState {
    folder: Option<String>,
    title: String,
    template: String,
    favorite: i64,
    trashed: i64,
    icon: convert::IconColumns,
}

fn raw_meta(row: &Row<'_>) -> rusqlite::Result<RawMeta> {
    Ok(RawMeta {
        id: row.get(0)?,
        head: row.get(1)?,
        revision: row.get(2)?,
        heads: row.get(3)?,
        state: raw_state(row, 4)?,
    })
}

/// Seven state columns starting at `at`: folder, title, template, favorite,
/// trashed and the three icon columns.
fn raw_state(row: &Row<'_>, at: usize) -> rusqlite::Result<RawState> {
    Ok(RawState {
        folder: row.get(at)?,
        title: row.get(at + 1)?,
        template: row.get(at + 2)?,
        favorite: row.get(at + 3)?,
        trashed: row.get(at + 4)?,
        icon: (row.get(at + 5)?, row.get(at + 6)?, row.get(at + 7)?),
    })
}

fn meta(raw: RawMeta, fingerprints: Vec<String>) -> StoreResult<ItemMeta> {
    if raw.heads < 1 {
        return Err(corrupt("item heads"));
    }
    Ok(ItemMeta {
        id: item_id(&raw.id)?,
        head: version_id(&raw.head)?,
        revision: revision(raw.revision)?,
        conflicted: raw.heads > 1,
        state: state(raw.state, fingerprints)?,
    })
}

fn state(raw: RawState, fingerprints: Vec<String>) -> StoreResult<ItemState> {
    Ok(ItemState {
        folder: raw.folder.as_deref().map(folder_id).transpose()?,
        title: raw.title,
        template: convert::template(&raw.template)?,
        recipients: convert::recipients(fingerprints)?,
        icon: convert::icon(raw.icon)?,
        favorite: flag(raw.favorite)?,
        trashed: flag(raw.trashed)?,
    })
}

/// The ciphertext of a version, bounded before it is copied out of SQLite.
pub(crate) fn payload(connection: &Connection, version: VersionId) -> StoreResult<Vec<u8>> {
    connection
        .query_row(
            "SELECT p.ciphertext FROM item_versions v \
             JOIN item_payloads p ON p.hash = v.payload WHERE v.id = ?1",
            [text(version.0)],
            |row| match row.get_ref(0)? {
                ValueRef::Blob(bytes)
                    if !bytes.is_empty() && bytes.len() <= MAX_CIPHERTEXT_BYTES =>
                {
                    Ok(Some(bytes.to_vec()))
                }
                _ => Ok(None),
            },
        )
        .optional()
        .sql()?
        .flatten()
        .ok_or_else(|| corrupt("payload"))
}

pub(crate) fn links(connection: &Connection) -> StoreResult<Vec<SecretLink>> {
    let mut statement = connection
        .prepare("SELECT first_item_id, second_item_id FROM item_links ORDER BY first_item_id")
        .sql()?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    rows.into_iter()
        .map(|(first, second)| {
            let (first, second) = (item_id(&first)?, item_id(&second)?);
            match SecretLink::new(first, second) {
                Some(link) if link.first == first => Ok(link),
                _ => Err(corrupt("link")),
            }
        })
        .collect()
}

pub(crate) fn link_of(connection: &Connection, id: SecretId) -> StoreResult<Option<SecretId>> {
    let key = text(id.0);
    connection
        .query_row(
            "SELECT CASE WHEN first_item_id = ?1 THEN second_item_id ELSE first_item_id END \
             FROM item_links WHERE first_item_id = ?1 OR second_item_id = ?1",
            [&key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .sql()?
        .map(|other| item_id(&other))
        .transpose()
}

/// A stored icon image, bounded before it is copied and validated after.
pub(crate) fn icon_asset(connection: &Connection, id: Uuid) -> StoreResult<Option<EntryIconAsset>> {
    let png = connection
        .query_row(
            "SELECT png FROM entry_icon_assets WHERE id = ?1",
            [text(id)],
            |row| match row.get_ref(0)? {
                ValueRef::Blob(bytes) if bytes.len() <= MAX_ENTRY_ICON_BYTES => {
                    Ok(Some(bytes.to_vec()))
                }
                _ => Ok(None),
            },
        )
        .optional()
        .sql()?;
    match png {
        None => Ok(None),
        Some(None) => Err(corrupt("icon image")),
        Some(Some(png)) => {
            let asset = EntryIconAsset { id, png };
            asset.validate().map_err(|_| corrupt("icon image"))?;
            Ok(Some(asset))
        }
    }
}

/// Which versions of an item to load.
pub(crate) enum Versions {
    All,
    Heads,
    One(VersionId),
}

/// Versions of an item, newest first by local storage order.
pub(crate) fn versions(
    connection: &Connection,
    id: SecretId,
    which: Versions,
) -> StoreResult<Vec<VersionInfo>> {
    let item = text(id.0);
    let (filter, extra) = match which {
        Versions::All => ("", None),
        Versions::Heads => (
            "AND v.id IN (SELECT version_id FROM item_heads WHERE item_id = ?1)",
            None,
        ),
        Versions::One(version) => ("AND v.id = ?2", Some(text(version.0))),
    };
    let sql = format!(
        "SELECT v.id, v.author, v.created_at, v.folder_id, v.title, v.template, v.favorite, \
                v.trashed, v.icon_theme, v.icon_asset_id, v.icon_favicon_host \
         FROM item_versions v WHERE v.item_id = ?1 {filter} ORDER BY v.rowid DESC"
    );
    let mut statement = connection.prepare(&sql).sql()?;
    let map = |row: &Row<'_>| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i64>(2)?,
            raw_state(row, 3)?,
        ))
    };
    let rows = match &extra {
        Some(version) => statement.query_map(params![item, version], map),
        None => statement.query_map(params![item], map),
    }
    .sql()?
    .collect::<rusqlite::Result<Vec<_>>>()
    .sql()?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let mut recipients = fingerprints_by(
        connection,
        "SELECT r.version_id, r.fingerprint FROM item_version_recipients r \
         JOIN item_versions v ON v.id = r.version_id WHERE v.item_id = ?1 \
         ORDER BY r.version_id, r.fingerprint",
        [&item],
    )?;
    let mut parents = parents_of(connection, id)?;
    rows.into_iter()
        .map(|(version, author, created_at, raw)| {
            let fingerprints = recipients.remove(&version).unwrap_or_default();
            let id_value = version_id(&version)?;
            Ok(VersionInfo {
                id: id_value,
                item: id,
                parents: parents.remove(&id_value).unwrap_or_default(),
                state: state(raw, fingerprints)?,
                author: convert::author(author)?,
                created_at: convert::timestamp(created_at)?,
            })
        })
        .collect()
}

pub(crate) fn version(
    connection: &Connection,
    id: SecretId,
    version: VersionId,
) -> StoreResult<Option<ItemVersion>> {
    let Some(info) = versions(connection, id, Versions::One(version))?.pop() else {
        return Ok(None);
    };
    let ciphertext = payload(connection, version)?;
    Ok(Some(ItemVersion { info, ciphertext }))
}

/// Parent edges of every version of an item, parents sorted.
pub(crate) fn parents_of(
    connection: &Connection,
    id: SecretId,
) -> StoreResult<HashMap<VersionId, Vec<VersionId>>> {
    let mut statement = connection
        .prepare(
            "SELECT p.version_id, p.parent_id FROM item_version_parents p \
             JOIN item_versions v ON v.id = p.version_id WHERE v.item_id = ?1",
        )
        .sql()?;
    let rows = statement
        .query_map([text(id.0)], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    let mut edges: HashMap<VersionId, Vec<VersionId>> = HashMap::new();
    for (child, parent) in rows {
        edges
            .entry(version_id(&child)?)
            .or_default()
            .push(version_id(&parent)?);
    }
    edges.values_mut().for_each(|list| list.sort_unstable());
    Ok(edges)
}

fn version_fingerprints(connection: &Connection, version: &str) -> StoreResult<Vec<String>> {
    Ok(fingerprints_by(
        connection,
        "SELECT version_id, fingerprint FROM item_version_recipients WHERE version_id = ?1 \
         ORDER BY fingerprint",
        [version],
    )?
    .remove(version)
    .unwrap_or_default())
}

/// Rows of `(owner, fingerprint)` grouped by owner.
fn fingerprints_by<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> StoreResult<HashMap<String, Vec<String>>> {
    let mut statement = connection.prepare(sql).sql()?;
    let rows = statement
        .query_map(params, |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .sql()?;
    let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        let (owner, fingerprint) = row.sql()?;
        grouped.entry(owner).or_default().push(fingerprint);
    }
    Ok(grouped)
}
