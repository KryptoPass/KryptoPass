//! Bounded row decoding and queries for folders, secrets, and list metadata.

use super::icons::parse_entry_icon;
use super::recipients::{load_folder_recipients, load_secret_recipients};
use crate::infrastructure::MAX_CIPHERTEXT_BYTES;
use anyhow::{Context, Result, bail, ensure};
use kryptopass_core::{
    DomainError, Folder, FolderAppearance, FolderId, Secret, SecretId, SecretKind, SecretMetadata,
};
use rusqlite::types::{Type, ValueRef};
use rusqlite::{Connection, OptionalExtension, Row};
use std::io;
use uuid::Uuid;

pub(super) fn read_folders(connection: &Connection) -> Result<Vec<Folder>> {
    let mut statement = connection
        .prepare(
            "SELECT id, parent_id, name, icon, color, sort_position \
             FROM folders ORDER BY sort_position, name COLLATE NOCASE, id",
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
                sort_position: row.get(5)?,
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
            sort_position: raw.sort_position,
            name: raw.name,
            appearance: parse_folder_appearance(&raw.icon, &raw.color)?,
            default_recipients,
        });
    }

    Ok(folders)
}

pub(super) fn read_secrets(connection: &Connection) -> Result<Vec<Secret>> {
    let mut statement = connection
        .prepare(
            "SELECT id, folder_id, title, kind, favorite, trashed, revision, \
                    length(ciphertext), ciphertext, icon_theme, icon_asset_id, icon_favicon_host \
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
                icon_theme: row.get(9)?,
                icon_asset_id: row.get(10)?,
                icon_favicon_host: row.get(11)?,
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
            icon: parse_entry_icon(raw.icon_theme, raw.icon_asset_id, raw.icon_favicon_host)?,
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

pub(super) fn read_secret_metadata(connection: &Connection) -> Result<Vec<SecretMetadata>> {
    let mut statement = connection
        .prepare(
            "SELECT id, folder_id, title, kind, favorite, trashed, revision, icon_theme, icon_asset_id, icon_favicon_host \
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
                icon_theme: row.get(7)?,
                icon_asset_id: row.get(8)?,
                icon_favicon_host: row.get(9)?,
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
            icon: parse_entry_icon(raw.icon_theme, raw.icon_asset_id, raw.icon_favicon_host)?,
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

pub(super) fn column_data_error(
    index: usize,
    column_type: Type,
    message: &'static str,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        column_type,
        Box::new(io::Error::new(io::ErrorKind::InvalidData, message)),
    )
}

pub(super) fn load_folder(connection: &Connection, id: FolderId) -> Result<Option<Folder>> {
    let raw = connection
        .query_row(
            "SELECT id, parent_id, name, icon, color, sort_position FROM folders WHERE id = ?1",
            [id.0.to_string()],
            |row| {
                Ok(RawFolder {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    name: row.get(2)?,
                    icon: row.get(3)?,
                    color: row.get(4)?,
                    sort_position: row.get(5)?,
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
            sort_position: raw.sort_position,
            name: raw.name,
            appearance: parse_folder_appearance(&raw.icon, &raw.color)?,
            default_recipients,
        })
    })
    .transpose()
}

pub(super) fn load_secret(connection: &Connection, id: SecretId) -> Result<Option<Secret>> {
    let raw = connection
        .query_row(
            "SELECT id, folder_id, title, kind, favorite, trashed, revision, \
                    length(ciphertext), ciphertext, icon_theme, icon_asset_id, icon_favicon_host \
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
                    icon_theme: row.get(9)?,
                    icon_asset_id: row.get(10)?,
                    icon_favicon_host: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(|_| anyhow::anyhow!("could not read secret"))?;
    raw.map(|raw| {
        let id = parse_secret_id(&raw.id)?;
        let folder_id = raw.folder_id.as_deref().map(parse_folder_id).transpose()?;
        Ok(Secret {
            icon: parse_entry_icon(raw.icon_theme, raw.icon_asset_id, raw.icon_favicon_host)?,
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

fn parse_folder_appearance(icon: &str, color: &str) -> Result<FolderAppearance> {
    FolderAppearance::new(icon.to_owned(), color.to_owned())
        .map_err(|_| anyhow::anyhow!("stored folder appearance is invalid"))
}

fn parse_kind(value: &str) -> Result<SecretKind> {
    SecretKind::parse(value).map_err(|error| match error {
        DomainError::UnsupportedSecretKind => {
            anyhow::anyhow!("stored secret kind is unsupported")
        }
        _ => anyhow::anyhow!("stored secret kind is invalid"),
    })
}

pub(super) fn parse_bool(value: i64) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => bail!("stored boolean metadata is invalid"),
    }
}

fn parse_revision(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| anyhow::anyhow!("stored secret revision is invalid"))
}

pub(super) fn parse_folder_id(value: &str) -> Result<FolderId> {
    parse_uuid(value).map(FolderId)
}

pub(super) fn parse_secret_id(value: &str) -> Result<SecretId> {
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

#[derive(Debug)]
struct RawFolder {
    id: String,
    parent_id: Option<String>,
    name: String,
    icon: String,
    color: String,
    sort_position: i64,
}

#[derive(Debug)]
struct RawSecret {
    icon_theme: Option<String>,
    icon_asset_id: Option<String>,
    icon_favicon_host: Option<String>,
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
    icon_theme: Option<String>,
    icon_asset_id: Option<String>,
    icon_favicon_host: Option<String>,
    id: String,
    folder_id: Option<String>,
    title: String,
    kind: String,
    favorite: i64,
    trashed: i64,
    revision: i64,
}
