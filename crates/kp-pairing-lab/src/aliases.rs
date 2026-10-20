//! Private local display metadata. It never participates in device authorization.

use kryptopass_identity::{DeviceId, PublicIdentity};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    path::Path,
    time::Duration,
};
use thiserror::Error;

const FILE: &str = "device-labels-v1.sqlite3";
const APP_ID: i64 = 0x4b50_4c42;
const MAX_SIZE: u64 = 1024 * 1024;

/// Redacted display-metadata failures; trust is retained on every error.
#[derive(Debug, Error)]
pub enum AliasError {
    #[error("device alias must contain at most 64 characters and 256 bytes, without controls")]
    InvalidName,
    #[error("device is not authorized")]
    UnknownDevice,
    #[error("local device names could not be loaded or saved")]
    Storage,
}

fn normalized(value: &str) -> Result<&str, AliasError> {
    let value = value.trim();
    if value.len() > 256 || value.chars().count() > 64 || value.chars().any(|ch| {
        ch.is_control() || matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    }) {
        return Err(AliasError::InvalidName);
    }
    Ok(value)
}

pub(super) fn read(
    directory: &Path,
    local_id: DeviceId,
    peers: &[PublicIdentity],
) -> Result<BTreeMap<DeviceId, String>, AliasError> {
    let path = directory.join(FILE);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err(AliasError::Storage),
        Ok(_) => {}
    }
    let connection = open(&path, local_id, false)?;
    let mut query = connection
        .prepare("SELECT device_id, label FROM labels ORDER BY device_id LIMIT 1025")
        .map_err(|_| AliasError::Storage)?;
    let mut rows = query.query([]).map_err(|_| AliasError::Storage)?;
    let mut result = BTreeMap::new();
    let mut count = 0;
    while let Some(row) = rows.next().map_err(|_| AliasError::Storage)? {
        count += 1;
        if count > 1024 {
            return Err(AliasError::Storage);
        }
        let bytes: Vec<u8> = row.get(0).map_err(|_| AliasError::Storage)?;
        let label: String = row.get(1).map_err(|_| AliasError::Storage)?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| AliasError::Storage)?;
        if bytes.iter().all(|byte| *byte == 0) || label.is_empty() || normalized(&label)? != label {
            return Err(AliasError::Storage);
        }
        let id = DeviceId::from_bytes(bytes);
        if peers.iter().any(|peer| peer.device_id == id) {
            result.insert(id, label);
        }
    }
    Ok(result)
}

pub(super) fn write(
    directory: &Path,
    local_id: DeviceId,
    peer_id: DeviceId,
    value: &str,
) -> Result<(), AliasError> {
    let label = normalized(value)?;
    let path = directory.join(FILE);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let created = match options.open(&path) {
        Ok(file) => {
            file.sync_all().map_err(|_| AliasError::Storage)?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(_) => return Err(AliasError::Storage),
    };
    let mut connection = open(&path, local_id, created)?;
    let tx = connection.transaction().map_err(|_| AliasError::Storage)?;
    if label.is_empty() {
        tx.execute(
            "DELETE FROM labels WHERE device_id = ?1",
            [peer_id.as_bytes().as_slice()],
        )
        .map_err(|_| AliasError::Storage)?;
    } else {
        let count: i64 = tx
            .query_row("SELECT count(*) FROM labels", [], |row| row.get(0))
            .map_err(|_| AliasError::Storage)?;
        let present: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM labels WHERE device_id = ?1)",
                [peer_id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(|_| AliasError::Storage)?;
        if !present && count >= 1024 {
            return Err(AliasError::Storage);
        }
        tx.execute("INSERT INTO labels(device_id,label) VALUES(?1,?2) ON CONFLICT(device_id) DO UPDATE SET label=excluded.label", params![peer_id.as_bytes().as_slice(), label]).map_err(|_| AliasError::Storage)?;
    }
    tx.commit().map_err(|_| AliasError::Storage)?;
    Ok(())
}

pub(super) fn forget(
    directory: &Path,
    local_id: DeviceId,
    peer_id: DeviceId,
) -> Result<(), AliasError> {
    let path = directory.join(FILE);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            for sidecar in reset_paths(directory).iter().skip(1) {
                match fs::symlink_metadata(sidecar) {
                    Ok(_) => return Err(AliasError::Storage),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err(AliasError::Storage),
                }
            }
            return Ok(());
        }
        Err(_) => return Err(AliasError::Storage),
        Ok(_) => {}
    }
    let mut connection = open(&path, local_id, false)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AliasError::Storage)?;
    tx.execute(
        "DELETE FROM labels WHERE device_id = ?1",
        [peer_id.as_bytes().as_slice()],
    )
    .map_err(|_| AliasError::Storage)?;
    tx.commit().map_err(|_| AliasError::Storage)
}

pub(super) fn validate_reset_targets(directory: &Path) -> Result<(), AliasError> {
    validate_directory(directory)?;
    for path in reset_paths(directory) {
        validate_reset_file(&path)?;
    }
    Ok(())
}

pub(super) fn reset_profile_files(directory: &Path) -> Result<(), AliasError> {
    validate_reset_targets(directory)?;
    let paths = reset_paths(directory);
    for path in paths.iter().skip(1).chain(paths.first()) {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(AliasError::Storage),
        }
    }
    for path in reset_paths(directory) {
        validate_reset_file(&path)?;
        if path.exists() {
            return Err(AliasError::Storage);
        }
    }
    Ok(())
}

fn reset_paths(directory: &Path) -> Vec<std::path::PathBuf> {
    let database = directory.join(FILE);
    let mut journal = database.as_os_str().to_os_string();
    journal.push("-journal");
    let mut shm = database.as_os_str().to_os_string();
    shm.push("-shm");
    let mut wal = database.as_os_str().to_os_string();
    wal.push("-wal");
    vec![
        database,
        std::path::PathBuf::from(journal),
        std::path::PathBuf::from(shm),
        std::path::PathBuf::from(wal),
    ]
}

#[cfg(unix)]
fn validate_directory(directory: &Path) -> Result<(), AliasError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(directory).map_err(|_| AliasError::Storage)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(AliasError::Storage);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_directory(_directory: &Path) -> Result<(), AliasError> {
    Err(AliasError::Storage)
}

#[cfg(unix)]
fn validate_reset_file(path: &Path) -> Result<(), AliasError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(AliasError::Storage),
    };
    let directory =
        fs::metadata(path.parent().ok_or(AliasError::Storage)?).map_err(|_| AliasError::Storage)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != directory.uid()
        || metadata.mode() & 0o077 != 0
        || metadata.nlink() != 1
        || metadata.len() > MAX_SIZE
    {
        return Err(AliasError::Storage);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_reset_file(_path: &Path) -> Result<(), AliasError> {
    Err(AliasError::Storage)
}

fn open(path: &Path, local_id: DeviceId, created: bool) -> Result<Connection, AliasError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| AliasError::Storage)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_SIZE {
        return Err(AliasError::Storage);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let parent = fs::metadata(path.parent().ok_or(AliasError::Storage)?)
            .map_err(|_| AliasError::Storage)?;
        if metadata.mode() & 0o077 != 0 || metadata.uid() != parent.uid() || metadata.nlink() != 1 {
            return Err(AliasError::Storage);
        }
    }
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| AliasError::Storage)?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(|_| AliasError::Storage)?;
    connection
        .execute_batch(
            "PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;",
        )
        .map_err(|_| AliasError::Storage)?;
    if created {
        connection
            .execute_batch("PRAGMA journal_mode=DELETE; PRAGMA max_page_count=256;")
            .map_err(|_| AliasError::Storage)?;
        let tx = connection.transaction().map_err(|_| AliasError::Storage)?;
        tx.execute_batch("PRAGMA application_id=1263553602; PRAGMA user_version=1;
            CREATE TABLE metadata (singleton INTEGER PRIMARY KEY CHECK(singleton=1),local_id BLOB NOT NULL CHECK(length(local_id)=32));
            CREATE TABLE labels (device_id BLOB PRIMARY KEY CHECK(length(device_id)=32),label TEXT NOT NULL CHECK(length(CAST(label AS BLOB)) BETWEEN 1 AND 256));").map_err(|_| AliasError::Storage)?;
        tx.execute(
            "INSERT INTO metadata VALUES(1,?1)",
            [local_id.as_bytes().as_slice()],
        )
        .map_err(|_| AliasError::Storage)?;
        tx.commit().map_err(|_| AliasError::Storage)?;
    }
    let app_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|_| AliasError::Storage)?;
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_| AliasError::Storage)?;
    let journal: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .map_err(|_| AliasError::Storage)?;
    let stored_id: Vec<u8> = connection
        .query_row(
            "SELECT local_id FROM metadata WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| AliasError::Storage)?;
    if app_id != APP_ID || version != 1 || journal != "delete" || stored_id != local_id.as_bytes() {
        return Err(AliasError::Storage);
    }
    validate_schema(&connection)?;
    Ok(connection)
}

fn validate_schema(connection: &Connection) -> Result<(), AliasError> {
    let quick_check: Vec<String> = connection
        .prepare("PRAGMA quick_check")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|_| AliasError::Storage)?;
    if quick_check.len() != 1 || quick_check[0] != "ok" {
        return Err(AliasError::Storage);
    }
    let objects: Vec<String> = connection
        .prepare(
            "SELECT name FROM sqlite_schema WHERE type IN ('table', 'index', 'view', 'trigger')
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|_| AliasError::Storage)?;
    if objects
        .iter()
        .map(String::as_str)
        .ne(["labels", "metadata"])
    {
        return Err(AliasError::Storage);
    }
    validate_columns(connection, "metadata", &["singleton", "local_id"])?;
    validate_columns(connection, "labels", &["device_id", "label"])?;
    Ok(())
}

fn validate_columns(
    connection: &Connection,
    table: &str,
    expected: &[&str],
) -> Result<(), AliasError> {
    let actual: Vec<String> = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get(1))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|_| AliasError::Storage)?;
    if actual
        .iter()
        .map(String::as_str)
        .ne(expected.iter().copied())
    {
        return Err(AliasError::Storage);
    }
    Ok(())
}
