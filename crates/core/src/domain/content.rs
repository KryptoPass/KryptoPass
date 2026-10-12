//! Typed plaintext payloads and their versioned storage envelope.

mod login;
mod note;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::SecretKind;
pub use crate::otp::{TotpAlgorithm, TotpConfig};
pub use login::{
    HistoricalPassword, LoginContent, LoginHistory, LoginHistoryEvent, LoginHistoryEventKind,
};
pub use note::NoteContent;

/// Current encrypted content envelope version.
pub const CONTENT_SCHEMA_VERSION: u32 = 3;
/// Maximum serialized or decoded content size in bytes.
pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;
/// Maximum retained login-history event count.
pub const MAX_LOGIN_HISTORY_EVENTS: usize = 100;
/// Maximum retained previous-password count.
pub const MAX_PREVIOUS_PASSWORDS: usize = 20;
/// Maximum combined byte size of retained previous passwords and snapshots.
pub const MAX_HISTORY_PASSWORD_BYTES: usize = 512 * 1024;

const LEGACY_CONTENT_SCHEMA_VERSION: u32 = 1;
const LOGIN_HISTORY_SCHEMA_VERSION: u32 = 2;

/// Supported payloads. Each variant owns its fields and validation rules.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub enum SecretContent {
    Login(LoginContent),
    Note(NoteContent),
}

impl SecretContent {
    pub const fn kind(&self) -> SecretKind {
        match self {
            Self::Login(_) => SecretKind::Login,
            Self::Note(_) => SecretKind::SecureNote,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Login(login) => login::validate_login(login),
            Self::Note(note) => note.validate(),
        }
    }

    pub(crate) const fn needs_previous_content(&self) -> bool {
        match self {
            Self::Login(_) => true,
            Self::Note(_) => false,
        }
    }

    pub(crate) fn record_save(
        &mut self,
        previous: Option<&mut Self>,
        previous_title: &str,
        metadata_changed: bool,
        timestamp: u64,
    ) -> Result<()> {
        match (self, previous) {
            (Self::Login(login), Some(Self::Login(previous))) => {
                login.record_save(Some(previous), previous_title, metadata_changed, timestamp)
            }
            (Self::Login(login), None) => {
                login.record_save(None, previous_title, metadata_changed, timestamp)
            }
            (Self::Note(_), Some(Self::Note(_)) | None) => Ok(()),
            _ => anyhow::bail!("an existing secret cannot change its content kind"),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum EnvelopeRef<'a> {
    #[serde(rename = "login")]
    Login {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: login::FieldsRef<'a>,
    },
    #[serde(rename = "note")]
    Note {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: &'a NoteContent,
    },
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(tag = "kind", deny_unknown_fields)]
enum StoredEnvelope {
    #[serde(rename = "login")]
    Login {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: Box<login::StoredFields>,
    },
    #[serde(rename = "note")]
    Note {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: NoteContent,
    },
}

/// Encodes validated content. The plaintext buffer is zeroized on drop.
pub fn encode_content(content: &SecretContent) -> Result<Zeroizing<Vec<u8>>> {
    content.validate()?;
    let envelope = match content {
        SecretContent::Login(login) => EnvelopeRef::Login {
            schema_version: CONTENT_SCHEMA_VERSION,
            fields: login.into(),
        },
        SecretContent::Note(note) => EnvelopeRef::Note {
            schema_version: CONTENT_SCHEMA_VERSION,
            fields: note,
        },
    };
    let mut bytes = Zeroizing::new(Vec::new());
    serde_json::to_writer(&mut *bytes, &envelope).context("could not encode secret content")?;
    ensure!(
        bytes.len() <= MAX_CONTENT_BYTES,
        "secret content exceeds the maximum size"
    );
    Ok(bytes)
}

/// Reads supported versions without accepting unknown or cross-variant fields.
pub fn decode_content(bytes: &[u8]) -> Result<SecretContent> {
    ensure!(
        bytes.len() <= MAX_CONTENT_BYTES,
        "secret content exceeds the maximum size"
    );
    let mut envelope: StoredEnvelope =
        serde_json::from_slice(bytes).context("could not decode secret content envelope")?;
    let content = match &mut envelope {
        StoredEnvelope::Login {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::Login(fields.take_content(*schema_version)?)
        }
        StoredEnvelope::Note {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::Note(NoteContent {
                value: std::mem::take(&mut fields.value),
                notes: std::mem::take(&mut fields.notes),
            })
        }
    };
    content.validate()?;
    Ok(content)
}

fn validate_schema_version(version: u32) -> Result<()> {
    ensure!(
        matches!(
            version,
            LEGACY_CONTENT_SCHEMA_VERSION | LOGIN_HISTORY_SCHEMA_VERSION | CONTENT_SCHEMA_VERSION
        ),
        "unsupported secret content schema version"
    );
    Ok(())
}

fn ensure_field_size(field: &str, value: &str, maximum: usize) -> Result<()> {
    ensure!(
        value.len() <= maximum,
        "secret content {field} exceeds the maximum size"
    );
    Ok(())
}
