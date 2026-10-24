//! Login payload and encrypted history rules.

use crate::SensitiveText;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{MAX_HISTORY_PASSWORD_BYTES, MAX_LOGIN_HISTORY_EVENTS, ensure_field_size};
use crate::otp::{TotpConfig, validate_totp_config};

const MAX_LOGIN_VERSIONS: usize = 20;
const MAX_USERNAME_BYTES: usize = 16 * 1024;
const MAX_PASSWORD_BYTES: usize = 64 * 1024;
const MAX_URL_BYTES: usize = 8 * 1024;
const MAX_URLS: usize = 100;
const MAX_NOTES_BYTES: usize = 256 * 1024;

/// Login fields stored only in the encrypted payload.
#[derive(Clone, Default, Zeroize, ZeroizeOnDrop)]
pub struct LoginContent {
    pub username: SensitiveText,
    pub password: SensitiveText,
    pub notes: SensitiveText,
    /// Complete ordered site list; the first item is the primary site.
    pub urls: Vec<SensitiveText>,
    pub totp: Option<TotpConfig>,
    pub history: Box<LoginHistory>,
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

/// A previous login state; history is excluded to avoid recursive snapshots.
#[derive(Clone, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoginVersion {
    #[zeroize(skip)]
    changed_at: u64,
    title: SensitiveText,
    username: SensitiveText,
    password: SensitiveText,
    urls: Vec<SensitiveText>,
    notes: SensitiveText,
    totp: Option<TotpConfig>,
}

/// Credential history stored inside the encrypted login payload.
#[derive(Clone, Default, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginHistory {
    events: Vec<LoginHistoryEvent>,
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
            title: title.into(),
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
            if self.versions.pop().is_none() {
                break;
            }
        }
        Ok(())
    }

    fn retained_version_bytes(&self) -> Result<usize> {
        // Count bytes without materializing another plaintext history buffer.
        struct SizeWriter(usize);
        impl std::io::Write for SizeWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self.0.checked_add(bytes.len()).ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "history size overflow")
                })?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut size = SizeWriter(0);
        serde_json::to_writer(&mut size, &self.versions)?;
        Ok(size.0)
    }

    /// Timestamps of the retained complete versions, newest first.
    pub fn version_metadata(&self) -> Vec<u64> {
        self.versions.iter().map(|entry| entry.changed_at).collect()
    }

    /// Returns the original title of one retained version.
    pub fn version_title(&self, index: usize) -> Option<&str> {
        self.versions.get(index).map(|entry| entry.title.as_str())
    }

    /// Opens one retained state in Rust.
    pub fn version_content(&self, index: usize) -> Option<LoginContent> {
        let entry = self.versions.get(index)?;
        Some(LoginContent {
            username: entry.username.clone(),
            password: entry.password.clone(),
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

    /// Records an event in descending timestamp order and retains the newest 100.
    pub fn record_event(&mut self, kind: LoginHistoryEventKind, timestamp: u64) {
        let index = self
            .events
            .partition_point(|event| event.timestamp >= timestamp);
        self.events
            .insert(index, LoginHistoryEvent { timestamp, kind });
        self.events.truncate(MAX_LOGIN_HISTORY_EVENTS);
    }
}

pub(super) fn validate_login(content: &LoginContent) -> Result<()> {
    ensure_field_size("username", &content.username, MAX_USERNAME_BYTES)?;
    ensure_field_size("password", &content.password, MAX_PASSWORD_BYTES)?;
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
    Ok(())
}

impl LoginContent {
    pub(super) fn record_save(
        &mut self,
        previous: Option<&mut Self>,
        previous_title: &str,
        metadata_changed: bool,
        timestamp: u64,
    ) -> Result<()> {
        let Some(previous) = previous else {
            *self.history = LoginHistory::default();
            self.history
                .record_event(LoginHistoryEventKind::Create, timestamp);
            return Ok(());
        };
        let password_rotated = self.password != previous.password;
        let content_changed = self.username != previous.username
            || password_rotated
            || self.urls != previous.urls
            || self.totp != previous.totp
            || self.notes != previous.notes;
        let mut history = std::mem::take(&mut previous.history);
        if content_changed || metadata_changed {
            history.record_version(previous, previous_title, timestamp)?;
        }
        self.history = history;
        if password_rotated {
            self.history
                .record_event(LoginHistoryEventKind::PasswordRotated, timestamp);
        }
        if content_changed || metadata_changed {
            self.history
                .record_event(LoginHistoryEventKind::Edited, timestamp);
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FieldsRef<'a> {
    username: &'a str,
    password: &'a str,
    notes: &'a str,
    urls: &'a [SensitiveText],
    totp: Option<&'a TotpConfig>,
    history: &'a LoginHistory,
}

impl<'a> From<&'a LoginContent> for FieldsRef<'a> {
    fn from(content: &'a LoginContent) -> Self {
        Self {
            username: &content.username,
            password: &content.password,
            notes: &content.notes,
            urls: &content.urls,
            totp: content.totp.as_ref(),
            history: &content.history,
        }
    }
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoredFields {
    username: SensitiveText,
    password: SensitiveText,
    notes: SensitiveText,
    urls: Vec<SensitiveText>,
    // Required, but may be null.
    #[serde(deserialize_with = "Option::deserialize")]
    totp: Option<TotpConfig>,
    history: LoginHistory,
}

impl StoredFields {
    pub(super) fn take_content(&mut self) -> LoginContent {
        LoginContent {
            username: std::mem::take(&mut self.username),
            password: std::mem::take(&mut self.password),
            notes: std::mem::take(&mut self.notes),
            urls: std::mem::take(&mut self.urls),
            totp: self.totp.take(),
            history: Box::new(std::mem::take(&mut self.history)),
        }
    }
}
