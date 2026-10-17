//! Opening and configuring private, durable SQLite connections.

use super::SqliteStorage;
use super::filesystem::{
    ensure_database_file_is_safe, ensure_database_parent, ensure_existing_database_parent,
    set_private_database_permissions,
};
use super::schema::{initialize_schema, prevalidate_existing_database};
use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::time::Duration;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

impl SqliteStorage {
    /// Opens the encrypted store, creating its schema if this is a new file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_internal(path.as_ref(), true)
    }

    /// Opens an existing encrypted store without creating the file or parent.
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_internal(path.as_ref(), false)
    }

    fn open_internal(path: &Path, create: bool) -> Result<Self> {
        if create {
            ensure_database_parent(path)?;
        } else {
            ensure_existing_database_parent(path)?;
        }
        ensure_database_file_is_safe(path)?;
        if !create {
            prevalidate_existing_database(path)?;
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let flags = if create {
            flags | OpenFlags::SQLITE_OPEN_CREATE
        } else {
            flags
        };
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
}
