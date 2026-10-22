//! Store identity, schema creation, and versioned migrations.

use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};
use std::path::Path;
pub(super) const SCHEMA_VERSION: i64 = 6;
pub(super) const APPLICATION_ID: i64 = 0x4b50_5331;

pub(super) fn initialize_schema(connection: &mut Connection) -> Result<()> {
    let version = pragma_i64(connection, "user_version")?;
    if matches!(version, 0..=5) {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not initialize the encrypted store")?;
        let locked_version = pragma_i64(&transaction, "user_version")?;
        match locked_version {
            0 => {
                let existing_application_id = pragma_i64(&transaction, "application_id")?;
                ensure!(
                    existing_application_id == 0 || existing_application_id == APPLICATION_ID,
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
                    .context("could not inspect the encrypted store schema")?;
                ensure!(
                    objects == 0,
                    "store schema is unversioned; refusing to modify it",
                );
                create_schema(&transaction)?;
                transaction
                    .pragma_update(None, "application_id", APPLICATION_ID)
                    .context("could not mark the encrypted store")?;
                transaction
                    .pragma_update(None, "user_version", SCHEMA_VERSION)
                    .context("could not version the encrypted store")?;
            }
            1 => {
                let application_id = pragma_i64(&transaction, "application_id")?;
                ensure!(
                    application_id == APPLICATION_ID,
                    "database file is not a KryptoPass store",
                );
                migrate_schema_v1_to_v2(&transaction)?;
                migrate_schema_v2_to_v3(&transaction)?;
                migrate_schema_v3_to_v4(&transaction)?;
                migrate_schema_v4_to_v5(&transaction)?;
                migrate_schema_v5_to_v6(&transaction)?;
            }
            2 => {
                ensure!(
                    pragma_i64(&transaction, "application_id")? == APPLICATION_ID,
                    "database file is not a KryptoPass store"
                );
                migrate_schema_v2_to_v3(&transaction)?;
                migrate_schema_v3_to_v4(&transaction)?;
                migrate_schema_v4_to_v5(&transaction)?;
                migrate_schema_v5_to_v6(&transaction)?;
            }
            3 => {
                ensure!(
                    pragma_i64(&transaction, "application_id")? == APPLICATION_ID,
                    "database file is not a KryptoPass store"
                );
                migrate_schema_v3_to_v4(&transaction)?;
                migrate_schema_v4_to_v5(&transaction)?;
                migrate_schema_v5_to_v6(&transaction)?;
            }
            4 => {
                ensure!(
                    pragma_i64(&transaction, "application_id")? == APPLICATION_ID,
                    "database file is not a KryptoPass store"
                );
                migrate_schema_v4_to_v5(&transaction)?;
                migrate_schema_v5_to_v6(&transaction)?;
            }
            5 => {
                ensure!(
                    pragma_i64(&transaction, "application_id")? == APPLICATION_ID,
                    "database file is not a KryptoPass store"
                );
                migrate_schema_v5_to_v6(&transaction)?;
            }
            SCHEMA_VERSION => {}
            _ => bail!("store schema version is unsupported"),
        }
        transaction
            .commit()
            .context("could not finish encrypted store initialization")?;
    } else {
        ensure!(
            version == SCHEMA_VERSION,
            "store schema version is unsupported"
        );
    }

    let application_id = pragma_i64(connection, "application_id")?;
    ensure!(
        application_id == APPLICATION_ID,
        "database file is not a KryptoPass store",
    );
    ensure_schema_is_readable(connection)?;
    ensure_secret_links_schema_is_readable(connection)?;
    ensure_primary_key_schema_is_readable(connection)?;
    Ok(())
}

fn migrate_schema_v1_to_v2(transaction: &Transaction<'_>) -> Result<()> {
    transaction
        .execute_batch(
            "ALTER TABLE folders ADD COLUMN icon TEXT NOT NULL DEFAULT 'folder';
             ALTER TABLE folders ADD COLUMN color TEXT NOT NULL DEFAULT 'transparent';",
        )
        .map_err(|_| anyhow::anyhow!("could not migrate folder appearance metadata"))?;
    transaction
        .pragma_update(None, "user_version", 2)
        .context("could not version the encrypted store")
}

fn migrate_schema_v2_to_v3(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch(
        "ALTER TABLE folders ADD COLUMN sort_position INTEGER NOT NULL DEFAULT 0
             CHECK(sort_position >= 0 AND typeof(sort_position) = 'integer');
         WITH ordered AS (
             SELECT id, ROW_NUMBER() OVER (PARTITION BY parent_id ORDER BY name COLLATE NOCASE, id) - 1 AS position
             FROM folders
         )
         UPDATE folders SET sort_position = (SELECT position FROM ordered WHERE ordered.id = folders.id);"
    ).context("could not migrate folder order")?;
    transaction
        .pragma_update(None, "user_version", 3)
        .context("could not version the encrypted store")
}

fn migrate_schema_v3_to_v4(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch(
        "CREATE TABLE entry_icon_assets (id TEXT PRIMARY KEY NOT NULL, png BLOB NOT NULL
             CHECK(typeof(png) = 'blob' AND length(png) BETWEEN 45 AND 524288));
         ALTER TABLE secrets ADD COLUMN icon_theme TEXT;
         ALTER TABLE secrets ADD COLUMN icon_favicon_host TEXT;
         ALTER TABLE secrets ADD COLUMN icon_asset_id TEXT REFERENCES entry_icon_assets(id) ON DELETE RESTRICT;"
    ).context("could not migrate entry icons")?;
    transaction
        .pragma_update(None, "user_version", 4)
        .context("could not version entry icons")
}

fn migrate_schema_v4_to_v5(transaction: &Transaction<'_>) -> Result<()> {
    transaction
        .execute_batch(
            "CREATE TABLE secret_links (
                 first_secret_id TEXT PRIMARY KEY NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
                 second_secret_id TEXT NOT NULL UNIQUE REFERENCES secrets(id) ON DELETE CASCADE,
                 CHECK(first_secret_id < second_secret_id)
             );
             CREATE TRIGGER secret_links_unique_endpoints_insert
             BEFORE INSERT ON secret_links
             WHEN EXISTS (
                 SELECT 1 FROM secret_links
                 WHERE first_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
                    OR second_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
             )
             BEGIN
                 SELECT RAISE(ABORT, 'secret endpoint is already paired');
             END;
             CREATE TRIGGER secret_links_unique_endpoints_update
             BEFORE UPDATE ON secret_links
             WHEN EXISTS (
                 SELECT 1 FROM secret_links
                 WHERE (first_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
                    OR second_secret_id IN (NEW.first_secret_id, NEW.second_secret_id))
                   AND NOT (first_secret_id = OLD.first_secret_id
                        AND second_secret_id = OLD.second_secret_id)
             )
             BEGIN
                 SELECT RAISE(ABORT, 'secret endpoint is already paired');
             END;",
        )
        .context("could not migrate linked item metadata")?;
    transaction
        .pragma_update(None, "user_version", 5)
        .context("could not version linked item metadata")
}

const PRIMARY_KEY_TABLE: &str = "CREATE TABLE vault_primary_key (
         singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
         fingerprint TEXT NOT NULL CHECK(length(fingerprint) IN (40, 64))
     );";

fn migrate_schema_v5_to_v6(transaction: &Transaction<'_>) -> Result<()> {
    transaction
        .execute_batch(PRIMARY_KEY_TABLE)
        .context("could not migrate the vault primary key")?;
    transaction
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .context("could not version the vault primary key")
}

fn create_schema(transaction: &Transaction<'_>) -> Result<()> {
    transaction
        .execute_batch(
            "CREATE TABLE entry_icon_assets (id TEXT PRIMARY KEY NOT NULL, png BLOB NOT NULL
                 CHECK(typeof(png) = 'blob' AND length(png) BETWEEN 45 AND 524288));
             CREATE TABLE folders (
                 id TEXT PRIMARY KEY NOT NULL,
                 parent_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
                 name TEXT NOT NULL CHECK(length(trim(name)) > 0),
                 icon TEXT NOT NULL DEFAULT 'folder',
                 color TEXT NOT NULL DEFAULT 'transparent',
                 sort_position INTEGER NOT NULL DEFAULT 0 CHECK(sort_position >= 0 AND typeof(sort_position) = 'integer'),
                 CHECK(parent_id IS NULL OR parent_id <> id)
             );
             CREATE TABLE folder_recipients (
                 folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                 fingerprint TEXT NOT NULL,
                 PRIMARY KEY(folder_id, fingerprint)
             );
             CREATE TABLE secrets (
                 id TEXT PRIMARY KEY NOT NULL,
                 icon_theme TEXT,
                 icon_favicon_host TEXT,
                 icon_asset_id TEXT REFERENCES entry_icon_assets(id) ON DELETE RESTRICT,
                 folder_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
                 title TEXT NOT NULL CHECK(length(trim(title)) > 0),
                 kind TEXT NOT NULL,
                 favorite INTEGER NOT NULL CHECK(favorite IN (0, 1)),
                 trashed INTEGER NOT NULL CHECK(trashed IN (0, 1)),
                 ciphertext BLOB NOT NULL CHECK(length(ciphertext) > 0),
                 revision INTEGER NOT NULL CHECK(revision >= 0 AND typeof(revision) = 'integer')
             );
             CREATE TABLE secret_recipients (
                 secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
                 fingerprint TEXT NOT NULL,
                 PRIMARY KEY(secret_id, fingerprint)
             );
             CREATE TABLE secret_links (
                 first_secret_id TEXT PRIMARY KEY NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
                 second_secret_id TEXT NOT NULL UNIQUE REFERENCES secrets(id) ON DELETE CASCADE,
                 CHECK(first_secret_id < second_secret_id)
             );
             CREATE TRIGGER secret_links_unique_endpoints_insert
             BEFORE INSERT ON secret_links
             WHEN EXISTS (
                 SELECT 1 FROM secret_links
                 WHERE first_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
                    OR second_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
             )
             BEGIN
                 SELECT RAISE(ABORT, 'secret endpoint is already paired');
             END;
             CREATE TRIGGER secret_links_unique_endpoints_update
             BEFORE UPDATE ON secret_links
             WHEN EXISTS (
                 SELECT 1 FROM secret_links
                 WHERE (first_secret_id IN (NEW.first_secret_id, NEW.second_secret_id)
                    OR second_secret_id IN (NEW.first_secret_id, NEW.second_secret_id))
                   AND NOT (first_secret_id = OLD.first_secret_id
                        AND second_secret_id = OLD.second_secret_id)
             )
             BEGIN
                 SELECT RAISE(ABORT, 'secret endpoint is already paired');
             END;
             CREATE INDEX folders_by_parent ON folders(parent_id);
             CREATE INDEX secrets_by_folder ON secrets(folder_id);
             CREATE INDEX secrets_by_favorite ON secrets(favorite, trashed);
             CREATE INDEX secret_recipients_by_fingerprint ON secret_recipients(fingerprint);",
        )
        .and_then(|()| transaction.execute_batch(PRIMARY_KEY_TABLE))
        .map_err(|_| anyhow::anyhow!("could not create the encrypted store schema"))
}

fn ensure_schema_is_readable(connection: &Connection) -> Result<()> {
    connection
        .prepare("SELECT id, parent_id, name, icon, color, sort_position FROM folders LIMIT 0")
        .and_then(|_| {
            connection.prepare(
                "SELECT id, folder_id, title, kind, favorite, trashed, ciphertext, revision \
                 FROM secrets LIMIT 0",
            )
        })
        .and_then(|_| {
            connection.prepare("SELECT folder_id, fingerprint FROM folder_recipients LIMIT 0")
        })
        .and_then(|_| {
            connection.prepare("SELECT secret_id, fingerprint FROM secret_recipients LIMIT 0")
        })
        .map_err(|_| anyhow::anyhow!("store schema is corrupt or unsupported"))?;
    Ok(())
}

fn ensure_primary_key_schema_is_readable(connection: &Connection) -> Result<()> {
    connection
        .prepare("SELECT singleton, fingerprint FROM vault_primary_key LIMIT 0")
        .map_err(|_| anyhow::anyhow!("store schema is corrupt or unsupported"))?;
    Ok(())
}

fn ensure_secret_links_schema_is_readable(connection: &Connection) -> Result<()> {
    connection
        .prepare("SELECT first_secret_id, second_secret_id FROM secret_links LIMIT 0")
        .map_err(|_| anyhow::anyhow!("store schema is corrupt or unsupported"))?;
    Ok(())
}

/// Checks an existing vault using a read-only connection before any writable
/// connection, pragma changes, migration, or permission adjustment occurs.
pub(super) fn prevalidate_existing_database(path: &Path) -> Result<()> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| anyhow::anyhow!("existing file is not a readable KryptoPass store"))?;
    let application_id = pragma_i64(&connection, "application_id")?;
    let version = pragma_i64(&connection, "user_version")?;
    ensure!(
        application_id == APPLICATION_ID,
        "existing file is not a KryptoPass store"
    );
    ensure!(
        matches!(version, 1 | 2 | 3 | 4 | 5 | SCHEMA_VERSION),
        "existing store schema version is unsupported"
    );

    if version == 1 || version == 2 {
        connection
            .prepare("SELECT id, parent_id, name FROM folders LIMIT 0")
            .and_then(|_| {
                connection.prepare(
                    "SELECT id, folder_id, title, kind, favorite, trashed, ciphertext, revision \
                     FROM secrets LIMIT 0",
                )
            })
            .and_then(|_| {
                connection.prepare("SELECT folder_id, fingerprint FROM folder_recipients LIMIT 0")
            })
            .and_then(|_| {
                connection.prepare("SELECT secret_id, fingerprint FROM secret_recipients LIMIT 0")
            })
            .map_err(|_| anyhow::anyhow!("existing store schema is corrupt or unsupported"))?;
    } else {
        ensure_schema_is_readable(&connection)?;
        if version >= 5 {
            ensure_secret_links_schema_is_readable(&connection)?;
        }
        if version == SCHEMA_VERSION {
            ensure_primary_key_schema_is_readable(&connection)?;
        }
    }
    Ok(())
}

fn pragma_i64(connection: &Connection, name: &str) -> Result<i64> {
    connection
        .pragma_query_value(None, name, |row| row.get(0))
        .map_err(|_| anyhow::anyhow!("could not read encrypted store metadata"))
}
