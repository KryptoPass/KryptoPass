//! Typed plaintext payloads and their versioned storage envelope.

mod card;
mod custom;
mod identity;
mod license;
mod login;
mod note;
mod ssh_key;
mod totp;
mod wifi;

use crate::SensitiveBytes;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::SecretKind;
pub use crate::otp::TotpUriParameter;
pub use crate::otp::{TotpAlgorithm, TotpConfig};
pub use card::CardContent;
pub use custom::{CustomContent, CustomRecord};
pub use identity::{IdentityContent, IdentityDocument};
pub use license::LicenseContent;
pub use login::{LoginContent, LoginHistory, LoginHistoryEvent, LoginHistoryEventKind};
pub use note::NoteContent;
pub use ssh_key::SshKeyContent;
pub use totp::TotpContent;
pub use wifi::{WifiContent, WifiSecurity, wifi_qr_payload};

/// Current encrypted content envelope version.
pub const CONTENT_SCHEMA_VERSION: u32 = 1;
/// Maximum serialized or decoded content size in bytes.
pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;
/// Maximum retained login-history event count.
pub const MAX_LOGIN_HISTORY_EVENTS: usize = 100;
/// Maximum combined byte size of retained login snapshots.
pub const MAX_HISTORY_PASSWORD_BYTES: usize = 512 * 1024;

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
            Self::Note(_)
            | Self::Custom(_)
            | Self::Card(_)
            | Self::Totp(_)
            | Self::Identity(_)
            | Self::Wifi(_)
            | Self::License(_)
            | Self::SshKey(_) => false,
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

#[derive(Zeroize, ZeroizeOnDrop)]
enum StoredEnvelope {
    Login {
        schema_version: u32,
        fields: Box<login::StoredFields>,
    },
    Note {
        schema_version: u32,
        fields: NoteContent,
    },
    Custom {
        schema_version: u32,
        fields: Option<CustomContent>,
    },
    Card {
        schema_version: u32,
        fields: Option<CardContent>,
    },
    Totp {
        schema_version: u32,
        fields: Option<TotpContent>,
    },
    Identity {
        schema_version: u32,
        fields: Option<IdentityContent>,
    },
    Wifi {
        schema_version: u32,
        fields: Option<WifiContent>,
    },
    License {
        schema_version: u32,
        fields: Option<LicenseContent>,
    },
    SshKey {
        schema_version: u32,
        fields: Option<SshKeyContent>,
    },
}

// Borrow the raw fields instead of serde's internally tagged content buffer,
// which would retain private strings in ordinary heap allocations.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BorrowedEnvelope<'a> {
    schema_version: u32,
    kind: StoredKind,
    #[serde(borrow)]
    fields: &'a serde_json::value::RawValue,
}

#[derive(Deserialize)]
enum StoredKind {
    #[serde(rename = "login")]
    Login,
    #[serde(rename = "note")]
    Note,
    #[serde(rename = "custom")]
    Custom,
    #[serde(rename = "card")]
    Card,
    #[serde(rename = "totp")]
    Totp,
    #[serde(rename = "identity")]
    Identity,
    #[serde(rename = "wifi")]
    Wifi,
    #[serde(rename = "license")]
    License,
    #[serde(rename = "ssh-key")]
    SshKey,
}

fn read_envelope(bytes: &[u8]) -> Result<StoredEnvelope> {
    let envelope: BorrowedEnvelope<'_> = serde_json::from_slice(bytes)?;
    let schema_version = envelope.schema_version;
    let raw = envelope.fields.get();
    Ok(match envelope.kind {
        StoredKind::Login => StoredEnvelope::Login {
            schema_version,
            fields: serde_json::from_str(raw)?,
        },
        StoredKind::Note => StoredEnvelope::Note {
            schema_version,
            fields: serde_json::from_str(raw)?,
        },
        StoredKind::Custom => StoredEnvelope::Custom {
            schema_version,
            fields: serde_json::from_str(raw)?,
        },
        StoredKind::Card => StoredEnvelope::Card {
            schema_version,
            fields: serde_json::from_str(raw)?,
        },
        StoredKind::Totp => StoredEnvelope::Totp {
            schema_version,
            fields: serde_json::from_str(raw)?,
        },
        StoredKind::Identity => StoredEnvelope::Identity {
            schema_version,
            fields: serde_json::from_str(raw)?,
        },
        StoredKind::Wifi => StoredEnvelope::Wifi {
            schema_version,
            fields: serde_json::from_str(raw)?,
        },
        StoredKind::License => StoredEnvelope::License {
            schema_version,
            fields: serde_json::from_str(raw)?,
        },
        StoredKind::SshKey => StoredEnvelope::SshKey {
            schema_version,
            fields: serde_json::from_str(raw)?,
        },
    })
}

/// Encodes validated content. The plaintext buffer is zeroized on drop.
pub fn encode_content(content: &SecretContent) -> Result<SensitiveBytes> {
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
    let mut bytes = SensitiveBytes::new();
    serde_json::to_writer(bytes.bounded_writer(MAX_CONTENT_BYTES), &envelope)
        .context("could not encode secret content")?;
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
        read_envelope(bytes).context("could not decode secret content envelope")?;
    let content = match &mut envelope {
        StoredEnvelope::Login {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::Login(fields.take_content())
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
        StoredEnvelope::Custom {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::Custom(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::Card {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::Card(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::Totp {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::Totp(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::Identity {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::Identity(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::Wifi {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::Wifi(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::License {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::License(fields.take().context("secret content is missing fields")?)
        }
        StoredEnvelope::SshKey {
            schema_version,
            fields,
        } => {
            validate_schema_version(*schema_version)?;
            SecretContent::SshKey(fields.take().context("secret content is missing fields")?)
        }
    };
    content.validate()?;
    Ok(content)
}

fn validate_schema_version(version: u32) -> Result<()> {
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
