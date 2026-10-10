//! Typed plaintext payloads for secrets.
//!
//! The JSON envelope is versioned independently from public secret metadata.
//! Only its encrypted bytes belong in [`super::Secret::ciphertext`].

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::SecretKind;

/// Current encrypted content envelope version.
pub const CONTENT_SCHEMA_VERSION: u32 = 1;
/// Maximum serialized or decoded content size in bytes.
pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;

const MAX_USERNAME_BYTES: usize = 16 * 1024;
const MAX_PASSWORD_BYTES: usize = 64 * 1024;
const MAX_URL_BYTES: usize = 8 * 1024;
const MAX_NOTES_BYTES: usize = 256 * 1024;
const MAX_NOTE_VALUE_BYTES: usize = 256 * 1024;

/// Login fields stored only in the encrypted payload.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct LoginContent {
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
}

/// Secure note fields stored only in the encrypted payload.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct NoteContent {
    pub value: String,
    pub notes: String,
}

/// Supported first-generation encrypted secret content.
#[derive(Zeroize, ZeroizeOnDrop)]
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
}

/// Encodes typed content as a bounded, versioned JSON envelope.
///
/// The returned buffer is zeroized on drop. Callers should encrypt it and then
/// let it leave scope without copying it into logs, diagnostics, or UI state.
pub fn encode_content(content: &SecretContent) -> Result<Zeroizing<Vec<u8>>> {
    validate_content(content)?;

    let mut bytes = Zeroizing::new(Vec::new());
    match content {
        SecretContent::Login(login) => serde_json::to_writer(
            &mut *bytes,
            &EnvelopeRef {
                schema_version: CONTENT_SCHEMA_VERSION,
                kind: SecretKind::Login.as_str(),
                fields: LoginFieldsRef {
                    username: &login.username,
                    password: &login.password,
                    url: &login.url,
                    notes: &login.notes,
                },
            },
        ),
        SecretContent::Note(note) => serde_json::to_writer(
            &mut *bytes,
            &EnvelopeRef {
                schema_version: CONTENT_SCHEMA_VERSION,
                kind: SecretKind::SecureNote.as_str(),
                fields: NoteFieldsRef {
                    value: &note.value,
                    notes: &note.notes,
                },
            },
        ),
    }
    .context("could not encode secret content")?;

    ensure!(
        bytes.len() <= MAX_CONTENT_BYTES,
        "secret content exceeds the maximum size"
    );
    Ok(bytes)
}

/// Decodes only known schema versions and known typed payloads.
///
/// Unknown versions, kinds, or fields are rejected so data is never silently
/// converted to a representation that would lose information on save.
pub fn decode_content(bytes: &[u8]) -> Result<SecretContent> {
    ensure!(
        bytes.len() <= MAX_CONTENT_BYTES,
        "secret content exceeds the maximum size"
    );

    let mut envelope: RawEnvelope =
        serde_json::from_slice(bytes).context("could not decode secret content envelope")?;
    ensure!(
        envelope.schema_version == CONTENT_SCHEMA_VERSION,
        "unsupported secret content schema version"
    );

    let kind = SecretKind::parse(&envelope.kind).context("unsupported secret content kind")?;
    let content = match kind {
        SecretKind::Login => {
            ensure!(
                envelope.fields.value.is_none(),
                "login content contains an unexpected field"
            );
            let content = LoginContent {
                username: envelope
                    .fields
                    .username
                    .take()
                    .context("login content is missing username")?,
                password: envelope
                    .fields
                    .password
                    .take()
                    .context("login content is missing password")?,
                url: envelope
                    .fields
                    .url
                    .take()
                    .context("login content is missing URL")?,
                notes: envelope
                    .fields
                    .notes
                    .take()
                    .context("login content is missing notes")?,
            };
            validate_login(&content)?;
            SecretContent::Login(content)
        }
        SecretKind::SecureNote => {
            ensure!(
                envelope.fields.username.is_none()
                    && envelope.fields.password.is_none()
                    && envelope.fields.url.is_none(),
                "note content contains an unexpected field"
            );
            let content = NoteContent {
                value: envelope
                    .fields
                    .value
                    .take()
                    .context("note content is missing value")?,
                notes: envelope
                    .fields
                    .notes
                    .take()
                    .context("note content is missing notes")?,
            };
            validate_note(&content)?;
            SecretContent::Note(content)
        }
        SecretKind::ApiToken
        | SecretKind::Totp
        | SecretKind::SshKey
        | SecretKind::Card
        | SecretKind::Identity => anyhow::bail!("unsupported secret content kind"),
    };

    Ok(content)
}

fn validate_content(content: &SecretContent) -> Result<()> {
    match content {
        SecretContent::Login(login) => validate_login(login),
        SecretContent::Note(note) => validate_note(note),
    }
}

fn validate_login(content: &LoginContent) -> Result<()> {
    ensure_field_size("username", &content.username, MAX_USERNAME_BYTES)?;
    ensure_field_size("password", &content.password, MAX_PASSWORD_BYTES)?;
    ensure_field_size("URL", &content.url, MAX_URL_BYTES)?;
    ensure_field_size("notes", &content.notes, MAX_NOTES_BYTES)
}

fn validate_note(content: &NoteContent) -> Result<()> {
    ensure_field_size("note value", &content.value, MAX_NOTE_VALUE_BYTES)?;
    ensure_field_size("notes", &content.notes, MAX_NOTES_BYTES)
}

fn ensure_field_size(field: &str, value: &str, maximum: usize) -> Result<()> {
    ensure!(
        value.len() <= maximum,
        "secret content {field} exceeds the maximum size"
    );
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvelopeRef<F> {
    schema_version: u32,
    kind: &'static str,
    fields: F,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginFieldsRef<'a> {
    username: &'a str,
    password: &'a str,
    url: &'a str,
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteFieldsRef<'a> {
    value: &'a str,
    notes: &'a str,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawEnvelope {
    schema_version: u32,
    kind: String,
    fields: RawFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawFields {
    username: Option<String>,
    password: Option<String>,
    url: Option<String>,
    value: Option<String>,
    notes: Option<String>,
}
