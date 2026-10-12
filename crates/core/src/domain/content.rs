//! Typed plaintext payloads and their versioned storage envelope.

mod login;
mod note;
mod card;
mod custom;
mod identity;
mod license;
mod ssh_key;
mod totp;
mod wifi;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::SecretKind;
pub use crate::otp::{TotpAlgorithm, TotpConfig};
pub use login::{
    HistoricalPassword, LoginContent, LoginHistory, LoginHistoryEvent, LoginHistoryEventKind,
};
pub use note::NoteContent;
pub use card::CardContent;
pub use custom::{CustomContent, CustomRecord};
pub use identity::{IdentityContent, IdentityDocument};
pub use license::LicenseContent;
pub use ssh_key::SshKeyContent;
pub use totp::TotpContent;
pub use wifi::{WifiContent, WifiSecurity, wifi_qr_payload};
pub use crate::otp::TotpUriParameter;

/// Current encrypted content envelope version.
pub const CONTENT_SCHEMA_VERSION: u32 = 4;
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
const PREVIOUS_CONTENT_SCHEMA_VERSION: u32 = 3;

/// Supported payloads. Each variant owns its fields and validation rules.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub enum SecretContent {
    Login(LoginContent),
    Note(NoteContent),
    Custom(CustomContent),
    Card(CardContent),
    Totp(TotpContent),
    Identity(IdentityContent),
    Wifi(WifiContent),
    License(LicenseContent),
    SshKey(SshKeyContent),
}

impl SecretContent {
    pub const fn kind(&self) -> SecretKind {
        match self {
            Self::Login(_) => SecretKind::Login,
            Self::Note(_) => SecretKind::SecureNote,
            Self::Custom(_) => SecretKind::Custom,
            Self::Card(_) => SecretKind::Card,
            Self::Totp(_) => SecretKind::Totp,
            Self::Identity(_) => SecretKind::Identity,
            Self::Wifi(_) => SecretKind::Wifi,
            Self::License(_) => SecretKind::License,
            Self::SshKey(_) => SecretKind::SshKey,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Login(login) => login::validate_login(login),
            Self::Note(note) => note.validate(),
            Self::Custom(content) => content.validate(),
            Self::Card(content) => content.validate(),
            Self::Totp(content) => content.validate(),
            Self::Identity(content) => content.validate(),
            Self::Wifi(content) => content.validate(),
            Self::License(content) => content.validate(),
            Self::SshKey(content) => content.validate(),
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
            (Self::Note(_), Some(Self::Note(_)) | None)
            | (Self::Custom(_), Some(Self::Custom(_)) | None)
            | (Self::Card(_), Some(Self::Card(_)) | None)
            | (Self::Totp(_), Some(Self::Totp(_)) | None)
            | (Self::Identity(_), Some(Self::Identity(_)) | None)
            | (Self::Wifi(_), Some(Self::Wifi(_)) | None)
            | (Self::License(_), Some(Self::License(_)) | None)
            | (Self::SshKey(_), Some(Self::SshKey(_)) | None) => Ok(()),
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
    #[serde(rename = "custom")]
    Custom {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: &'a CustomContent,
    },
    #[serde(rename = "card")]
    Card {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: &'a CardContent,
    },
    #[serde(rename = "totp")]
    Totp {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: &'a TotpContent,
    },
    #[serde(rename = "identity")]
    Identity {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: &'a IdentityContent,
    },
    #[serde(rename = "wifi")]
    Wifi {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: &'a WifiContent,
    },
    #[serde(rename = "license")]
    License {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: &'a LicenseContent,
    },
    #[serde(rename = "ssh-key")]
    SshKey {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: &'a SshKeyContent,
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
    #[serde(rename = "custom")]
    Custom {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: Option<CustomContent>,
    },
    #[serde(rename = "card")]
    Card {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: Option<CardContent>,
    },
    #[serde(rename = "totp")]
    Totp {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: Option<TotpContent>,
    },
    #[serde(rename = "identity")]
    Identity {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: Option<IdentityContent>,
    },
    #[serde(rename = "wifi")]
    Wifi {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: Option<WifiContent>,
    },
    #[serde(rename = "license")]
    License {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: Option<LicenseContent>,
    },
    #[serde(rename = "ssh-key")]
    SshKey {
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
        fields: Option<SshKeyContent>,
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
        SecretContent::Custom(content) => EnvelopeRef::Custom {
            schema_version: CONTENT_SCHEMA_VERSION,
            fields: content,
        },
        SecretContent::Card(content) => EnvelopeRef::Card {
            schema_version: CONTENT_SCHEMA_VERSION,
            fields: content,
        },
        SecretContent::Totp(content) => EnvelopeRef::Totp {
            schema_version: CONTENT_SCHEMA_VERSION,
            fields: content,
        },
        SecretContent::Identity(content) => EnvelopeRef::Identity {
            schema_version: CONTENT_SCHEMA_VERSION,
            fields: content,
        },
        SecretContent::Wifi(content) => EnvelopeRef::Wifi {
            schema_version: CONTENT_SCHEMA_VERSION,
            fields: content,
        },
        SecretContent::License(content) => EnvelopeRef::License {
            schema_version: CONTENT_SCHEMA_VERSION,
            fields: content,
        },
        SecretContent::SshKey(content) => EnvelopeRef::SshKey {
            schema_version: CONTENT_SCHEMA_VERSION,
            fields: content,
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
        StoredEnvelope::Custom { schema_version, fields } => {
            validate_new_schema_version(*schema_version)?;
            SecretContent::Custom(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::Card { schema_version, fields } => {
            validate_new_schema_version(*schema_version)?;
            SecretContent::Card(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::Totp { schema_version, fields } => {
            validate_new_schema_version(*schema_version)?;
            SecretContent::Totp(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::Identity { schema_version, fields } => {
            validate_new_schema_version(*schema_version)?;
            SecretContent::Identity(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::Wifi { schema_version, fields } => {
            validate_new_schema_version(*schema_version)?;
            SecretContent::Wifi(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::License { schema_version, fields } => {
            validate_new_schema_version(*schema_version)?;
            SecretContent::License(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::SshKey { schema_version, fields } => {
            validate_new_schema_version(*schema_version)?;
            SecretContent::SshKey(fields.take().context("secret content is missing fields")?)
        }
    };
    content.validate()?;
    Ok(content)
}

fn validate_schema_version(version: u32) -> Result<()> {
    ensure!(
        matches!(
            version,
            LEGACY_CONTENT_SCHEMA_VERSION
                | LOGIN_HISTORY_SCHEMA_VERSION
                | PREVIOUS_CONTENT_SCHEMA_VERSION
                | CONTENT_SCHEMA_VERSION
        ),
        "unsupported secret content schema version"
    );
    Ok(())
}

fn validate_new_schema_version(version: u32) -> Result<()> {
    ensure!(
        version == CONTENT_SCHEMA_VERSION,
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
