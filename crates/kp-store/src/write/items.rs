//! Item writes: local versions, versions from elsewhere, moves and deletion.

use kryptopass_core::vault::{
    Author, Base, Hlc, ItemRecord, ItemState, ItemVersion, LinkChange, Parents, PutItem,
    StoreError, VersionId, VersionInfo,
};
use kryptopass_core::{EntryIconAsset, FolderId, SecretId};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use super::{
    bump_revision, check_ciphertext, collect_garbage, links, require_folder, store_icon_asset,
};
use crate::convert::{
    SqlResult, StoreResult, author_param, clock, clock_params, icon_columns, revision,
    revision_param, text, version_id,
};
use crate::{graph, read};
use kryptopass_core::util::{is_clean_line, unix_millis};

/// Sanity bound on a stored title; the vault applies the real, smaller limit.
const MAX_TITLE_BYTES: usize = 1024;
/// A merge joins at most this many versions, as a Sync revision does.
const MAX_PARENTS: usize = 16;

pub(super) fn put(connection: &Connection, put: PutItem) -> StoreResult<()> {
    let PutItem {
        record: ItemRecord { meta, ciphertext },
        base,
        parents,
        now_millis,
        icon_asset,
        link,
    } = put;
    check_ciphertext(&ciphertext)?;
    check_state(connection, &meta.state)?;
    store_icon_asset(connection, meta.state.icon.as_ref(), icon_asset.as_ref())?;
    match base {
        Base::New => {
            if meta.revision != 0 {
                return Err(StoreError::Rejected("a new item starts at revision 0"));
            }
            if current(connection, meta.id)?.is_some() {
                return Err(StoreError::Rejected("item already exists"));
            }
            create(
                connection,
                meta.id,
                meta.head,
                &meta.state,
                &Author::Local,
                Hlc::next(now_millis, []),
                &ciphertext,
            )?;
        }
        Base::Revision(base) => {
            let (stored, head) = current(connection, meta.id)?.ok_or(StoreError::ItemNotFound)?;
            if stored != base {
                return Err(StoreError::Stale);
            }
            if base.checked_add(1) != Some(meta.revision) {
                return Err(StoreError::Rejected(
                    "the new revision must follow the base",
                ));
            }
            let parents = match parents {
                Parents::Head => vec![head],
                Parents::Merge(list) => merge_parents(connection, meta.id, head, list)?,
            };
            let clock = Hlc::next(now_millis, clocks_of(connection, &parents)?);
            insert_version(
                connection,
                meta.id,
                meta.head,
                &parents,
                &meta.state,
                &Author::Local,
                clock,
                &ciphertext,
            )?;
            advance_heads(connection, meta.id, meta.head, &parents)?;
            connection
                .execute(
                    "UPDATE items SET head = ?1, revision = ?2 WHERE id = ?3",
                    params![
                        text(meta.head.0),
                        revision_param(meta.revision)?,
                        text(meta.id.0)
                    ],
                )
                .sql()?;
        }
    }
    if let LinkChange::Set(target) = link {
        links::set(connection, meta.id, target)?;
    }
    Ok(())
}

/// Parents of a local merge: versions of this item, the head among them.
fn merge_parents(
    connection: &Connection,
    item: SecretId,
    head: VersionId,
    mut list: Vec<VersionId>,
) -> StoreResult<Vec<VersionId>> {
    list.sort_unstable();
    list.dedup();
    // The shown version must move forward, never sideways.
    if !list.contains(&head) || list.len() < 2 || list.len() > MAX_PARENTS {
        return Err(StoreError::Rejected(
            "a merge joins the head with other versions",
        ));
    }
    for parent in &list {
        if !belongs(connection, item, *parent)? {
            return Err(StoreError::Rejected(
                "merge parent is not a version of the item",
            ));
        }
    }
    Ok(list)
}

pub(super) fn add_version(
    connection: &Connection,
    version: ItemVersion,
    icon_asset: Option<EntryIconAsset>,
) -> StoreResult<()> {
    let ItemVersion {
        info:
            VersionInfo {
                id,
                item,
                mut parents,
                state,
                author,
                clock,
            },
        ciphertext,
    } = version;
    check_ciphertext(&ciphertext)?;
    check_state(connection, &state)?;
    author_param(&author)?;
    // Delivery may repeat; the same version twice is not an error.
    if let Some(owner) = owner_of(connection, id)? {
        return if owner == item {
            Ok(())
        } else {
            Err(StoreError::Rejected("version id belongs to another item"))
        };
    }
    store_icon_asset(connection, state.icon.as_ref(), icon_asset.as_ref())?;
    parents.sort_unstable();
    parents.dedup();
    if parents.len() > MAX_PARENTS {
        return Err(StoreError::Rejected("too many parent versions"));
    }

    let Some((_, head)) = current(connection, item)? else {
        if !parents.is_empty() {
            return Err(StoreError::Rejected("a parent version is missing"));
        }
        create(connection, item, id, &state, &author, clock, &ciphertext)?;
        tracing::debug!(item = %item.0, version = %id.0, "item received");
        return Ok(());
    };
    for parent in &parents {
        if !belongs(connection, item, *parent)? {
            return Err(StoreError::Rejected("a parent version is missing"));
        }
    }
    if head_template(connection, head)? != state.template.as_str() {
        return Err(StoreError::Rejected("an item cannot change its template"));
    }
    insert_version(
        connection,
        item,
        id,
        &parents,
        &state,
        &author,
        clock,
        &ciphertext,
    )?;
    advance_heads(connection, item, id, &parents)?;
    let edges = read::parents_of(connection, item)?;
    let fast_forward = graph::descends_from(&edges, id, head);
    if fast_forward {
        connection
            .execute(
                "UPDATE items SET head = ?1 WHERE id = ?2",
                params![text(id.0), text(item.0)],
            )
            .sql()?;
    }
    // Either the head moved or a conflict appeared: open editors are stale.
    bump_revision(connection, item)?;
    tracing::debug!(
        item = %item.0,
        version = %id.0,
        fast_forward,
        "item version received"
    );
    Ok(())
}

pub(super) fn remove(connection: &Connection, id: SecretId, base: u64) -> StoreResult<()> {
    let (stored, _) = current(connection, id)?.ok_or(StoreError::ItemNotFound)?;
    if stored != base {
        return Err(StoreError::Stale);
    }
    delete(connection, id)?;
    collect_garbage(connection)
}

/// Deletes an item with its whole history; its partner, if any, is bumped.
pub(super) fn delete(connection: &Connection, id: SecretId) -> StoreResult<()> {
    if let Some(other) = read::link_of(connection, id)? {
        bump_revision(connection, other)?;
    }
    // Versions, edges, heads, recipients and the link go by cascade; payloads
    // and images are collected by the caller once the whole change is done.
    connection
        .execute("DELETE FROM items WHERE id = ?1", [text(id.0)])
        .sql()?;
    Ok(())
}

/// Creates an item from a record whose revision is 0, as its first version.
pub(super) fn create_from_record(
    connection: &Connection,
    record: &ItemRecord,
    now_millis: u64,
) -> StoreResult<()> {
    let ItemRecord { meta, ciphertext } = record;
    check_ciphertext(ciphertext)?;
    check_state(connection, &meta.state)?;
    if meta.revision != 0 {
        return Err(StoreError::Rejected("a new item starts at revision 0"));
    }
    if current(connection, meta.id)?.is_some() {
        return Err(StoreError::Rejected("item already exists"));
    }
    create(
        connection,
        meta.id,
        meta.head,
        &meta.state,
        &Author::Local,
        Hlc::next(now_millis, []),
        ciphertext,
    )
}

/// Gives the item a new head that only changes its folder, sharing the
/// payload and recipients of the current head.
pub(super) fn move_head(
    connection: &Connection,
    item: SecretId,
    folder: Option<FolderId>,
) -> StoreResult<()> {
    let (_, head) = current(connection, item)?.ok_or(StoreError::ItemNotFound)?;
    let version = VersionId::new();
    let (millis, counter) =
        clock_params(Hlc::next(unix_millis(), clocks_of(connection, &[head])?))?;
    connection
        .execute(
            "INSERT INTO item_versions(id, item_id, payload, folder_id, title, template, \
                 favorite, trashed, icon_theme, icon_asset_id, icon_favicon_host, author, \
                 clock_millis, clock_counter) \
             SELECT ?1, item_id, payload, ?2, title, template, favorite, trashed, icon_theme, \
                 icon_asset_id, icon_favicon_host, NULL, ?3, ?4 FROM item_versions WHERE id = ?5",
            params![
                text(version.0),
                folder.map(|id| text(id.0)),
                millis,
                counter,
                text(head.0)
            ],
        )
        .sql()?;
    connection
        .execute(
            "INSERT INTO item_version_recipients(version_id, fingerprint) \
             SELECT ?1, fingerprint FROM item_version_recipients WHERE version_id = ?2",
            params![text(version.0), text(head.0)],
        )
        .sql()?;
    connection
        .execute(
            "INSERT INTO item_version_parents(version_id, parent_id) VALUES (?1, ?2)",
            params![text(version.0), text(head.0)],
        )
        .sql()?;
    advance_heads(connection, item, version, &[head])?;
    let changed = connection
        .execute(
            "UPDATE items SET head = ?1, revision = revision + 1 \
             WHERE id = ?2 AND revision < 9223372036854775807",
            params![text(version.0), text(item.0)],
        )
        .sql()?;
    if changed != 1 {
        return Err(StoreError::Rejected("item revision exhausted"));
    }
    Ok(())
}

fn create(
    connection: &Connection,
    item: SecretId,
    version: VersionId,
    state: &ItemState,
    author: &Author,
    clock: Hlc,
    ciphertext: &[u8],
) -> StoreResult<()> {
    // `items.head` is checked at commit, so the row can precede its version.
    connection
        .execute(
            "INSERT INTO items(id, head, revision) VALUES (?1, ?2, 0)",
            params![text(item.0), text(version.0)],
        )
        .sql()?;
    insert_version(
        connection,
        item,
        version,
        &[],
        state,
        author,
        clock,
        ciphertext,
    )?;
    connection
        .execute(
            "INSERT INTO item_heads(item_id, version_id) VALUES (?1, ?2)",
            params![text(item.0), text(version.0)],
        )
        .sql()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_version(
    connection: &Connection,
    item: SecretId,
    version: VersionId,
    parents: &[VersionId],
    state: &ItemState,
    author: &Author,
    clock: Hlc,
    ciphertext: &[u8],
) -> StoreResult<()> {
    let (millis, counter) = clock_params(clock)?;
    let hash = Sha256::digest(ciphertext).to_vec();
    connection
        .execute(
            "INSERT INTO item_payloads(hash, ciphertext) VALUES (?1, ?2) \
             ON CONFLICT(hash) DO NOTHING",
            params![hash, ciphertext],
        )
        .sql()?;
    let (theme, asset, host) = icon_columns(state.icon.as_ref());
    connection
        .execute(
            "INSERT INTO item_versions(id, item_id, payload, folder_id, title, template, \
                 favorite, trashed, icon_theme, icon_asset_id, icon_favicon_host, author, \
                 clock_millis, clock_counter) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                text(version.0),
                text(item.0),
                hash,
                state.folder.map(|id| text(id.0)),
                state.title,
                state.template.as_str(),
                state.favorite,
                state.trashed,
                theme,
                asset,
                host,
                author_param(author)?,
                millis,
                counter,
            ],
        )
        .sql()?;
    for fingerprint in state.recipients.as_slice() {
        connection
            .execute(
                "INSERT INTO item_version_recipients(version_id, fingerprint) VALUES (?1, ?2)",
                params![text(version.0), fingerprint.as_str()],
            )
            .sql()?;
    }
    for parent in parents {
        connection
            .execute(
                "INSERT INTO item_version_parents(version_id, parent_id) VALUES (?1, ?2)",
                params![text(version.0), text(parent.0)],
            )
            .sql()?;
    }
    Ok(())
}

/// The new version replaces its parents among the heads.
fn advance_heads(
    connection: &Connection,
    item: SecretId,
    version: VersionId,
    parents: &[VersionId],
) -> StoreResult<()> {
    for parent in parents {
        connection
            .execute(
                "DELETE FROM item_heads WHERE item_id = ?1 AND version_id = ?2",
                params![text(item.0), text(parent.0)],
            )
            .sql()?;
    }
    connection
        .execute(
            "INSERT INTO item_heads(item_id, version_id) VALUES (?1, ?2)",
            params![text(item.0), text(version.0)],
        )
        .sql()?;
    Ok(())
}

/// Stored revision and head of an item.
fn current(connection: &Connection, id: SecretId) -> StoreResult<Option<(u64, VersionId)>> {
    connection
        .query_row(
            "SELECT revision, head FROM items WHERE id = ?1",
            [text(id.0)],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .sql()?
        .map(|(stored, head)| Ok((revision(stored)?, version_id(&head)?)))
        .transpose()
}

pub(crate) fn belongs(
    connection: &Connection,
    item: SecretId,
    version: VersionId,
) -> StoreResult<bool> {
    Ok(owner_of(connection, version)? == Some(item))
}

fn owner_of(connection: &Connection, version: VersionId) -> StoreResult<Option<SecretId>> {
    connection
        .query_row(
            "SELECT item_id FROM item_versions WHERE id = ?1",
            [text(version.0)],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .sql()?
        .map(|raw| crate::convert::item_id(&raw))
        .transpose()
}

/// The clocks of `versions`, which must be stored.
fn clocks_of(connection: &Connection, versions: &[VersionId]) -> StoreResult<Vec<Hlc>> {
    versions
        .iter()
        .map(|version| {
            let (millis, counter) = connection
                .query_row(
                    "SELECT clock_millis, clock_counter FROM item_versions WHERE id = ?1",
                    [text(version.0)],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .sql()?;
            clock(millis, counter)
        })
        .collect()
}

fn head_template(connection: &Connection, head: VersionId) -> StoreResult<String> {
    connection
        .query_row(
            "SELECT template FROM item_versions WHERE id = ?1",
            [text(head.0)],
            |row| row.get(0),
        )
        .sql()
}

/// Bounds the store enforces whatever the caller checked: a printable,
/// non-empty title and an existing folder.
fn check_state(connection: &Connection, state: &ItemState) -> StoreResult<()> {
    if !is_clean_line(&state.title, MAX_TITLE_BYTES) {
        return Err(StoreError::Rejected("invalid title"));
    }
    if let Some(folder) = state.folder {
        require_folder(connection, folder)?;
    }
    Ok(())
}
