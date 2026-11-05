//! Store identity and the layout of schema version 2.
//!
//! Two layers share the file. The Sync layer is the truth: every signed
//! version (`versions` with its parents, recipients and attachments), the
//! encrypted blobs they name, the deletion marks and which records were
//! exchanged with each peer. It holds versions this device cannot open as
//! they came. The local index is derived: `item_index` has the state of the
//! versions this device opened, and `items` the head shown for each readable
//! item with its local revision. `default_recipients` and `key_settings` hold
//! the vault's settings; no table singles out a key.
//!
//! Version 1 was the store Desktop had before this one. Its files are refused
//! and left untouched; there is no migration because no vault is in production.

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::Path;

pub(crate) const SCHEMA_VERSION: i64 = 2;
pub(crate) const APPLICATION_ID: i64 = 0x4b50_5331;

const SCHEMA: &str = "
-- Folders as this device shows them: each one's newest readable version,
-- with a parent known here and its own sibling order.
CREATE TABLE folders (
    id TEXT PRIMARY KEY NOT NULL,
    head TEXT NOT NULL REFERENCES folder_index(version_id) DEFERRABLE INITIALLY DEFERRED,
    parent_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    icon TEXT NOT NULL DEFAULT 'folder',
    color TEXT NOT NULL DEFAULT 'transparent',
    sort_position INTEGER NOT NULL DEFAULT 0
        CHECK(sort_position >= 0 AND typeof(sort_position) = 'integer'),
    CHECK(parent_id IS NULL OR parent_id <> id)
);
CREATE TABLE folder_recipients (
    folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL,
    PRIMARY KEY(folder_id, fingerprint)
);
CREATE TABLE entry_icon_assets (
    id TEXT PRIMARY KEY NOT NULL,
    png BLOB NOT NULL CHECK(typeof(png) = 'blob' AND length(png) BETWEEN 45 AND 524288)
);
-- Signed versions of items and folders, as Sync carries them.
CREATE TABLE versions (
    id TEXT PRIMARY KEY NOT NULL,
    record_kind INTEGER NOT NULL CHECK(record_kind IN (1, 2)),
    record_id TEXT NOT NULL,
    author BLOB NOT NULL CHECK(typeof(author) = 'blob' AND length(author) = 32),
    -- Hybrid logical clock: wall-clock milliseconds and a tie counter.
    clock_millis INTEGER NOT NULL CHECK(clock_millis >= 0 AND typeof(clock_millis) = 'integer'),
    clock_counter INTEGER NOT NULL
        CHECK(clock_counter BETWEEN 0 AND 4294967295 AND typeof(clock_counter) = 'integer'),
    state BLOB NOT NULL CHECK(typeof(state) = 'blob' AND length(state) BETWEEN 1 AND 262144),
    content_hash BLOB CHECK(content_hash IS NULL
        OR (typeof(content_hash) = 'blob' AND length(content_hash) = 32)),
    signature BLOB NOT NULL CHECK(typeof(signature) = 'blob' AND length(signature) = 64),
    hash BLOB NOT NULL UNIQUE CHECK(typeof(hash) = 'blob' AND length(hash) = 32),
    CHECK((record_kind = 1) = (content_hash IS NOT NULL))
);
CREATE TABLE version_parents (
    version_id TEXT NOT NULL REFERENCES versions(id) ON DELETE CASCADE,
    parent_id TEXT NOT NULL REFERENCES versions(id) ON DELETE CASCADE,
    PRIMARY KEY(version_id, parent_id),
    CHECK(version_id <> parent_id)
);
CREATE TABLE version_recipients (
    version_id TEXT NOT NULL REFERENCES versions(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL CHECK(length(fingerprint) IN (40, 64)),
    PRIMARY KEY(version_id, fingerprint)
);
CREATE TABLE version_attachments (
    version_id TEXT NOT NULL REFERENCES versions(id) ON DELETE CASCADE,
    hash BLOB NOT NULL CHECK(typeof(hash) = 'blob' AND length(hash) = 32),
    PRIMARY KEY(version_id, hash)
);
-- Encrypted content and attachments, by SHA-256.
CREATE TABLE blobs (
    hash BLOB PRIMARY KEY NOT NULL CHECK(typeof(hash) = 'blob' AND length(hash) = 32),
    bytes BLOB NOT NULL CHECK(typeof(bytes) = 'blob' AND length(bytes) BETWEEN 1 AND 2359296)
);
-- What stays of a record deleted for good.
CREATE TABLE deletions (
    record_id TEXT PRIMARY KEY NOT NULL,
    record_kind INTEGER NOT NULL CHECK(record_kind IN (1, 2)),
    author BLOB NOT NULL CHECK(typeof(author) = 'blob' AND length(author) = 32),
    clock_millis INTEGER NOT NULL CHECK(clock_millis >= 0 AND typeof(clock_millis) = 'integer'),
    clock_counter INTEGER NOT NULL
        CHECK(clock_counter BETWEEN 0 AND 4294967295 AND typeof(clock_counter) = 'integer'),
    signature BLOB NOT NULL CHECK(typeof(signature) = 'blob' AND length(signature) = 64),
    hash BLOB NOT NULL UNIQUE CHECK(typeof(hash) = 'blob' AND length(hash) = 32)
);
CREATE TABLE deletion_recipients (
    record_id TEXT NOT NULL REFERENCES deletions(record_id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL CHECK(length(fingerprint) IN (40, 64)),
    PRIMARY KEY(record_id, fingerprint)
);
-- Records exchanged with each peer; they keep going to it.
CREATE TABLE peer_records (
    peer BLOB NOT NULL CHECK(typeof(peer) = 'blob' AND length(peer) = 32),
    record_id TEXT NOT NULL,
    PRIMARY KEY(peer, record_id)
);
-- The state of folder versions this device opened.
CREATE TABLE folder_index (
    version_id TEXT PRIMARY KEY NOT NULL REFERENCES versions(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    -- The parent its writer chose; it may not be known here.
    parent TEXT,
    icon TEXT NOT NULL,
    color TEXT NOT NULL
);
CREATE TABLE folder_index_recipients (
    version_id TEXT NOT NULL REFERENCES folder_index(version_id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL CHECK(length(fingerprint) IN (40, 64)),
    PRIMARY KEY(version_id, fingerprint)
);
-- The state of item versions this device opened.
CREATE TABLE item_index (
    version_id TEXT PRIMARY KEY NOT NULL REFERENCES versions(id) ON DELETE CASCADE,
    -- The folder the state names, and the same folder once it is known here;
    -- until then, or once it is gone, the item shows at the root.
    wanted_folder TEXT,
    folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
    title TEXT NOT NULL CHECK(length(trim(title)) > 0),
    template TEXT NOT NULL CHECK(length(template) > 0),
    favorite INTEGER NOT NULL CHECK(favorite IN (0, 1)),
    trashed INTEGER NOT NULL CHECK(trashed IN (0, 1)),
    icon_theme TEXT,
    icon_asset_id TEXT REFERENCES entry_icon_assets(id) ON DELETE RESTRICT,
    icon_favicon_host TEXT,
    CHECK((icon_theme IS NOT NULL) + (icon_asset_id IS NOT NULL)
          + (icon_favicon_host IS NOT NULL) <= 1)
);
-- Readable items: the head shown and the local revision.
CREATE TABLE items (
    id TEXT PRIMARY KEY NOT NULL,
    head TEXT NOT NULL REFERENCES item_index(version_id) DEFERRABLE INITIALLY DEFERRED,
    revision INTEGER NOT NULL CHECK(revision >= 0 AND typeof(revision) = 'integer')
);
-- Local only: a link relates two items, not two versions.
CREATE TABLE item_links (
    first_item_id TEXT PRIMARY KEY NOT NULL,
    second_item_id TEXT NOT NULL UNIQUE,
    CHECK(first_item_id < second_item_id)
);
CREATE TRIGGER item_links_unique_endpoints_insert
BEFORE INSERT ON item_links
WHEN EXISTS (
    SELECT 1 FROM item_links
    WHERE first_item_id IN (NEW.first_item_id, NEW.second_item_id)
       OR second_item_id IN (NEW.first_item_id, NEW.second_item_id)
)
BEGIN
    SELECT RAISE(ABORT, 'item endpoint is already paired');
END;
-- Recipients of new items outside a folder with defaults. No key is the
-- vault's: these are only a starting selection.
CREATE TABLE default_recipients (
    fingerprint TEXT PRIMARY KEY NOT NULL CHECK(length(fingerprint) IN (40, 64))
);
-- Only keys whose settings differ from the defaults have a row.
CREATE TABLE key_settings (
    fingerprint TEXT PRIMARY KEY NOT NULL CHECK(length(fingerprint) IN (40, 64)),
    forget_on_lock INTEGER NOT NULL CHECK(forget_on_lock IN (0, 1))
);
CREATE INDEX folders_by_parent ON folders(parent_id);
CREATE INDEX versions_by_record ON versions(record_id);
CREATE INDEX versions_by_content ON versions(content_hash);
CREATE INDEX version_parents_by_parent ON version_parents(parent_id);
CREATE INDEX version_recipients_by_fingerprint ON version_recipients(fingerprint);
CREATE INDEX version_attachments_by_hash ON version_attachments(hash);
CREATE INDEX item_index_by_folder ON item_index(folder_id);
CREATE INDEX item_index_by_wanted_folder ON item_index(wanted_folder);
CREATE INDEX folders_by_head ON folders(head);
CREATE INDEX item_index_by_icon ON item_index(icon_asset_id);
CREATE INDEX items_by_head ON items(head);
";

/// One probe per table, so a damaged or foreign schema fails on open.
const PROBES: [&str; 18] = [
    "SELECT id, head, parent_id, name, icon, color, sort_position FROM folders LIMIT 0",
    "SELECT folder_id, fingerprint FROM folder_recipients LIMIT 0",
    "SELECT id, png FROM entry_icon_assets LIMIT 0",
    "SELECT id, record_kind, record_id, author, clock_millis, clock_counter, state, \
     content_hash, signature, hash FROM versions LIMIT 0",
    "SELECT version_id, parent_id FROM version_parents LIMIT 0",
    "SELECT version_id, fingerprint FROM version_recipients LIMIT 0",
    "SELECT version_id, hash FROM version_attachments LIMIT 0",
    "SELECT hash, bytes FROM blobs LIMIT 0",
    "SELECT record_id, record_kind, author, clock_millis, clock_counter, signature, hash \
     FROM deletions LIMIT 0",
    "SELECT record_id, fingerprint FROM deletion_recipients LIMIT 0",
    "SELECT peer, record_id FROM peer_records LIMIT 0",
    "SELECT version_id, name, parent, icon, color FROM folder_index LIMIT 0",
    "SELECT version_id, fingerprint FROM folder_index_recipients LIMIT 0",
    "SELECT version_id, wanted_folder, folder_id, title, template, favorite, trashed, \
     icon_theme, icon_asset_id, icon_favicon_host FROM item_index LIMIT 0",
    "SELECT id, head, revision FROM items LIMIT 0",
    "SELECT first_item_id, second_item_id FROM item_links LIMIT 0",
    "SELECT fingerprint FROM default_recipients LIMIT 0",
    "SELECT fingerprint, forget_on_lock FROM key_settings LIMIT 0",
];

pub(crate) fn initialize(connection: &mut Connection) -> Result<()> {
    if pragma_i64(connection, "user_version")? == 0 {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not initialize the store")?;
        if pragma_i64(&transaction, "user_version")? == 0 {
            let application_id = pragma_i64(&transaction, "application_id")?;
            ensure!(
                application_id == 0 || application_id == APPLICATION_ID,
                "database file belongs to another application"
            );
            let objects: i64 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master \
                     WHERE type IN ('table', 'index', 'view', 'trigger') \
                       AND name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )
                .context("could not inspect the store schema")?;
            ensure!(
                objects == 0,
                "store schema is unversioned; refusing to modify it"
            );
            transaction
                .execute_batch(SCHEMA)
                .map_err(|_| anyhow::anyhow!("could not create the store schema"))?;
            transaction
                .pragma_update(None, "application_id", APPLICATION_ID)
                .context("could not mark the store")?;
            transaction
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .context("could not version the store")?;
        }
        transaction
            .commit()
            .context("could not finish store initialization")?;
    }
    verify(connection)
}

/// Checks identity, version and every table, without changing anything.
pub(crate) fn verify(connection: &Connection) -> Result<()> {
    ensure!(
        pragma_i64(connection, "application_id")? == APPLICATION_ID,
        "database file is not a KryptoPass store"
    );
    ensure!(
        pragma_i64(connection, "user_version")? == SCHEMA_VERSION,
        "store schema version is unsupported"
    );
    for probe in PROBES {
        connection
            .prepare(probe)
            .map_err(|_| anyhow::anyhow!("store schema is corrupt or unsupported"))?;
    }
    Ok(())
}

/// Checks an existing file through a read-only connection before any
/// writable connection, pragma change or permission adjustment touches it.
pub(crate) fn prevalidate(path: &Path) -> Result<()> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| anyhow::anyhow!("existing file is not a readable KryptoPass store"))?;
    verify(&connection)
}

fn pragma_i64(connection: &Connection, name: &str) -> Result<i64> {
    connection
        .pragma_query_value(None, name, |row| row.get(0))
        .map_err(|_| anyhow::anyhow!("could not read store metadata"))
}
