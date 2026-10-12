//! Variant-specific editor input and read-only projections.

use kryptopass_core::{
    LoginContent, NoteContent, SecretContent, format_totp_input, parse_totp_input,
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::{
    HistoryProjection, SensitiveText, TotpProjection, history_projection, totp_projection,
};

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(tag = "kind", content = "fields", deny_unknown_fields)]
pub(super) enum DraftContent {
    #[serde(rename = "login")]
    Login(LoginDraft),
    #[serde(rename = "note")]
    Note(NoteDraft),
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LoginDraft {
    username: SensitiveText,
    password: SensitiveText,
    url: SensitiveText,
    #[serde(default)]
    urls: Option<Vec<SensitiveText>>,
    #[serde(default)]
    totp_input: SensitiveText,
    notes: SensitiveText,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteDraft {
    value: SensitiveText,
    notes: SensitiveText,
}

impl DraftContent {
    pub(super) fn prepare(&mut self) -> anyhow::Result<SecretContent> {
        match self {
            Self::Login(draft) => {
                let totp = parse_totp_input(&draft.totp_input.0)?;
                let urls = match draft.urls.take() {
                    Some(urls) => urls.into_iter().map(|mut url| url.take()).collect(),
                    None if draft.url.0.is_empty() => vec![],
                    None => vec![draft.url.take()],
                };
                Ok(SecretContent::Login(LoginContent {
                    username: draft.username.take(),
                    password: draft.password.take(),
                    url: urls.first().cloned().unwrap_or_default(),
                    urls,
                    totp,
                    notes: draft.notes.take(),
                    history: Box::default(),
                }))
            }
            Self::Note(draft) => Ok(SecretContent::Note(NoteContent {
                value: draft.value.take(),
                notes: draft.notes.take(),
            })),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "fields")]
pub(super) enum EditorContent<'a> {
    #[serde(rename = "login")]
    Login(LoginEditor<'a>),
    #[serde(rename = "note")]
    Note(&'a NoteContent),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoginEditor<'a> {
    username: &'a str,
    password: &'a str,
    url: &'a str,
    urls: &'a [String],
    #[serde(serialize_with = "serialize_sensitive_text")]
    totp_input: Zeroizing<String>,
    notes: &'a str,
}

impl<'a> EditorContent<'a> {
    pub(super) fn new(content: &'a SecretContent) -> anyhow::Result<Self> {
        Ok(match content {
            SecretContent::Login(login) => Self::Login(LoginEditor {
                username: &login.username,
                password: &login.password,
                url: &login.url,
                urls: &login.urls,
                totp_input: login
                    .totp
                    .as_ref()
                    .map(format_totp_input)
                    .transpose()?
                    .unwrap_or_default(),
                notes: &login.notes,
            }),
            SecretContent::Note(note) => Self::Note(note),
        })
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "fields")]
pub(super) enum DetailContent<'a> {
    #[serde(rename = "login")]
    Login(Box<LoginDetail<'a>>),
    #[serde(rename = "note")]
    Note(&'a NoteContent),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoginDetail<'a> {
    username: &'a str,
    url: &'a str,
    urls: &'a [String],
    notes: &'a str,
    password: &'a str,
    password_revealed: bool,
    totp: Option<TotpProjection>,
    history: HistoryProjection,
}

impl<'a> DetailContent<'a> {
    pub(super) fn new(
        content: &'a SecretContent,
        reveal_password: bool,
        unix_seconds: u64,
    ) -> anyhow::Result<Self> {
        Ok(match content {
            SecretContent::Login(login) => Self::Login(Box::new(LoginDetail {
                username: &login.username,
                url: &login.url,
                urls: &login.urls,
                notes: &login.notes,
                password: if reveal_password { &login.password } else { "" },
                password_revealed: reveal_password,
                totp: login
                    .totp
                    .as_ref()
                    .map(|config| totp_projection(config, unix_seconds))
                    .transpose()?,
                history: history_projection(&login.history),
            })),
            SecretContent::Note(note) => Self::Note(note),
        })
    }
}

fn serialize_sensitive_text<S: serde::Serializer>(
    value: &Zeroizing<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(value.as_str())
}
