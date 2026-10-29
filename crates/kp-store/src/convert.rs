//! Converting between rows and port types, and SQLite errors into `StoreError`.
//!
//! Stored data is not trusted: every id, flag, icon and fingerprint is parsed
//! and validated on the way out, and a bad row is a backend error, never a
//! panic. Errors name what was wrong, never a stored value.

use kp_template::TemplateId;
use kryptopass_core::vault::{Author, StoreError};
use kryptopass_core::vault::{Hlc, VersionId};
use kryptopass_core::{EntryIcon, FolderId, KeyFingerprint, RecipientSet, SecretId};
use rusqlite::ErrorCode;
use uuid::Uuid;

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

pub(crate) fn clock(millis: i64, counter: i64) -> StoreResult<Hlc> {
    Ok(Hlc {
        millis: u64::try_from(millis).map_err(|_| corrupt("clock"))?,
        counter: u32::try_from(counter).map_err(|_| corrupt("clock"))?,
    })
}

/// Clock columns as SQLite stores them.
pub(crate) fn clock_params(clock: Hlc) -> StoreResult<(i64, i64)> {
    let millis =
        i64::try_from(clock.millis).map_err(|_| StoreError::Rejected("clock out of range"))?;
    Ok((millis, i64::from(clock.counter)))
}

pub(crate) fn author(raw: &[u8]) -> StoreResult<Author> {
    raw.try_into()
        .map(Author)
        .map_err(|_| corrupt("version author"))
}

/// A record or version id as the 16 bytes Sync signs.
pub(crate) fn id_bytes(id: Uuid) -> [u8; 16] {
    *id.as_bytes()
}

/// An id received from Sync, which must not be nil.
pub(crate) fn id_from_bytes(bytes: [u8; 16]) -> StoreResult<Uuid> {
    let id = Uuid::from_bytes(bytes);
    if id.is_nil() {
        return Err(StoreError::Rejected("nil identifier"));
    }
    Ok(id)
}

pub(crate) fn uuid_text(raw: &str) -> StoreResult<Uuid> {
    uuid(raw)
}

/// A fingerprint as Sync carries it: raw bytes.
pub(crate) fn fingerprint_bytes(fingerprint: &str) -> StoreResult<Vec<u8>> {
    if fingerprint.len() % 2 != 0 {
        return Err(corrupt("recipient fingerprint"));
    }
    (0..fingerprint.len())
        .step_by(2)
        .map(|at| {
            u8::from_str_radix(&fingerprint[at..at + 2], 16)
                .map_err(|_| corrupt("recipient fingerprint"))
        })
        .collect()
}

/// A fingerprint received from Sync, as this store writes it: upper-case hex.
pub(crate) fn fingerprint_text(bytes: &[u8]) -> StoreResult<String> {
    let text: String = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
    KeyFingerprint::parse(&text)
        .map(|fingerprint| fingerprint.as_str().to_owned())
        .map_err(|_| StoreError::Rejected("invalid recipient fingerprint"))
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
