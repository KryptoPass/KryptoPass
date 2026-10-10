//! SQLite storage for encrypted secret payloads and public organizing metadata.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use kryptopass_core::{
    DomainError, Folder, FolderAppearance, FolderId, KeyFingerprint, RecipientSet, Secret,
    SecretId, SecretKind, SecretMetadata, Storage,
};
use rusqlite::types::{Type, ValueRef};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Row, Transaction, TransactionBehavior, params,
};
use uuid::Uuid;

use super::MAX_CIPHERTEXT_BYTES;

const SCHEMA_VERSION: i64 = 2;
const APPLICATION_ID: i64 = 0x4b50_5331;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Persistent storage whose secret payload column contains ciphertext only.
pub struct SqliteStorage {
    connection: Connection,
}

impl SqliteStorage {
    /// Opens the encrypted store, creating its schema if this is a new file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        ensure_database_parent(path)?;
        ensure_database_file_is_safe(path)?;

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let mut connection = Connection::open_with_flags(path, flags)
            .map_err(|_| anyhow::anyhow!("could not open the encrypted store"))?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .context("could not configure the encrypted store")?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA synchronous = FULL;
                 PRAGMA trusted_schema = OFF;
                 PRAGMA secure_delete = ON;",
            )
            .map_err(|_| anyhow::anyhow!("could not configure the encrypted store"))?;
        set_private_database_permissions(path)?;
        initialize_schema(&mut connection)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|_| anyhow::anyhow!("could not enable durable store journaling"))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|_| anyhow::anyhow!("could not configure durable store journaling"))?;

        Ok(Self { connection })
    }

    /// Reads folder and secret metadata from the same SQLite snapshot.
    pub fn read_snapshot(&self) -> Result<(Vec<Folder>, Vec<SecretMetadata>)> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let folders = read_folders(&transaction)?;
        let secrets = read_secret_metadata(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading store metadata")?;
        Ok((folders, secrets))
    }
}

impl Storage for SqliteStorage {
    fn find_folder(&self, id: FolderId) -> Result<Option<Folder>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let folder = load_folder(&transaction, id)?;
        transaction
            .commit()
            .context("could not finish reading folder metadata")?;
        Ok(folder)
    }

    fn find_secret(&self, id: SecretId) -> Result<Option<Secret>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let secret = load_secret(&transaction, id)?;
        transaction
            .commit()
            .context("could not finish reading secret metadata")?;
        Ok(secret)
    }

    fn list_folders(&self) -> Result<Vec<Folder>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let folders = read_folders(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading folder metadata")?;
        Ok(folders)
    }

    fn list_secrets(&self) -> Result<Vec<Secret>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let secrets = read_secrets(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading secret metadata")?;
        Ok(secrets)
    }

    fn list_secret_metadata(&self) -> Result<Vec<SecretMetadata>> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("could not read the encrypted store")?;
        let secrets = read_secret_metadata(&transaction)?;
        transaction
            .commit()
            .context("could not finish reading secret list metadata")?;
        Ok(secrets)
    }

    fn insert_folder(&mut self, folder: Folder) -> Result<()> {
        validate_folder(&folder)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin folder update")?;
        validate_parent(&transaction, folder.id, folder.parent_id)?;

        transaction
            .execute(
                "INSERT INTO folders(id, parent_id, name, icon, color) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    folder.id.0.to_string(),
                    folder.parent_id.map(|id| id.0.to_string()),
                    folder.name,
                    folder.appearance.icon(),
                    folder.appearance.color(),
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not save folder"))?;
        replace_folder_recipients(&transaction, folder.id, folder.default_recipients.as_ref())?;
        transaction
            .commit()
            .context("could not finish folder update")
    }

    fn insert_secret(&mut self, secret: Secret) -> Result<()> {
        validate_secret(&secret)?;
        ensure!(
            secret.revision == 0,
            "a new secret must start at revision zero"
        );

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret update")?;
        transaction
            .execute(
                "INSERT INTO secrets(id, folder_id, title, kind, favorite, trashed, ciphertext, revision) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                params![
                    secret.id.0.to_string(),
                    secret.folder_id.map(|id| id.0.to_string()),
                    secret.title,
                    secret.kind.as_str(),
                    secret.favorite,
                    secret.trashed,
                    secret.ciphertext,
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not save secret"))?;
        replace_secret_recipients(&transaction, secret.id, &secret.recipients)?;
        transaction
            .commit()
            .context("could not finish secret update")
    }

    fn move_secret(&mut self, id: SecretId, destination: Option<FolderId>) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret update")?;
        if let Some(folder_id) = destination {
            ensure_folder_exists(&transaction, folder_id)?;
        }

        let changed = transaction
            .execute(
                "UPDATE secrets SET folder_id = ?1, revision = revision + 1 \
                 WHERE id = ?2 AND revision < 9223372036854775807",
                params![destination.map(|id| id.0.to_string()), id.0.to_string()],
            )
            .map_err(|_| anyhow::anyhow!("could not move secret"))?;
        ensure!(
            changed == 1,
            "secret does not exist or its revision is exhausted"
        );
        transaction
            .commit()
            .context("could not finish secret update")
    }

    fn replace_encryption(
        &mut self,
        id: SecretId,
        ciphertext: Vec<u8>,
        recipients: RecipientSet,
        expected_revision: u64,
    ) -> Result<()> {
        ensure!(!ciphertext.is_empty(), "encrypted payload cannot be empty");
        ensure!(
            ciphertext.len() <= MAX_CIPHERTEXT_BYTES,
            "encrypted payload exceeds the maximum size"
        );
        ensure!(
            !recipients.as_slice().is_empty(),
            "at least one recipient is required"
        );
        ensure!(
            expected_revision < i64::MAX as u64,
            "secret revision is exhausted",
        );

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret update")?;
        let changed = transaction
            .execute(
                "UPDATE secrets SET ciphertext = ?1, revision = revision + 1 \
                 WHERE id = ?2 AND revision = ?3",
                params![
                    ciphertext,
                    id.0.to_string(),
                    i64::try_from(expected_revision).context("secret revision is invalid")?,
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not update secret encryption"))?;
        ensure!(
            changed == 1,
            "secret has changed or no longer exists; reload before updating encryption",
        );
        replace_secret_recipients(&transaction, id, &recipients)?;
        transaction
            .commit()
            .context("could not finish secret encryption update")
    }

    fn replace_secret(&mut self, secret: Secret, expected_revision: u64) -> Result<()> {
        validate_secret(&secret)?;
        ensure!(
            secret.revision == expected_revision,
            "secret revision does not match the expected revision",
        );
        ensure!(
            expected_revision < i64::MAX as u64,
            "secret revision is exhausted",
        );

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret update")?;
        if let Some(folder_id) = secret.folder_id {
            ensure_folder_exists(&transaction, folder_id)?;
        }

        let changed = transaction
            .execute(
                "UPDATE secrets \
                 SET folder_id = ?1, title = ?2, kind = ?3, favorite = ?4, trashed = ?5, \
                     ciphertext = ?6, revision = revision + 1 \
                 WHERE id = ?7 AND revision = ?8",
                params![
                    secret.folder_id.map(|id| id.0.to_string()),
                    secret.title,
                    secret.kind.as_str(),
                    secret.favorite,
                    secret.trashed,
                    secret.ciphertext,
                    secret.id.0.to_string(),
                    i64::try_from(expected_revision).context("secret revision is invalid")?,
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not save secret"))?;
        ensure!(
            changed == 1,
            "secret has changed or no longer exists; reload before saving",
        );
        replace_secret_recipients(&transaction, secret.id, &secret.recipients)?;
        transaction
            .commit()
            .context("could not finish secret update")
    }

    fn replace_folder(&mut self, folder: Folder) -> Result<()> {
        validate_folder(&folder)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin folder update")?;
        validate_parent(&transaction, folder.id, folder.parent_id)?;

        let changed = transaction
            .execute(
                "UPDATE folders SET parent_id = ?1, name = ?2, icon = ?3, color = ?4 \
                 WHERE id = ?5",
                params![
                    folder.parent_id.map(|id| id.0.to_string()),
                    folder.name,
                    folder.appearance.icon(),
                    folder.appearance.color(),
                    folder.id.0.to_string(),
                ],
            )
            .map_err(|_| anyhow::anyhow!("could not save folder"))?;
        ensure!(changed == 1, "folder does not exist");
        replace_folder_recipients(&transaction, folder.id, folder.default_recipients.as_ref())?;
        transaction
            .commit()
            .context("could not finish folder update")
    }

    fn remove_secret(&mut self, id: SecretId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin secret deletion")?;
        let changed = transaction
            .execute("DELETE FROM secrets WHERE id = ?1", [id.0.to_string()])
            .map_err(|_| anyhow::anyhow!("could not delete secret"))?;
        ensure!(changed == 1, "secret does not exist");
        transaction
            .commit()
            .context("could not finish secret deletion")
    }

    fn remove_folder(&mut self, id: FolderId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("could not begin folder deletion")?;
        let parent_id: Option<String> = transaction
            .query_row(
                "SELECT parent_id FROM folders WHERE id = ?1",
                [id.0.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| anyhow::anyhow!("could not read folder"))?
            .ok_or_else(|| anyhow::anyhow!("folder does not exist"))?;

        transaction
            .execute(
                "UPDATE folders SET parent_id = ?1 WHERE parent_id = ?2",
                params![parent_id, id.0.to_string()],
            )
            .map_err(|_| anyhow::anyhow!("could not reparent child folders"))?;
        transaction
            .execute(
                "UPDATE secrets SET folder_id = ?1, revision = revision + 1 \
                 WHERE folder_id = ?2 AND revision < 9223372036854775807",
                params![parent_id, id.0.to_string()],
            )
            .map_err(|_| anyhow::anyhow!("could not reparent secrets"))?;

        let unreparented_secrets: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM secrets WHERE folder_id = ?1)",
                [id.0.to_string()],
                |row| row.get(0),
            )
            .map_err(|_| anyhow::anyhow!("could not verify secret reparenting"))?;
        ensure!(
            !unreparented_secrets,
            "a secret revision is exhausted; folder deletion was cancelled"
        );

        let changed = transaction
            .execute("DELETE FROM folders WHERE id = ?1", [id.0.to_string()])
            .map_err(|_| anyhow::anyhow!("could not delete folder"))?;
        ensure!(changed == 1, "folder does not exist");
        transaction
            .commit()
            .context("could not finish folder deletion")
    }
}

fn read_folders(connection: &Connection) -> Result<Vec<Folder>> {
    let mut statement = connection
        .prepare(
            "SELECT id, parent_id, name, icon, color \
             FROM folders ORDER BY name COLLATE NOCASE, id",
        )
        .context("could not read folder metadata")?;
    let rows = statement
        .query_map([], |row| {
            Ok(RawFolder {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                icon: row.get(3)?,
                color: row.get(4)?,
            })
        })
        .context("could not read folder metadata")?;
    let raw_folders = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("could not read folder metadata")?;
    drop(statement);

    let mut folders = Vec::with_capacity(raw_folders.len());
    for raw in raw_folders {
        let id = parse_folder_id(&raw.id)?;
        let parent_id = raw.parent_id.as_deref().map(parse_folder_id).transpose()?;
        let default_recipients = load_folder_recipients(connection, id)?;
        folders.push(Folder {
            id,
            parent_id,
            name: raw.name,
            appearance: parse_folder_appearance(&raw.icon, &raw.color)?,
            default_recipients,
        });
    }

    Ok(folders)
}

fn read_secrets(connection: &Connection) -> Result<Vec<Secret>> {
    let mut statement = connection
        .prepare(
            "SELECT id, folder_id, title, kind, favorite, trashed, revision, \
                    length(ciphertext), ciphertext \
             FROM secrets ORDER BY title COLLATE NOCASE, id",
        )
        .context("could not read secret metadata")?;
    let rows = statement
        .query_map([], |row| {
            Ok(RawSecret {
                id: row.get(0)?,
                folder_id: row.get(1)?,
                title: row.get(2)?,
                kind: row.get(3)?,
                favorite: row.get(4)?,
                trashed: row.get(5)?,
                revision: row.get(6)?,
                ciphertext: read_ciphertext(row, 7, 8)?,
            })
        })
        .context("could not read secret metadata")?;
    let raw_secrets = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("could not read secret metadata")?;
    drop(statement);

    let mut secrets = Vec::with_capacity(raw_secrets.len());
    for raw in raw_secrets {
        let id = parse_secret_id(&raw.id)?;
        let folder_id = raw.folder_id.as_deref().map(parse_folder_id).transpose()?;
        let recipients = load_secret_recipients(connection, id)?;
        secrets.push(Secret {
            id,
            folder_id,
            title: raw.title,
            kind: parse_kind(&raw.kind)?,
            recipients,
            ciphertext: raw.ciphertext,
            favorite: parse_bool(raw.favorite)?,
            trashed: parse_bool(raw.trashed)?,
            revision: parse_revision(raw.revision)?,
        });
    }

    Ok(secrets)
}

fn read_secret_metadata(connection: &Connection) -> Result<Vec<SecretMetadata>> {
    let mut statement = connection
        .prepare(
            "SELECT id, folder_id, title, kind, favorite, trashed, revision \
             FROM secrets ORDER BY title COLLATE NOCASE, id",
        )
        .context("could not read secret list metadata")?;
    let rows = statement
        .query_map([], |row| {
            Ok(RawSecretMetadata {
                id: row.get(0)?,
                folder_id: row.get(1)?,
                title: row.get(2)?,
                kind: row.get(3)?,
                favorite: row.get(4)?,
                trashed: row.get(5)?,
                revision: row.get(6)?,
            })
        })
        .context("could not read secret list metadata")?;
    let raw_secrets = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("could not read secret list metadata")?;
    drop(statement);

    let mut secrets = Vec::with_capacity(raw_secrets.len());
    for raw in raw_secrets {
        let id = parse_secret_id(&raw.id)?;
        let folder_id = raw.folder_id.as_deref().map(parse_folder_id).transpose()?;
        let recipients = load_secret_recipients(connection, id)?;
        secrets.push(SecretMetadata {
            id,
            folder_id,
            title: raw.title,
            kind: parse_kind(&raw.kind)?,
            recipients,
            favorite: parse_bool(raw.favorite)?,
            trashed: parse_bool(raw.trashed)?,
            revision: parse_revision(raw.revision)?,
        });
    }

    Ok(secrets)
}

fn read_ciphertext(
    row: &Row<'_>,
    length_index: usize,
    blob_index: usize,
) -> rusqlite::Result<Vec<u8>> {
    let stored_length: i64 = row.get(length_index)?;
    let stored_length = usize::try_from(stored_length).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(length_index, Type::Integer, Box::new(error))
    })?;
    if stored_length > MAX_CIPHERTEXT_BYTES {
        return Err(column_data_error(
            length_index,
            Type::Integer,
            "encrypted payload exceeds the maximum size",
        ));
    }

    let bytes = match row.get_ref(blob_index)? {
        ValueRef::Blob(bytes) => bytes,
        value => {
            return Err(rusqlite::Error::InvalidColumnType(
                blob_index,
                "ciphertext".to_owned(),
                value.data_type(),
            ));
        }
    };
    if bytes.len() != stored_length || bytes.len() > MAX_CIPHERTEXT_BYTES {
        return Err(column_data_error(
            blob_index,
            Type::Blob,
            "encrypted payload length is invalid",
        ));
    }

    Ok(bytes.to_vec())
}

fn column_data_error(index: usize, column_type: Type, message: &'static str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        column_type,
        Box::new(io::Error::new(io::ErrorKind::InvalidData, message)),
    )
}

fn initialize_schema(connection: &mut Connection) -> Result<()> {
    let version = pragma_i64(connection, "user_version")?;
    if matches!(version, 0 | 1) {
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
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .context("could not version the encrypted store")
}

fn create_schema(transaction: &Transaction<'_>) -> Result<()> {
    transaction
        .execute_batch(
            "CREATE TABLE folders (
                 id TEXT PRIMARY KEY NOT NULL,
                 parent_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
                 name TEXT NOT NULL CHECK(length(trim(name)) > 0),
                 icon TEXT NOT NULL DEFAULT 'folder',
                 color TEXT NOT NULL DEFAULT 'transparent',
                 CHECK(parent_id IS NULL OR parent_id <> id)
             );
             CREATE TABLE folder_recipients (
                 folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                 fingerprint TEXT NOT NULL,
                 PRIMARY KEY(folder_id, fingerprint)
             );
             CREATE TABLE secrets (
                 id TEXT PRIMARY KEY NOT NULL,
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
             CREATE INDEX folders_by_parent ON folders(parent_id);
             CREATE INDEX secrets_by_folder ON secrets(folder_id);
             CREATE INDEX secrets_by_favorite ON secrets(favorite, trashed);
             CREATE INDEX secret_recipients_by_fingerprint ON secret_recipients(fingerprint);",
        )
        .map_err(|_| anyhow::anyhow!("could not create the encrypted store schema"))
}

fn ensure_schema_is_readable(connection: &Connection) -> Result<()> {
    connection
        .prepare("SELECT id, parent_id, name, icon, color FROM folders LIMIT 0")
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

fn pragma_i64(connection: &Connection, name: &str) -> Result<i64> {
    connection
        .pragma_query_value(None, name, |row| row.get(0))
        .map_err(|_| anyhow::anyhow!("could not read encrypted store metadata"))
}

fn load_folder(connection: &Connection, id: FolderId) -> Result<Option<Folder>> {
    let raw = connection
        .query_row(
            "SELECT id, parent_id, name, icon, color FROM folders WHERE id = ?1",
            [id.0.to_string()],
            |row| {
                Ok(RawFolder {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    name: row.get(2)?,
                    icon: row.get(3)?,
                    color: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|_| anyhow::anyhow!("could not read folder"))?;
    raw.map(|raw| {
        let id = parse_folder_id(&raw.id)?;
        let parent_id = raw.parent_id.as_deref().map(parse_folder_id).transpose()?;
        let default_recipients = load_folder_recipients(connection, id)?;
        Ok(Folder {
            id,
            parent_id,
            name: raw.name,
            appearance: parse_folder_appearance(&raw.icon, &raw.color)?,
            default_recipients,
        })
    })
    .transpose()
}

fn load_secret(connection: &Connection, id: SecretId) -> Result<Option<Secret>> {
    let raw = connection
        .query_row(
            "SELECT id, folder_id, title, kind, favorite, trashed, revision, \
                    length(ciphertext), ciphertext \
             FROM secrets WHERE id = ?1",
            [id.0.to_string()],
            |row| {
                Ok(RawSecret {
                    id: row.get(0)?,
                    folder_id: row.get(1)?,
                    title: row.get(2)?,
                    kind: row.get(3)?,
                    favorite: row.get(4)?,
                    trashed: row.get(5)?,
                    revision: row.get(6)?,
                    ciphertext: read_ciphertext(row, 7, 8)?,
                })
            },
        )
        .optional()
        .map_err(|_| anyhow::anyhow!("could not read secret"))?;
    raw.map(|raw| {
        let id = parse_secret_id(&raw.id)?;
        let folder_id = raw.folder_id.as_deref().map(parse_folder_id).transpose()?;
        Ok(Secret {
            id,
            folder_id,
            title: raw.title,
            kind: parse_kind(&raw.kind)?,
            recipients: load_secret_recipients(connection, id)?,
            ciphertext: raw.ciphertext,
            favorite: parse_bool(raw.favorite)?,
            trashed: parse_bool(raw.trashed)?,
            revision: parse_revision(raw.revision)?,
        })
    })
    .transpose()
}

fn load_folder_recipients(connection: &Connection, id: FolderId) -> Result<Option<RecipientSet>> {
    let fingerprints = connection
        .prepare(
            "SELECT fingerprint FROM folder_recipients WHERE folder_id = ?1 ORDER BY fingerprint",
        )
        .and_then(|mut statement| {
            let rows = statement.query_map([id.0.to_string()], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| anyhow::anyhow!("stored folder recipient metadata is invalid"))?;
    if fingerprints.is_empty() {
        return Ok(None);
    }

    parse_recipient_set(fingerprints).map(Some)
}

fn load_secret_recipients(connection: &Connection, id: SecretId) -> Result<RecipientSet> {
    let fingerprints = connection
        .prepare(
            "SELECT fingerprint FROM secret_recipients WHERE secret_id = ?1 ORDER BY fingerprint",
        )
        .and_then(|mut statement| {
            let rows = statement.query_map([id.0.to_string()], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| anyhow::anyhow!("stored secret recipient metadata is invalid"))?;
    ensure!(
        !fingerprints.is_empty(),
        "stored secret recipient metadata is invalid",
    );
    parse_recipient_set(fingerprints)
}

fn parse_recipient_set(values: Vec<String>) -> Result<RecipientSet> {
    let fingerprints = values
        .iter()
        .map(|value| {
            KeyFingerprint::parse(value)
                .map_err(|_| anyhow::anyhow!("stored recipient fingerprint is invalid"))
        })
        .collect::<Result<Vec<_>>>()?;
    RecipientSet::new(fingerprints)
        .map_err(|_| anyhow::anyhow!("stored recipient metadata is invalid"))
}

fn parse_folder_appearance(icon: &str, color: &str) -> Result<FolderAppearance> {
    FolderAppearance::new(icon.to_owned(), color.to_owned())
        .map_err(|_| anyhow::anyhow!("stored folder appearance is invalid"))
}

fn replace_folder_recipients(
    transaction: &Transaction<'_>,
    id: FolderId,
    recipients: Option<&RecipientSet>,
) -> Result<()> {
    transaction
        .execute(
            "DELETE FROM folder_recipients WHERE folder_id = ?1",
            [id.0.to_string()],
        )
        .map_err(|_| anyhow::anyhow!("could not update folder recipients"))?;
    if let Some(recipients) = recipients {
        for fingerprint in recipients.as_slice() {
            transaction
                .execute(
                    "INSERT INTO folder_recipients(folder_id, fingerprint) VALUES (?1, ?2)",
                    params![id.0.to_string(), fingerprint.as_str()],
                )
                .map_err(|_| anyhow::anyhow!("could not update folder recipients"))?;
        }
    }
    Ok(())
}

fn replace_secret_recipients(
    transaction: &Transaction<'_>,
    id: SecretId,
    recipients: &RecipientSet,
) -> Result<()> {
    transaction
        .execute(
            "DELETE FROM secret_recipients WHERE secret_id = ?1",
            [id.0.to_string()],
        )
        .map_err(|_| anyhow::anyhow!("could not update secret recipients"))?;
    for fingerprint in recipients.as_slice() {
        transaction
            .execute(
                "INSERT INTO secret_recipients(secret_id, fingerprint) VALUES (?1, ?2)",
                params![id.0.to_string(), fingerprint.as_str()],
            )
            .map_err(|_| anyhow::anyhow!("could not update secret recipients"))?;
    }
    Ok(())
}

fn ensure_folder_exists(connection: &Connection, id: FolderId) -> Result<()> {
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

fn validate_parent(
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

fn validate_folder(folder: &Folder) -> Result<()> {
    ensure!(
        !folder.name.trim().is_empty(),
        "folder name cannot be empty"
    );
    Ok(())
}

fn validate_secret(secret: &Secret) -> Result<()> {
    ensure!(
        !secret.title.trim().is_empty(),
        "secret title cannot be empty"
    );
    ensure!(
        !secret.ciphertext.is_empty(),
        "encrypted payload cannot be empty"
    );
    ensure!(
        secret.ciphertext.len() <= MAX_CIPHERTEXT_BYTES,
        "encrypted payload exceeds the maximum size"
    );
    ensure!(
        !secret.recipients.as_slice().is_empty(),
        "at least one recipient is required"
    );
    Ok(())
}

fn parse_kind(value: &str) -> Result<SecretKind> {
    SecretKind::parse(value).map_err(|error| match error {
        DomainError::UnsupportedSecretKind => {
            anyhow::anyhow!("stored secret kind is unsupported")
        }
        _ => anyhow::anyhow!("stored secret kind is invalid"),
    })
}

fn parse_bool(value: i64) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => bail!("stored boolean metadata is invalid"),
    }
}

fn parse_revision(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| anyhow::anyhow!("stored secret revision is invalid"))
}

fn parse_folder_id(value: &str) -> Result<FolderId> {
    parse_uuid(value).map(FolderId)
}

fn parse_secret_id(value: &str) -> Result<SecretId> {
    parse_uuid(value).map(SecretId)
}

fn parse_uuid(value: &str) -> Result<Uuid> {
    let id = Uuid::parse_str(value).map_err(|_| anyhow::anyhow!("stored identifier is invalid"))?;
    ensure!(
        id.to_string() == value,
        "stored identifier is not canonical"
    );
    Ok(id)
}

fn ensure_database_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let existed = match fs::symlink_metadata(parent) {
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(_) => bail!("could not inspect the store directory"),
    };
    if !existed {
        fs::create_dir_all(parent).context("could not create the store directory")?;
    }

    let directory = open_directory(parent)?;
    let metadata = directory
        .metadata()
        .context("could not inspect the store directory")?;
    ensure_owned_by_process(&metadata)?;
    if existed {
        ensure_private_directory(&metadata)?;
    } else {
        set_private_directory_permissions(&directory, &metadata)?;
    }
    Ok(())
}

fn ensure_database_file_is_safe(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                !metadata.file_type().is_symlink() && metadata.is_file(),
                "store path must be a regular file"
            );
            let file = open_database_file(path)
                .map_err(|_| anyhow::anyhow!("could not inspect the store file"))?;
            let opened_metadata = file
                .metadata()
                .context("could not inspect the store file")?;
            validate_database_file(&opened_metadata)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => bail!("could not inspect the store file"),
    }
}

fn set_private_database_permissions(path: &Path) -> Result<()> {
    let file = open_database_file(path).context("could not inspect the store file")?;
    let metadata = file
        .metadata()
        .context("could not inspect the store file")?;
    validate_database_file(&metadata)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o7777 != 0o600 {
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .context("could not secure the store file")?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn ensure_private_directory(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    ensure!(
        metadata.permissions().mode() & 0o7777 == 0o700,
        "store directory must have private 0700 permissions"
    );
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_directory(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(directory: &File, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    ensure_owned_by_process(metadata)?;
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        directory
            .set_permissions(fs::Permissions::from_mode(0o700))
            .context("could not secure the store directory")?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_directory: &File, _metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

fn open_directory(path: &Path) -> Result<File> {
    #[cfg(unix)]
    let directory = {
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .context("could not open store directory without following links")?
    };

    #[cfg(not(unix))]
    let directory = OpenOptions::new()
        .read(true)
        .open(path)
        .context("could not open store directory")?;

    ensure!(
        directory
            .metadata()
            .context("could not inspect store directory")?
            .is_dir(),
        "store directory must be a real directory"
    );
    Ok(directory)
}

fn open_database_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)
    }

    #[cfg(not(unix))]
    {
        OpenOptions::new().read(true).open(path)
    }
}

fn validate_database_file(metadata: &fs::Metadata) -> Result<()> {
    ensure!(metadata.is_file(), "store path must be a regular file");
    ensure_single_link(metadata)?;
    ensure_owned_by_process(metadata)?;
    Ok(())
}

#[cfg(unix)]
fn ensure_owned_by_process(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let effective_uid = unsafe { libc::geteuid() };
    ensure!(
        metadata.uid() == effective_uid,
        "store path is not owned by the current user"
    );
    Ok(())
}

#[cfg(not(unix))]
fn ensure_owned_by_process(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn ensure_single_link(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    ensure!(metadata.nlink() == 1, "store file must not have hard links");
    Ok(())
}

#[cfg(not(unix))]
fn ensure_single_link(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[derive(Debug)]
struct RawFolder {
    id: String,
    parent_id: Option<String>,
    name: String,
    icon: String,
    color: String,
}

#[derive(Debug)]
struct RawSecret {
    id: String,
    folder_id: Option<String>,
    title: String,
    kind: String,
    favorite: i64,
    trashed: i64,
    ciphertext: Vec<u8>,
    revision: i64,
}

#[derive(Debug)]
struct RawSecretMetadata {
    id: String,
    folder_id: Option<String>,
    title: String,
    kind: String,
    favorite: i64,
    trashed: i64,
    revision: i64,
}
