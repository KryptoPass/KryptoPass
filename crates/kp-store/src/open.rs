//! Opening a private, durable SQLite connection.
//!
//! Same safeguards as the version 1 store: no symlinks, owner-only files,
//! foreign keys on, full sync, untrusted schema and secure delete, so removed
//! ciphertext is overwritten instead of left in free pages.

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::time::Duration;

use crate::SqliteStore;
use crate::filesystem::{
    ensure_database_file_is_safe, ensure_database_parent, ensure_existing_database_parent,
    set_private_database_permissions,
};
use crate::schema;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

impl SqliteStore {
    /// Opens the store, creating the file and its schema if it is new.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_internal(path.as_ref(), true)
    }

    /// Opens an existing store without creating the file or its directory.
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
            schema::prevalidate(path)?;
        }

        let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        if create {
            flags |= OpenFlags::SQLITE_OPEN_CREATE;
        }
        let mut connection = Connection::open_with_flags(path, flags)
            .map_err(|_| anyhow::anyhow!("could not open the store"))?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .context("could not configure the store")?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA synchronous = FULL;
                 PRAGMA trusted_schema = OFF;
                 PRAGMA secure_delete = ON;",
            )
            .map_err(|_| anyhow::anyhow!("could not configure the store"))?;
        set_private_database_permissions(path)?;
        schema::initialize(&mut connection)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|_| anyhow::anyhow!("could not enable durable store journaling"))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|_| anyhow::anyhow!("could not configure durable store journaling"))?;
        tracing::debug!(schema = schema::SCHEMA_VERSION, "store opened");
        Ok(Self { connection })
    }
}
