//! Item writes: signed local versions, the local index, deletion for good.
//!
//! A local version is signed here, inside the transaction, because its clock
//! comes from its parents' and the signature covers it. The vault supplies
//! the state and content already encrypted; nothing here decrypts.

use kryptopass_core::vault::{
    AppendItem, Base, Hlc, IndexedVersion, ItemState, LinkChange, OpenedState, Parents, PutItem,
    StoreError, VersionId,
};
use kryptopass_core::{RecipientSet, SecretId};
use kryptopass_identity::DeviceIdentity;
use kryptopass_sync::{
    Clock, Deletion, DeletionDraft, Digest, Fingerprint, RecordKind, Version, VersionDraft,
    blob_hash,
};
use rusqlite::{Connection, OptionalExtension, params};

use super::{bump_revision, check_ciphertext, collect_garbage, links, store_icon_asset};
use crate::convert::{
    IconColumns, SqlResult, StoreResult, clock, clock_params, corrupt, fingerprint_bytes,
    fingerprint_text, icon_columns, id_bytes, id_from_bytes, item_id, revision, revision_param,
    text, version_id,
};
use crate::{graph, read};
use kryptopass_core::util::{has_control_characters, is_clean_line, unix_millis};

/// Sanity bound on a stored title; the vault applies the real, smaller limit.
const MAX_TITLE_BYTES: usize = 1024;
/// A merge joins at most this many versions, as Sync allows.
const MAX_PARENTS: usize = 16;

pub(super) fn put(
    connection: &Connection,
    identity: &DeviceIdentity,
    put: PutItem,
) -> StoreResult<()> {
    let PutItem {
        record,
        sealed_state,
        base,
        parents,
        now_millis,
        icon_asset,
        link,
    } = put;
    let meta = record.meta;
    check_state(connection, &meta.state)?;
    check_ciphertext(&record.ciphertext)?;
    store_icon_asset(connection, meta.state.icon.as_ref(), icon_asset.as_ref())?;
    match base {
        Base::New => {
            if meta.revision != 0 {
                return Err(StoreError::Rejected("a new item starts at revision 0"));
            }
            require_new(connection, meta.id)?;
            let local = LocalVersion {
                item: meta.id,
                version: meta.head,
                parents: &[],
                clock: Hlc::next(now_millis, []),
                state: &meta.state,
                sealed_state,
                content: &record.ciphertext,
            };
            local.write(connection, identity)?;
            insert_item(connection, meta.id, meta.head)?;
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
            if head_template(connection, head)? != meta.state.template.as_str() {
                return Err(StoreError::Rejected("an item cannot change its template"));
            }
            let local = LocalVersion {
                item: meta.id,
                version: meta.head,
                parents: &parents,
                clock: Hlc::next(now_millis, clocks_of(connection, &parents)?),
                state: &meta.state,
                sealed_state,
                content: &record.ciphertext,
            };
            local.write(connection, identity)?;
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

/// Creates an item from an import as its first version, its clock starting
/// at the time the source last wrote it.
pub(super) fn create_imported(
    connection: &Connection,
    identity: &DeviceIdentity,
    item: AppendItem,
) -> StoreResult<()> {
    let AppendItem {
        record,
        sealed_state,
        written_millis,
        previous,
    } = item;
    let mut meta = record.meta;
    check_state(connection, &meta.state)?;
    check_ciphertext(&record.ciphertext)?;
    if meta.revision != 0 {
        return Err(StoreError::Rejected("a new item starts at revision 0"));
    }
    require_new(connection, meta.id)?;
    let mut versions = previous
        .into_iter()
        .chain(std::iter::once((meta.head, record.ciphertext)));
    let Some((first_id, first_content)) = versions.next() else {
        return Ok(());
    };
    meta.head = first_id;
    put(
        connection,
        identity,
        PutItem {
            record: kryptopass_core::vault::ItemRecord {
                meta: meta.clone(),
                ciphertext: first_content,
            },
            sealed_state: sealed_state.clone(),
            base: Base::New,
            parents: Parents::Head,
            now_millis: written_millis,
            icon_asset: None,
            link: LinkChange::Keep,
        },
    )?;
    for (id, ciphertext) in versions {
        let base = meta.revision;
        meta.revision = base
            .checked_add(1)
            .ok_or(StoreError::Rejected("too many imported versions"))?;
        meta.head = id;
        put(
            connection,
            identity,
            PutItem {
                record: kryptopass_core::vault::ItemRecord {
                    meta: meta.clone(),
                    ciphertext,
                },
                sealed_state: sealed_state.clone(),
                base: Base::Revision(base),
                parents: Parents::Head,
                now_millis: written_millis,
                icon_asset: None,
                link: LinkChange::Keep,
            },
        )?;
    }
    Ok(())
}

/// Deletes an item for good at `base`, leaving a signed deletion mark.
pub(super) fn remove(
    connection: &Connection,
    identity: &DeviceIdentity,
    id: SecretId,
    base: u64,
) -> StoreResult<()> {
    let (stored, _) = current(connection, id)?.ok_or(StoreError::ItemNotFound)?;
    if stored != base {
        return Err(StoreError::Stale);
    }
    delete_with_mark(connection, identity, id)?;
    collect_garbage(connection)
}

/// Signs a deletion mark for an item and deletes everything else of it.
pub(super) fn delete_with_mark(
    connection: &Connection,
    identity: &DeviceIdentity,
    id: SecretId,
) -> StoreResult<()> {
    let deletion = sign_deletion(connection, identity, RecordKind::Item, id.0)?;
    purge_record(connection, &text(id.0))?;
    insert_deletion(connection, &deletion)
}

/// A signed deletion mark for a record, routed like it: to the recipients
/// of its current heads.
pub(super) fn sign_deletion(
    connection: &Connection,
    identity: &DeviceIdentity,
    kind: RecordKind,
    record: uuid::Uuid,
) -> StoreResult<Deletion> {
    let heads = record_heads(connection, &text(record))?;
    let mut recipients = Vec::new();
    for head in &heads {
        for fingerprint in read::version_fingerprints(connection, &text(head.0))? {
            let fingerprint = Fingerprint::new(fingerprint_bytes(&fingerprint)?)
                .map_err(|_| corrupt("recipient fingerprint"))?;
            if !recipients.contains(&fingerprint) {
                recipients.push(fingerprint);
            }
        }
    }
    let clock = Hlc::next(unix_millis(), clocks_of(connection, &heads)?);
    Deletion::sign(
        DeletionDraft {
            record_kind: kind,
            record_id: id_bytes(record),
            clock: Clock {
                millis: clock.millis,
                counter: clock.counter,
            },
            recipients,
        },
        identity,
    )
    .map_err(|_| StoreError::Rejected("deletion mark could not be signed"))
}

/// Deletes every version of a record with its index, item row and link.
/// Blobs and icon images are collected by the caller once the change is done.
pub(crate) fn purge_record(connection: &Connection, record: &str) -> StoreResult<()> {
    if let Ok(id) = item_id(record)
        && let Some(other) = read::link_of(connection, id)?
    {
        bump_revision(connection, other)?;
    }
    connection
        .execute(
            "DELETE FROM item_links WHERE first_item_id = ?1 OR second_item_id = ?1",
            [record],
        )
        .sql()?;
    connection
        .execute("DELETE FROM items WHERE id = ?1", [record])
        .sql()?;
    connection
        .execute("DELETE FROM versions WHERE record_id = ?1", [record])
        .sql()?;
    Ok(())
}

pub(crate) fn insert_deletion(connection: &Connection, deletion: &Deletion) -> StoreResult<()> {
    let hash = deletion
        .hash()
        .map_err(|_| StoreError::Rejected("invalid deletion mark"))?;
    let record = text(id_from_bytes(deletion.record_id)?);
    connection
        .execute(
            "INSERT INTO deletions(record_id, record_kind, author, clock_millis, clock_counter, \
                 signature, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record,
                i64::from(deletion.record_kind.as_wire()),
                deletion.author.as_bytes().as_slice(),
                millis_param(deletion.clock.millis)?,
                i64::from(deletion.clock.counter),
                deletion.signature.as_slice(),
                hash.as_slice(),
            ],
        )
        .sql()?;
    for fingerprint in &deletion.recipients {
        connection
            .execute(
                "INSERT INTO deletion_recipients(record_id, fingerprint) VALUES (?1, ?2)",
                params![record, fingerprint_text(fingerprint.as_bytes())?],
            )
            .sql()?;
    }
    Ok(())
}

/// Stores a signed version as it is, with the blobs it needs that are not
/// stored yet. Received versions and local ones go through here.
pub(crate) fn insert_signed(
    connection: &Connection,
    version: &Version,
    blobs: impl IntoIterator<Item = (Digest, Vec<u8>)>,
) -> StoreResult<()> {
    for (hash, bytes) in blobs {
        connection
            .execute(
                "INSERT INTO blobs(hash, bytes) VALUES (?1, ?2) ON CONFLICT(hash) DO NOTHING",
                params![hash.as_slice(), bytes],
            )
            .sql()?;
    }
    let hash = version
        .hash()
        .map_err(|_| StoreError::Rejected("invalid version"))?;
    let id = text(id_from_bytes(version.version_id)?);
    let record = text(id_from_bytes(version.record_id)?);
    connection
        .execute(
            "INSERT INTO versions(id, record_kind, record_id, author, clock_millis, \
                 clock_counter, state, content_hash, signature, hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                i64::from(version.record_kind.as_wire()),
                record,
                version.author.as_bytes().as_slice(),
                millis_param(version.clock.millis)?,
                i64::from(version.clock.counter),
                version.state,
                version.content_hash.as_ref().map(|hash| hash.to_vec()),
                version.signature.as_slice(),
                hash.as_slice(),
            ],
        )
        .sql()?;
    for parent in &version.parents {
        connection
            .execute(
                "INSERT INTO version_parents(version_id, parent_id) VALUES (?1, ?2)",
                params![id, text(id_from_bytes(*parent)?)],
            )
            .sql()?;
    }
    for fingerprint in &version.recipients {
        connection
            .execute(
                "INSERT INTO version_recipients(version_id, fingerprint) VALUES (?1, ?2)",
                params![id, fingerprint_text(fingerprint.as_bytes())?],
            )
            .sql()?;
    }
    for attachment in &version.attachments {
        connection
            .execute(
                "INSERT INTO version_attachments(version_id, hash) VALUES (?1, ?2)",
                params![id, attachment.as_slice()],
            )
            .sql()?;
    }
    for needed in version.blobs() {
        let stored: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM blobs WHERE hash = ?1)",
                [needed.as_slice()],
                |row| row.get(0),
            )
            .sql()?;
        if !stored {
            return Err(StoreError::Rejected("a blob of the version is missing"));
        }
    }
    Ok(())
}

/// Indexes versions the vault opened, then refreshes the items they belong to.
pub(super) fn index(connection: &Connection, versions: Vec<IndexedVersion>) -> StoreResult<()> {
    let mut items = Vec::new();
    for IndexedVersion { id, item, state } in versions {
        let key = text(id.0);
        let owner: Option<String> = connection
            .query_row(
                "SELECT record_id FROM versions WHERE id = ?1 AND record_kind = 1",
                [&key],
                |row| row.get(0),
            )
            .optional()
            .sql()?;
        // Versions are identified by their ids; the title only names the item for a person
        // and never decides whether a version is indexed.
        if is_indexed(connection, &key)? {
            continue;
        }
        if owner.as_deref() != Some(text(item.0).as_str()) {
            // Never silent: a readable version left out shows as unreadable in the list.
            tracing::warn!(version = %key, "received version not indexed: it does not belong to the item it names");
            continue;
        }
        insert_index(connection, id, &received_columns(connection, state)?)?;
        if !items.contains(&item) {
            items.push(item);
        }
    }
    for item in items {
        refresh_item(connection, item)?;
    }
    Ok(())
}

/// Chooses the head an item shows after its graph or index changed, and
/// bumps its revision so an open editor knows.
///
/// The head is an indexed version without children: the current one while
/// it still is one, else the newest descending from it, else the newest. With
/// no such version the item is not readable here and leaves the list.
pub(crate) fn refresh_item(connection: &Connection, item: SecretId) -> StoreResult<()> {
    let key = text(item.0);
    let mut readable = Vec::new();
    for head in graph_heads(connection, item)? {
        if is_indexed(connection, &text(head.0))? {
            readable.push(head);
        }
    }
    let shown = current(connection, item)?;
    if readable.is_empty() {
        connection
            .execute("DELETE FROM items WHERE id = ?1", [&key])
            .sql()?;
        return Ok(());
    }
    let candidates: Vec<(VersionId, Hlc)> = readable
        .iter()
        .copied()
        .zip(clocks_of(connection, &readable)?)
        .collect();
    let newest = |candidates: &mut dyn Iterator<Item = &(VersionId, Hlc)>| {
        candidates
            .max_by_key(|(version, clock)| (*clock, *version))
            .map(|(version, _)| *version)
    };
    let head = match shown {
        Some((_, head)) if readable.contains(&head) => head,
        Some((_, head)) => {
            let edges = read::parents_of(connection, item)?;
            newest(
                &mut candidates
                    .iter()
                    .filter(|(version, _)| graph::descends_from(&edges, *version, head)),
            )
            .or_else(|| newest(&mut candidates.iter()))
            .ok_or_else(|| corrupt("item heads"))?
        }
        None => newest(&mut candidates.iter()).ok_or_else(|| corrupt("item heads"))?,
    };
    if shown.is_none() {
        return insert_item(connection, item, head);
    }
    let changed = connection
        .execute(
            "UPDATE items SET head = ?1, revision = revision + 1 \
             WHERE id = ?2 AND revision < 9223372036854775807",
            params![text(head.0), key],
        )
        .sql()?;
    match changed {
        1 => Ok(()),
        _ => Err(StoreError::Rejected("item revision exhausted")),
    }
}

/// A version this device writes: signed, stored and indexed at once.
struct LocalVersion<'a> {
    item: SecretId,
    version: VersionId,
    parents: &'a [VersionId],
    clock: Hlc,
    state: &'a ItemState,
    sealed_state: Vec<u8>,
    content: &'a [u8],
}

impl LocalVersion<'_> {
    fn write(self, connection: &Connection, identity: &DeviceIdentity) -> StoreResult<()> {
        let content_hash = blob_hash(self.content);
        let signed = Version::sign(
            VersionDraft {
                record_kind: RecordKind::Item,
                record_id: id_bytes(self.item.0),
                version_id: id_bytes(self.version.0),
                parents: self
                    .parents
                    .iter()
                    .map(|parent| id_bytes(parent.0))
                    .collect(),
                clock: Clock {
                    millis: self.clock.millis,
                    counter: self.clock.counter,
                },
                recipients: sync_recipients(&self.state.recipients)?,
                state: self.sealed_state,
                content_hash: Some(content_hash),
                attachments: Vec::new(),
            },
            identity,
        )
        .map_err(|_| StoreError::Rejected("version could not be signed"))?;
        insert_signed(connection, &signed, [(content_hash, self.content.to_vec())])?;
        insert_index(
            connection,
            self.version,
            &IndexColumns::from_state(self.state),
        )
    }
}

/// The columns of an index row.
struct IndexColumns {
    /// The folder the state names, known here or not.
    wanted_folder: Option<String>,
    folder: Option<String>,
    title: String,
    template: String,
    favorite: bool,
    trashed: bool,
    icon: IconColumns,
}

impl IndexColumns {
    fn from_state(state: &ItemState) -> Self {
        Self {
            wanted_folder: state.folder.map(|folder| text(folder.0)),
            folder: state.folder.map(|folder| text(folder.0)),
            title: state.title.clone(),
            template: state.template.as_str().to_owned(),
            favorite: state.favorite,
            trashed: state.trashed,
            icon: icon_columns(state.icon.as_ref()),
        }
    }
}

/// The columns of a received state. A folder not known here is only wanted:
/// the item shows at the root until it arrives. An icon image not known here
/// is left out, for the automatic icon.
fn received_columns(connection: &Connection, state: OpenedState) -> StoreResult<IndexColumns> {
    let exists = |sql: &str, key: &str| -> StoreResult<bool> {
        connection.query_row(sql, [key], |row| row.get(0)).sql()
    };
    let folder = match state.folder {
        Some(folder)
            if exists(
                "SELECT EXISTS(SELECT 1 FROM folders WHERE id = ?1)",
                &text(folder.0),
            )? =>
        {
            Some(text(folder.0))
        }
        _ => None,
    };
    let mut icon = icon_columns(state.icon.as_ref());
    if let Some(asset) = &icon.1
        && !exists(
            "SELECT EXISTS(SELECT 1 FROM entry_icon_assets WHERE id = ?1)",
            asset,
        )?
    {
        icon = (None, None, None);
    }
    Ok(IndexColumns {
        wanted_folder: state.folder.map(|folder| text(folder.0)),
        folder,
        title: display_title(&state.title),
        template: state.template.as_str().to_owned(),
        favorite: state.favorite,
        trashed: state.trashed,
        icon,
    })
}

/// What the list shows for a title another device wrote. It is kept readable rather than
/// refused: control characters become spaces, the ends are trimmed, the length is bounded,
/// and a dash stands in when nothing printable is left (the index needs a non-empty title).
fn display_title(title: &str) -> String {
    let printable: String = title
        .chars()
        .map(|character| {
            let mut buffer = [0_u8; 4];
            if has_control_characters(character.encode_utf8(&mut buffer), false) {
                ' '
            } else {
                character
            }
        })
        .collect();
    let trimmed = printable.trim();
    let mut end = trimmed.len().min(MAX_TITLE_BYTES);
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    let bounded = trimmed[..end].trim_end();
    if bounded.is_empty() {
        "—".to_owned()
    } else {
        bounded.to_owned()
    }
}

fn insert_index(
    connection: &Connection,
    version: VersionId,
    columns: &IndexColumns,
) -> StoreResult<()> {
    let (theme, asset, host) = &columns.icon;
    connection
        .execute(
            "INSERT INTO item_index(version_id, wanted_folder, folder_id, title, template, \
                 favorite, trashed, icon_theme, icon_asset_id, icon_favicon_host) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                text(version.0),
                columns.wanted_folder,
                columns.folder,
                columns.title,
                columns.template,
                columns.favorite,
                columns.trashed,
                theme,
                asset,
                host,
            ],
        )
        .sql()?;
    Ok(())
}

fn insert_item(connection: &Connection, item: SecretId, head: VersionId) -> StoreResult<()> {
    connection
        .execute(
            "INSERT INTO items(id, head, revision) VALUES (?1, ?2, 0)",
            params![text(item.0), text(head.0)],
        )
        .sql()?;
    Ok(())
}

fn is_indexed(connection: &Connection, version: &str) -> StoreResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM item_index WHERE version_id = ?1)",
            [version],
            |row| row.get(0),
        )
        .sql()
}

fn sync_recipients(recipients: &RecipientSet) -> StoreResult<Vec<Fingerprint>> {
    recipients
        .as_slice()
        .iter()
        .map(|fingerprint| {
            Fingerprint::new(fingerprint_bytes(fingerprint.as_str())?)
                .map_err(|_| StoreError::Rejected("invalid recipient fingerprint"))
        })
        .collect()
}

/// A new item's id must be unused, and never one deleted for good.
fn require_new(connection: &Connection, id: SecretId) -> StoreResult<()> {
    let used: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM versions WHERE record_id = ?1) \
                 OR EXISTS(SELECT 1 FROM deletions WHERE record_id = ?1)",
            [text(id.0)],
            |row| row.get(0),
        )
        .sql()?;
    if used {
        Err(StoreError::Rejected("item already exists"))
    } else {
        Ok(())
    }
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

/// Versions of an item without children, readable here or not.
pub(crate) fn graph_heads(connection: &Connection, item: SecretId) -> StoreResult<Vec<VersionId>> {
    record_heads(connection, &text(item.0))
}

/// Versions of a record without children, readable here or not.
pub(crate) fn record_heads(connection: &Connection, record: &str) -> StoreResult<Vec<VersionId>> {
    let mut statement = connection
        .prepare(
            "SELECT v.id FROM versions v WHERE v.record_id = ?1 AND NOT EXISTS \
                 (SELECT 1 FROM version_parents p WHERE p.parent_id = v.id) ORDER BY v.id",
        )
        .sql()?;
    let rows = statement
        .query_map([record], |row| row.get::<_, String>(0))
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    rows.iter().map(|raw| version_id(raw)).collect()
}

/// Stored revision and head of a readable item.
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
    let owner: Option<String> = connection
        .query_row(
            "SELECT record_id FROM versions WHERE id = ?1",
            [text(version.0)],
            |row| row.get(0),
        )
        .optional()
        .sql()?;
    Ok(owner.as_deref() == Some(text(item.0).as_str()))
}

/// The clocks of `versions`, which must be stored.
pub(crate) fn clocks_of(connection: &Connection, versions: &[VersionId]) -> StoreResult<Vec<Hlc>> {
    versions
        .iter()
        .map(|version| {
            let (millis, counter) = connection
                .query_row(
                    "SELECT clock_millis, clock_counter FROM versions WHERE id = ?1",
                    [text(version.0)],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .sql()?;
            clock(millis, counter)
        })
        .collect()
}

fn millis_param(millis: u64) -> StoreResult<i64> {
    clock_params(Hlc { millis, counter: 0 }).map(|(millis, _)| millis)
}

fn head_template(connection: &Connection, head: VersionId) -> StoreResult<String> {
    connection
        .query_row(
            "SELECT template FROM item_index WHERE version_id = ?1",
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
        super::require_folder(connection, folder)?;
    }
    Ok(())
}
