//! Vault settings: default recipients and per-key settings.

use kryptopass_core::RecipientSet;
use kryptopass_core::vault::KeySettings;
use rusqlite::{Connection, params};

use crate::convert::{SqlResult, StoreResult};

pub(super) fn set_default_recipients(
    connection: &Connection,
    recipients: Option<&RecipientSet>,
) -> StoreResult<()> {
    connection
        .execute("DELETE FROM default_recipients", [])
        .sql()?;
    for fingerprint in recipients.map_or(&[][..], RecipientSet::as_slice) {
        connection
            .execute(
                "INSERT INTO default_recipients(fingerprint) VALUES (?1)",
                [fingerprint.as_str()],
            )
            .sql()?;
    }
    Ok(())
}

pub(super) fn set_key_settings(connection: &Connection, key: &KeySettings) -> StoreResult<()> {
    if key.is_default() {
        connection
            .execute(
                "DELETE FROM key_settings WHERE fingerprint = ?1",
                [key.fingerprint.as_str()],
            )
            .sql()?;
    } else {
        connection
            .execute(
                "INSERT INTO key_settings(fingerprint, forget_on_lock) VALUES (?1, ?2) \
                 ON CONFLICT(fingerprint) DO UPDATE SET forget_on_lock = excluded.forget_on_lock",
                params![key.fingerprint.as_str(), key.forget_on_lock],
            )
            .sql()?;
    }
    Ok(())
}
