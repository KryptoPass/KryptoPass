//! The vault's primary key: the default recipient the user chose for this vault.

use anyhow::{Context, Result};
use kryptopass_core::KeyFingerprint;
use rusqlite::{OptionalExtension, params};

use super::SqliteStorage;

impl SqliteStorage {
    /// Returns the primary key recorded in this vault, if one was chosen.
    pub fn primary_key(&self) -> Result<Option<KeyFingerprint>> {
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT fingerprint FROM vault_primary_key WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .context("could not read the vault primary key")?;
        value
            .map(|value| KeyFingerprint::parse(&value).context("vault primary key is invalid"))
            .transpose()
    }

    /// Records the primary key; it only changes defaults, never existing encryption.
    pub fn set_primary_key(&mut self, fingerprint: &KeyFingerprint) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO vault_primary_key(singleton, fingerprint) VALUES (1, ?1)
                 ON CONFLICT(singleton) DO UPDATE SET fingerprint = excluded.fingerprint",
                params![fingerprint.as_str()],
            )
            .context("could not save the vault primary key")?;
        Ok(())
    }
}
