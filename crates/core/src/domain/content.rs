//! Typed plaintext payloads for encrypted secrets.

use anyhow::{Context, Result, ensure};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::SecretKind;
use crate::otp::validate_totp_config;
pub use crate::otp::{TotpAlgorithm, TotpConfig};

/// Current encrypted content envelope version.
pub const CONTENT_SCHEMA_VERSION: u32 = 3;
/// Maximum serialized or decoded content size in bytes.
pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;
/// Maximum retained login-history event count.
pub const MAX_LOGIN_HISTORY_EVENTS: usize = 100;
/// Maximum retained previous-password count.
pub const MAX_PREVIOUS_PASSWORDS: usize = 20;
/// Maximum combined byte size of retained previous passwords.
pub const MAX_HISTORY_PASSWORD_BYTES: usize = 512 * 1024;

const LEGACY_CONTENT_SCHEMA_VERSION: u32 = 1;
const LOGIN_HISTORY_SCHEMA_VERSION: u32 = 2;
const MAX_LOGIN_VERSIONS: usize = 20;
const MAX_USERNAME_BYTES: usize = 16 * 1024;
const MAX_PASSWORD_BYTES: usize = 64 * 1024;
const MAX_URL_BYTES: usize = 8 * 1024;
const MAX_URLS: usize = 100;
const MAX_NOTES_BYTES: usize = 256 * 1024;
const MAX_NOTE_VALUE_BYTES: usize = 256 * 1024;

/// Login fields stored only in the encrypted payload.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct LoginContent {
    pub username: String,
    pub password: String,
    /// Legacy primary URL, synchronized to `urls.first()`.
    pub url: String,
    pub notes: String,
    /// Complete ordered site list; `url` mirrors its first item.
    pub urls: Vec<String>,
    pub totp: Option<TotpConfig>,
    pub history: Box<LoginHistory>,
}

impl LoginContent {
    /// Replaces the complete ordered site list and synchronizes the legacy URL.
    pub fn set_urls(&mut self, urls: Vec<String>) {
        self.urls = urls;
        self.sync_primary_url();
    }

    /// Synchronizes the legacy URL from the first site, or clears it if empty.
    pub fn sync_primary_url(&mut self) {
        self.url = self.urls.first().cloned().unwrap_or_default();
    }
}

/// Secure note fields stored only in the encrypted payload.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct NoteContent {
    pub value: String,
    pub notes: String,
}

/// Metadata-only event kind recorded in a login's encrypted history.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoginHistoryEventKind {
    Create,
    LoginUse,
    PasswordRotated,
    Edited,
    CopyUsername,
    CopyPassword,
    CopyTotp,
    CopyNextTotp,
}

/// A timestamped login-history event with no copied credential value.
#[derive(Clone, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginHistoryEvent {
    #[zeroize(skip)]
    pub timestamp: u64,
    #[zeroize(skip)]
    pub kind: LoginHistoryEventKind,
}

/// A prior login password, kept only in encrypted history and never projected.
#[derive(Clone, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalPassword {
    password: String,
    #[zeroize(skip)]
    changed_at: u64,
}

/// A previous login state; history is excluded to avoid recursive snapshots.
#[derive(Clone, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoginVersion {
    #[zeroize(skip)]
    changed_at: u64,
    title: String,
    username: String,
    password: String,
    urls: Vec<String>,
    notes: String,
    totp: Option<TotpConfig>,
}

/// Credential history stored inside the encrypted login payload.
#[derive(Clone, Default, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginHistory {
    events: Vec<LoginHistoryEvent>,
    previous_passwords: Vec<HistoricalPassword>,
    #[serde(default)]
    versions: Vec<LoginVersion>,
}

impl LoginHistory {
    /// Retains a prior state inside the encrypted payload, without nesting history.
    pub fn record_version(
        &mut self,
        content: &LoginContent,
        title: &str,
        changed_at: u64,
    ) -> Result<()> {
        validate_login(content)?;
        ensure!(
            title.len() <= 512,
            "historical title exceeds the maximum size"
        );
        let version = LoginVersion {
            changed_at,
            title: title.to_owned(),
            username: content.username.clone(),
            password: content.password.clone(),
            urls: content.urls.clone(),
            notes: content.notes.clone(),
            totp: content.totp.clone(),
        };
        let index = self
            .versions
            .partition_point(|entry| entry.changed_at >= changed_at);
        self.versions.insert(index, version);
        self.versions.truncate(MAX_LOGIN_VERSIONS);
        while self.retained_version_bytes()? > MAX_HISTORY_PASSWORD_BYTES {
            if self.previous_passwords.pop().is_none() && self.versions.pop().is_none() {
                break;
            }
        }
        Ok(())
    }

    fn retained_version_bytes(&self) -> Result<usize> {
        let versions = Zeroizing::new(serde_json::to_vec(&self.versions)?);
        let passwords = Zeroizing::new(serde_json::to_vec(&self.previous_passwords)?);
        Ok(versions.len() + passwords.len())
    }

    /// Metadata for snapshots and legacy password-only entries, newest first.
    pub fn version_metadata(&self) -> Vec<(u64, bool)> {
        let mut entries: Vec<_> = self
            .versions
            .iter()
            .map(|entry| (entry.changed_at, false))
            .collect();
        entries.extend(
            self.previous_passwords
                .iter()
                .map(|entry| (entry.changed_at, true)),
        );
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        entries
    }

    /// Returns the original title when a complete snapshot is available.
    pub fn version_title(&self, index: usize) -> Option<&str> {
        let (at, password_only) = *self.version_metadata().get(index)?;
        if password_only {
            return None;
        }
        let ordinal = self.version_metadata()[..index]
            .iter()
            .filter(|entry| **entry == (at, false))
            .count();
        self.versions
            .iter()
            .filter(|entry| entry.changed_at == at)
            .nth(ordinal)
            .map(|entry| entry.title.as_str())
    }

    /// Opens one retained state in Rust. Legacy entries contain only a password.
    pub fn version_content(&self, index: usize) -> Option<LoginContent> {
        let (at, password_only) = *self.version_metadata().get(index)?;
        if password_only {
            let entry = self
                .previous_passwords
                .iter()
                .filter(|entry| entry.changed_at == at)
                .nth(
                    self.version_metadata()[..index]
                        .iter()
                        .filter(|entry| **entry == (at, true))
                        .count(),
                )?;
            return Some(LoginContent {
                username: String::new(),
                password: entry.password.clone(),
                url: String::new(),
                urls: vec![],
                notes: String::new(),
                totp: None,
                history: Box::default(),
            });
        }
        let entry = self
            .versions
            .iter()
            .filter(|entry| entry.changed_at == at)
            .nth(
                self.version_metadata()[..index]
                    .iter()
                    .filter(|entry| **entry == (at, false))
                    .count(),
            )?;
        Some(LoginContent {
            username: entry.username.clone(),
            password: entry.password.clone(),
            url: entry.urls.first().cloned().unwrap_or_default(),
            urls: entry.urls.clone(),
            notes: entry.notes.clone(),
            totp: entry.totp.clone(),
            history: Box::default(),
        })
    }

    /// Returns newest-first event metadata. Historical passwords are never exposed.
    pub fn events(&self) -> &[LoginHistoryEvent] {
        &self.events
    }

    /// Returns password-change timestamps without exposing prior password values.
    pub fn password_change_timestamps(&self) -> impl Iterator<Item = u64> + '_ {
        self.previous_passwords.iter().map(|entry| entry.changed_at)
    }

    /// Records an event in descending timestamp order and retains the newest 100.
    pub fn record_event(&mut self, kind: LoginHistoryEventKind, timestamp: u64) {
        let index = self
            .events
            .partition_point(|event| event.timestamp >= timestamp);
        self.events
            .insert(index, LoginHistoryEvent { timestamp, kind });
        self.events.truncate(MAX_LOGIN_HISTORY_EVENTS);
    }

    /// Saves a prior password, retaining at most 20 entries and 512 KiB total.
    /// Oldest entries are removed when either retention limit is exceeded.
    pub fn record_password(&mut self, password: String, changed_at: u64) -> Result<()> {
        let mut password = Zeroizing::new(password);
        ensure!(
            password.len() <= MAX_PASSWORD_BYTES,
            "historical password exceeds the maximum size"
        );
        let index = self
            .previous_passwords
            .partition_point(|entry| entry.changed_at >= changed_at);
        self.previous_passwords.insert(
            index,
            HistoricalPassword {
                password: std::mem::take(&mut *password),
                changed_at,
            },
        );
        self.previous_passwords.truncate(MAX_PREVIOUS_PASSWORDS);

        let mut total_bytes = self
            .previous_passwords
            .iter()
            .map(|entry| entry.password.len())
            .sum::<usize>();
        while total_bytes > MAX_HISTORY_PASSWORD_BYTES {
            if let Some(oldest) = self.previous_passwords.pop() {
                total_bytes -= oldest.password.len();
            } else {
                break;
            }
        }
        Ok(())
    }
}

/// Typed secret content currently supported by the core.
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
                    url: login.urls.first().map(String::as_str).unwrap_or_default(),
                    notes: &login.notes,
                    urls: &login.urls,
                    totp: login.totp.as_ref(),
                    history: &login.history,
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

/// Decodes supported envelopes, preserving legacy events and prior passwords.
/// Schema 1 login payloads migrate to a one-entry URL list and empty OTP/history
/// fields. Schema 1 payloads containing schema 2 login fields are rejected.
pub fn decode_content(bytes: &[u8]) -> Result<SecretContent> {
    ensure!(
        bytes.len() <= MAX_CONTENT_BYTES,
        "secret content exceeds the maximum size"
    );

    let mut envelope: RawEnvelope =
        serde_json::from_slice(bytes).context("could not decode secret content envelope")?;
    ensure!(
        matches!(
            envelope.schema_version,
            LEGACY_CONTENT_SCHEMA_VERSION | LOGIN_HISTORY_SCHEMA_VERSION | CONTENT_SCHEMA_VERSION
        ),
        "unsupported secret content schema version"
    );

    let schema_version = envelope.schema_version;
    let kind = SecretKind::parse(&envelope.kind).context("unsupported secret content kind")?;
    let content = match kind {
        SecretKind::Login => {
            ensure!(
                envelope.fields.value.is_none(),
                "login content contains an unexpected field"
            );
            let (urls, totp, history) = match schema_version {
                LEGACY_CONTENT_SCHEMA_VERSION => {
                    ensure_v2_fields_absent(&envelope.fields)?;
                    (None, None, None)
                }
                LOGIN_HISTORY_SCHEMA_VERSION | CONTENT_SCHEMA_VERSION => {
                    let urls =
                        take_value(&mut envelope.fields.urls, "login content is missing URLs")?;
                    let totp = take_nullable(
                        &mut envelope.fields.totp,
                        "login content is missing TOTP field",
                    )?;
                    let history = take_value(
                        &mut envelope.fields.history,
                        "login content is missing history",
                    )?;
                    (Some(urls), Some(totp), Some(history))
                }
                _ => anyhow::bail!("unsupported secret content schema version"),
            };

            let username = take_required(
                &mut envelope.fields.username,
                "login content is missing username",
            )?;
            let password = take_required(
                &mut envelope.fields.password,
                "login content is missing password",
            )?;
            let url = take_required(&mut envelope.fields.url, "login content is missing URL")?;
            let notes =
                take_required(&mut envelope.fields.notes, "login content is missing notes")?;
            let urls = urls.unwrap_or_else(|| {
                if url.is_empty() {
                    Vec::new()
                } else {
                    vec![url.clone()]
                }
            });
            let content = LoginContent {
                username,
                password,
                url,
                notes,
                urls,
                totp: totp.unwrap_or(None),
                history: Box::new(history.unwrap_or_default()),
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
            ensure_v2_fields_absent(&envelope.fields)?;
            let value = take_required(&mut envelope.fields.value, "note content is missing value")?;
            let notes = take_required(&mut envelope.fields.notes, "note content is missing notes")?;
            let content = NoteContent { value, notes };
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
    ensure_field_size("notes", &content.notes, MAX_NOTES_BYTES)?;
    ensure!(
        content.urls.len() <= MAX_URLS,
        "login content contains too many URLs"
    );
    ensure!(
        content.urls.iter().all(|url| !url.trim().is_empty()),
        "login content contains an empty URL"
    );
    for url in &content.urls {
        ensure_field_size("URL", url, MAX_URL_BYTES)?;
    }
    ensure!(
        content.url == content.urls.first().map(String::as_str).unwrap_or_default(),
        "login content URL does not match the primary URL in its URL list"
    );
    if let Some(totp) = &content.totp {
        validate_totp_config(totp)?;
    }
    validate_history(&content.history)
}

fn validate_history(history: &LoginHistory) -> Result<()> {
    ensure!(
        history.versions.len() <= MAX_LOGIN_VERSIONS,
        "too many login versions"
    );
    ensure!(
        history
            .versions
            .windows(2)
            .all(|pair| pair[0].changed_at >= pair[1].changed_at),
        "login versions are not newest-first"
    );
    if !history.versions.is_empty() {
        ensure!(
            history.retained_version_bytes()? <= MAX_HISTORY_PASSWORD_BYTES,
            "login versions exceed the history size limit"
        );
    }
    for entry in &history.versions {
        ensure!(
            entry.title.len() <= 512,
            "historical title exceeds the maximum size"
        );
        let login = LoginContent {
            username: entry.username.clone(),
            password: entry.password.clone(),
            url: entry.urls.first().cloned().unwrap_or_default(),
            urls: entry.urls.clone(),
            notes: entry.notes.clone(),
            totp: entry.totp.clone(),
            history: Box::default(),
        };
        validate_login(&login)?;
    }

    ensure!(
        history.events.len() <= MAX_LOGIN_HISTORY_EVENTS,
        "login history contains too many events"
    );
    ensure!(
        history
            .events
            .windows(2)
            .all(|pair| pair[0].timestamp >= pair[1].timestamp),
        "login history events are not newest-first"
    );
    ensure!(
        history.previous_passwords.len() <= MAX_PREVIOUS_PASSWORDS,
        "login history contains too many previous passwords"
    );
    ensure!(
        history
            .previous_passwords
            .windows(2)
            .all(|pair| pair[0].changed_at >= pair[1].changed_at),
        "previous passwords are not newest-first"
    );
    let mut total_password_bytes = 0usize;
    for historical in &history.previous_passwords {
        ensure_field_size(
            "historical password",
            &historical.password,
            MAX_PASSWORD_BYTES,
        )?;
        total_password_bytes = total_password_bytes.saturating_add(historical.password.len());
    }
    ensure!(
        total_password_bytes <= MAX_HISTORY_PASSWORD_BYTES,
        "previous passwords exceed the history size limit"
    );
    Ok(())
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

fn ensure_v2_fields_absent(fields: &RawFields) -> Result<()> {
    ensure!(
        !fields.urls.present && !fields.totp.present && !fields.history.present,
        "content schema version 1 or note payload contains unexpected login fields"
    );
    Ok(())
}

fn take_required(field: &mut Option<String>, message: &'static str) -> Result<String> {
    field.take().context(message)
}

fn take_value<T: Zeroize>(field: &mut Present<T>, message: &'static str) -> Result<T> {
    ensure!(field.present, message);
    field.value.take().context(message)
}

fn take_nullable<T: Zeroize>(field: &mut Present<T>, message: &'static str) -> Result<Option<T>> {
    ensure!(field.present, message);
    Ok(field.value.take())
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
    urls: &'a [String],
    totp: Option<&'a TotpConfig>,
    history: &'a LoginHistory,
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
    #[serde(default)]
    urls: Present<Vec<String>>,
    #[serde(default)]
    totp: Present<TotpConfig>,
    #[serde(default)]
    history: Present<LoginHistory>,
}

struct Present<T: Zeroize> {
    present: bool,
    value: Option<T>,
}

impl<T: Zeroize> Zeroize for Present<T> {
    fn zeroize(&mut self) {
        self.present.zeroize();
        if let Some(value) = &mut self.value {
            value.zeroize();
        }
        self.value = None;
    }
}

impl<T: Zeroize> ZeroizeOnDrop for Present<T> {}

impl<T: Zeroize> Drop for Present<T> {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl<T: Zeroize> Default for Present<T> {
    fn default() -> Self {
        Self {
            present: false,
            value: None,
        }
    }
}

impl<'de, T> Deserialize<'de> for Present<T>
where
    T: Deserialize<'de> + Zeroize,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<T>::deserialize(deserializer)?;
        Ok(Self {
            present: true,
            value,
        })
    }
}
