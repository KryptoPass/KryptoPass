//! Store identity and the layout of schema version 2.
//!
//! Items are version graphs. `items` holds identity, the head shown and the
//! local revision; `item_versions` holds immutable states, `item_version_parents`
//! the edges between them, `item_heads` the versions without children, and
//! `item_payloads` the ciphertexts by SHA-256, so versions that only change
//! metadata share their payload. `default_recipients` and `key_settings`
//! hold the vault's settings; no table singles out a key.
//!
//! Version 1 is the store of `kp-desktop/src/storage`. Each refuses the
//! other's files; there is no migration because no vault is in production.

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::Path;

pub(crate) const SCHEMA_VERSION: i64 = 2;
pub(crate) const APPLICATION_ID: i64 = 0x4b50_5331;

const SCHEMA: &str = "
CREATE TABLE folders (
    id TEXT PRIMARY KEY NOT NULL,
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
CREATE TABLE items (
    id TEXT PRIMARY KEY NOT NULL,
    head TEXT NOT NULL REFERENCES item_versions(id) DEFERRABLE INITIALLY DEFERRED,
    revision INTEGER NOT NULL CHECK(revision >= 0 AND typeof(revision) = 'integer')
);
CREATE TABLE item_payloads (
    hash BLOB PRIMARY KEY NOT NULL CHECK(typeof(hash) = 'blob' AND length(hash) = 32),
    ciphertext BLOB NOT NULL CHECK(typeof(ciphertext) = 'blob' AND length(ciphertext) > 0)
);
CREATE TABLE item_versions (
    id TEXT PRIMARY KEY NOT NULL,
    item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    payload BLOB NOT NULL REFERENCES item_payloads(hash) ON DELETE RESTRICT,
    -- History may outlive a folder; the version then shows at the root.
    folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
    title TEXT NOT NULL CHECK(length(trim(title)) > 0),
    template TEXT NOT NULL CHECK(length(template) > 0),
    favorite INTEGER NOT NULL CHECK(favorite IN (0, 1)),
    trashed INTEGER NOT NULL CHECK(trashed IN (0, 1)),
    icon_theme TEXT,
    icon_asset_id TEXT REFERENCES entry_icon_assets(id) ON DELETE RESTRICT,
    icon_favicon_host TEXT,
    -- NULL for this device, otherwise the author's Sync device id.
    author TEXT CHECK(author IS NULL OR length(author) BETWEEN 1 AND 128),
    -- Hybrid logical clock: wall-clock milliseconds and a tie counter.
    clock_millis INTEGER NOT NULL CHECK(clock_millis >= 0 AND typeof(clock_millis) = 'integer'),
    clock_counter INTEGER NOT NULL
        CHECK(clock_counter BETWEEN 0 AND 4294967295 AND typeof(clock_counter) = 'integer'),
    CHECK((icon_theme IS NOT NULL) + (icon_asset_id IS NOT NULL)
          + (icon_favicon_host IS NOT NULL) <= 1)
);
CREATE TABLE item_version_recipients (
    version_id TEXT NOT NULL REFERENCES item_versions(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL,
    PRIMARY KEY(version_id, fingerprint)
);
CREATE TABLE item_version_parents (
    version_id TEXT NOT NULL REFERENCES item_versions(id) ON DELETE CASCADE,
    parent_id TEXT NOT NULL REFERENCES item_versions(id) ON DELETE CASCADE,
    PRIMARY KEY(version_id, parent_id),
    CHECK(version_id <> parent_id)
);
CREATE TABLE item_heads (
    item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    version_id TEXT NOT NULL REFERENCES item_versions(id) ON DELETE CASCADE,
    PRIMARY KEY(item_id, version_id)
);
CREATE TABLE item_links (
    first_item_id TEXT PRIMARY KEY NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    second_item_id TEXT NOT NULL UNIQUE REFERENCES items(id) ON DELETE CASCADE,
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
CREATE INDEX item_versions_by_item ON item_versions(item_id);
CREATE INDEX item_versions_by_folder ON item_versions(folder_id);
CREATE INDEX item_versions_by_payload ON item_versions(payload);
CREATE INDEX item_versions_by_icon ON item_versions(icon_asset_id);
CREATE INDEX item_version_parents_by_parent ON item_version_parents(parent_id);
CREATE INDEX item_version_recipients_by_fingerprint ON item_version_recipients(fingerprint);
CREATE INDEX items_by_head ON items(head);
";

/// One probe per table, so a damaged or foreign schema fails on open.
const PROBES: [&str; 12] = [
    "SELECT id, parent_id, name, icon, color, sort_position FROM folders LIMIT 0",
    "SELECT folder_id, fingerprint FROM folder_recipients LIMIT 0",
    "SELECT id, png FROM entry_icon_assets LIMIT 0",
    "SELECT id, head, revision FROM items LIMIT 0",
    "SELECT hash, ciphertext FROM item_payloads LIMIT 0",
    "SELECT id, item_id, payload, folder_id, title, template, favorite, trashed, \
     icon_theme, icon_asset_id, icon_favicon_host, author, clock_millis, clock_counter \
     FROM item_versions LIMIT 0",
    "SELECT version_id, fingerprint FROM item_version_recipients LIMIT 0",
    "SELECT version_id, parent_id FROM item_version_parents LIMIT 0",
    "SELECT item_id, version_id FROM item_heads LIMIT 0",
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
