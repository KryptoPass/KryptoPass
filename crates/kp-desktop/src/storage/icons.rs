//! Persistence, bounded reads, and garbage collection for public entry icon assets.

use super::records::column_data_error;
use crate::backup::MAX_BACKUP_PLAINTEXT_BYTES;
use anyhow::{Context, Result, bail, ensure};
use kryptopass_core::{EntryIcon, EntryIconAsset, MAX_ENTRY_ICON_BYTES, SecretId};
use rusqlite::types::Type;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

pub(super) fn parse_entry_icon(
    theme: Option<String>,
    asset: Option<String>,
    host: Option<String>,
) -> Result<Option<EntryIcon>> {
    let icon = match (theme, asset, host) {
        (None, None, None) => None,
        (None, None, Some(host)) => Some(EntryIcon::Favicon { host }),
        (Some(name), None, None) => Some(EntryIcon::Theme { name }),
        (None, Some(id), None) => Some(EntryIcon::Image {
            asset_id: Uuid::parse_str(&id)?,
        }),
        _ => bail!("entry contains conflicting icon sources"),
    };
    if let Some(icon) = &icon {
        icon.validate()?;
    }
    Ok(icon)
}

pub(super) fn insert_icon_asset(connection: &Connection, asset: &EntryIconAsset) -> Result<()> {
    asset.validate()?;
    connection
        .execute(
            "INSERT INTO entry_icon_assets(id, png) VALUES (?1, ?2)",
            params![asset.id.to_string(), &asset.png],
        )
        .context("could not save icon resource")?;
    Ok(())
}

pub(super) fn set_secret_icon(
    connection: &Connection,
    id: SecretId,
    icon: Option<&EntryIcon>,
) -> Result<()> {
    let (theme, asset, host) = match icon {
        Some(EntryIcon::Favicon { host }) => (None, None, Some(host.clone())),
        Some(EntryIcon::Theme { name }) => (Some(name.clone()), None, None),
        Some(EntryIcon::Image { asset_id }) => (None, Some(asset_id.to_string()), None),
        None => (None, None, None),
    };
    connection
        .execute(
            "UPDATE secrets SET icon_theme = ?1, icon_asset_id = ?2, icon_favicon_host = ?3 WHERE id = ?4",
            params![theme, asset, host, id.0.to_string()],
        )
        .context("could not save entry icon")?;
    Ok(())
}

pub(super) fn collect_unused_icons(connection: &Connection) -> Result<()> {
    connection
        .execute(
            "DELETE FROM entry_icon_assets WHERE id NOT IN
        (SELECT icon_asset_id FROM secrets WHERE icon_asset_id IS NOT NULL)",
            [],
        )
        .context("could not collect unused icons")?;
    Ok(())
}

pub(super) fn read_icon_assets(connection: &Connection) -> Result<Vec<EntryIconAsset>> {
    let total: i64 = connection.query_row(
        "SELECT COALESCE(SUM(length(png)), 0) FROM entry_icon_assets",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        total >= 0 && total as u64 <= MAX_BACKUP_PLAINTEXT_BYTES as u64 / 2,
        "icon resources exceed backup budget"
    );
    let mut statement =
        connection.prepare("SELECT id, length(png), png FROM entry_icon_assets ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        let size: i64 = row.get(1)?;
        if !(45..=MAX_ENTRY_ICON_BYTES as i64).contains(&size) {
            return Err(column_data_error(
                1,
                Type::Integer,
                "invalid icon resource size",
            ));
        }
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(2)?))
    })?;
    rows.map(|row| {
        let (id, png) = row?;
        let asset = EntryIconAsset {
            id: Uuid::parse_str(&id)?,
            png,
        };
        asset.validate()?;
        Ok(asset)
    })
    .collect()
}

/// Loads a bounded, validated image through a read-only store connection.
pub(crate) fn read_icon_asset(path: &Path, id: Uuid) -> Result<Vec<u8>> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    connection.pragma_update(None, "trusted_schema", false)?;
    let length: Option<i64> = connection
        .query_row(
            "SELECT length(png) FROM entry_icon_assets WHERE id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    ensure!(
        length.is_some_and(|length| length >= 45 && length <= MAX_ENTRY_ICON_BYTES as i64),
        "icon missing or too large"
    );
    let png: Vec<u8> = connection.query_row(
        "SELECT png FROM entry_icon_assets WHERE id = ?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    let asset = EntryIconAsset { id, png };
    asset.validate()?;
    Ok(asset.png)
}

impl super::SqliteStorage {
    /// SHA-256 of every stored icon image that passes strict validation,
    /// computed on the fly; an invalid row is logged and left out.
    pub(crate) fn icon_asset_hashes(&self) -> Result<HashMap<Uuid, [u8; 32]>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, png FROM entry_icon_assets WHERE length(png) BETWEEN 45 AND ?1")?;
        let rows = statement.query_map([MAX_ENTRY_ICON_BYTES as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut hashes = HashMap::new();
        for row in rows {
            let (id, png) = row?;
            let Ok(id) = Uuid::parse_str(&id) else {
                tracing::warn!(reason = "asset_id", "Icon asset skipped while hashing");
                continue;
            };
            let asset = EntryIconAsset { id, png };
            if let Err(error) = asset.validate() {
                tracing::warn!(reason = %error, "Icon asset failed strict validation while hashing");
                continue;
            }
            hashes.insert(id, asset.sha256());
        }
        Ok(hashes)
    }

    /// One stored icon image, validated; None when the row is absent.
    pub(crate) fn icon_asset(&self, id: Uuid) -> Result<Option<EntryIconAsset>> {
        let png: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT png FROM entry_icon_assets WHERE id = ?1 AND length(png) BETWEEN 45 AND ?2",
                params![id.to_string(), MAX_ENTRY_ICON_BYTES as i64],
                |row| row.get(0),
            )
            .optional()?;
        png.map(|png| {
            let asset = EntryIconAsset { id, png };
            asset.validate()?;
            Ok(asset)
        })
        .transpose()
    }
}
