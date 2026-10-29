//! Converting between rows and port types, and SQLite errors into `StoreError`.
//!
//! Stored data is not trusted: every id, flag, icon and fingerprint is parsed
//! and validated on the way out, and a bad row is a backend error, never a
//! panic. Errors name what was wrong, never a stored value.

use kp_template::TemplateId;
use kryptopass_core::vault::VersionId;
use kryptopass_core::vault::{Author, StoreError};
use kryptopass_core::{EntryIcon, FolderId, KeyFingerprint, RecipientSet, SecretId};
use rusqlite::ErrorCode;
use uuid::Uuid;

/// Longest accepted Sync device id of a version author.
const MAX_AUTHOR_BYTES: usize = 128;

pub(crate) type StoreResult<T> = Result<T, StoreError>;

/// Stored data that fails validation.
pub(crate) fn corrupt(what: &'static str) -> StoreError {
    StoreError::Backend(anyhow::anyhow!("stored {what} is invalid"))
}

/// A constraint violation is a refused change; anything else is a backend fault.
pub(crate) fn sql(error: rusqlite::Error) -> StoreError {
    match &error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == ErrorCode::ConstraintViolation =>
        {
            StoreError::Rejected("constraint violated")
        }
        _ => StoreError::Backend(error.into()),
    }
}

pub(crate) trait SqlResult<T> {
    fn sql(self) -> StoreResult<T>;
}

impl<T> SqlResult<T> for rusqlite::Result<T> {
    fn sql(self) -> StoreResult<T> {
        self.map_err(sql)
    }
}

pub(crate) fn text(id: Uuid) -> String {
    id.hyphenated().to_string()
}

/// A UUID written in its canonical form, the only form this store writes.
fn uuid(raw: &str) -> StoreResult<Uuid> {
    match Uuid::try_parse(raw) {
        Ok(id) if !id.is_nil() && raw == text(id) => Ok(id),
        _ => Err(corrupt("identifier")),
    }
}

pub(crate) fn item_id(raw: &str) -> StoreResult<SecretId> {
    uuid(raw).map(SecretId)
}

pub(crate) fn folder_id(raw: &str) -> StoreResult<FolderId> {
    uuid(raw).map(FolderId)
}

pub(crate) fn version_id(raw: &str) -> StoreResult<VersionId> {
    uuid(raw).map(VersionId)
}

pub(crate) fn template(raw: &str) -> StoreResult<TemplateId> {
    TemplateId::parse(raw).map_err(|_| corrupt("template id"))
}

pub(crate) fn flag(value: i64) -> StoreResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt("flag")),
    }
}

pub(crate) fn revision(value: i64) -> StoreResult<u64> {
    u64::try_from(value).map_err(|_| corrupt("revision"))
}

/// A revision as SQLite stores it; above `i64::MAX` it is exhausted.
pub(crate) fn revision_param(value: u64) -> StoreResult<i64> {
    i64::try_from(value).map_err(|_| StoreError::Rejected("revision exhausted"))
}

pub(crate) fn timestamp(value: i64) -> StoreResult<u64> {
    u64::try_from(value).map_err(|_| corrupt("timestamp"))
}

pub(crate) fn timestamp_param(value: u64) -> StoreResult<i64> {
    i64::try_from(value).map_err(|_| StoreError::Rejected("timestamp out of range"))
}

pub(crate) fn author(raw: Option<String>) -> StoreResult<Author> {
    match raw {
        None => Ok(Author::Local),
        Some(device) if valid_device(&device) => Ok(Author::Device(device)),
        Some(_) => Err(corrupt("version author")),
    }
}

pub(crate) fn author_param(author: &Author) -> StoreResult<Option<&str>> {
    match author {
        Author::Local => Ok(None),
        Author::Device(device) if valid_device(device) => Ok(Some(device)),
        Author::Device(_) => Err(StoreError::Rejected("invalid version author")),
    }
}

/// Printable ASCII, so an author id can be logged and shown safely.
fn valid_device(device: &str) -> bool {
    !device.is_empty()
        && device.len() <= MAX_AUTHOR_BYTES
        && device.bytes().all(|byte| byte.is_ascii_graphic())
}

/// The three icon columns: theme name, image asset id, favicon host.
pub(crate) type IconColumns = (Option<String>, Option<String>, Option<String>);

pub(crate) fn icon(columns: IconColumns) -> StoreResult<Option<EntryIcon>> {
    let icon = match columns {
        (None, None, None) => return Ok(None),
        (Some(name), None, None) => EntryIcon::Theme { name },
        (None, Some(asset), None) => EntryIcon::Image {
            asset_id: uuid(&asset)?,
        },
        (None, None, Some(host)) => EntryIcon::Favicon { host },
        _ => return Err(corrupt("icon")),
    };
    icon.validate().map_err(|_| corrupt("icon"))?;
    Ok(Some(icon))
}

pub(crate) fn icon_columns(icon: Option<&EntryIcon>) -> IconColumns {
    match icon {
        None => (None, None, None),
        Some(EntryIcon::Theme { name }) => (Some(name.clone()), None, None),
        Some(EntryIcon::Image { asset_id }) => (None, Some(text(*asset_id)), None),
        Some(EntryIcon::Favicon { host }) => (None, None, Some(host.clone())),
    }
}

pub(crate) fn recipients(fingerprints: Vec<String>) -> StoreResult<RecipientSet> {
    let fingerprints = fingerprints
        .iter()
        .map(|raw| KeyFingerprint::parse(raw).map_err(|_| corrupt("recipient fingerprint")))
        .collect::<StoreResult<Vec<_>>>()?;
    RecipientSet::new(fingerprints).map_err(|_| corrupt("recipient set"))
}
