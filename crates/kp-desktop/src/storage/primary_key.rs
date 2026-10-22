//! The vault's primary key: the default recipient the user chose for this vault.

use anyhow::{Context, Result};
use kryptopass_core::KeyFingerprint;
use rusqlite::{OptionalExtension, params};

use super::SqliteStorage;

/// The primary key and whether locking the vault makes gpg-agent forget its passphrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryKey {
    pub fingerprint: KeyFingerprint,
    pub forget_on_lock: bool,
}

impl SqliteStorage {
    /// Returns the primary key recorded in this vault, if one was chosen.
    pub fn primary_key(&self) -> Result<Option<PrimaryKey>> {
        let row: Option<(String, bool)> = self
            .connection
            .query_row(
                "SELECT fingerprint, forget_on_lock FROM vault_primary_key WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .context("could not read the vault primary key")?;
        row.map(|(fingerprint, forget_on_lock)| {
            Ok(PrimaryKey {
                fingerprint: KeyFingerprint::parse(&fingerprint)
                    .context("vault primary key is invalid")?,
                forget_on_lock,
            })
        })
        .transpose()
    }

    /// Records the primary key; it only changes defaults, never existing encryption.
    pub fn set_primary_key(&mut self, key: &PrimaryKey) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO vault_primary_key(singleton, fingerprint, forget_on_lock)
                 VALUES (1, ?1, ?2)
                 ON CONFLICT(singleton) DO UPDATE SET
                     fingerprint = excluded.fingerprint,
                     forget_on_lock = excluded.forget_on_lock",
                params![key.fingerprint.as_str(), key.forget_on_lock],
            )
            .context("could not save the vault primary key")?;
        Ok(())
    }
}
