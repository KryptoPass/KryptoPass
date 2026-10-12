//! Variant-specific editor input and read-only projections.

use kryptopass_core::{
    CardContent, CustomContent, IdentityContent, LicenseContent, LoginContent, NoteContent,
    SecretContent, SshKeyContent, TotpContent, WifiContent, format_totp_input, parse_totp_input,
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
    #[serde(rename = "custom")]
    Custom(CustomContent),
    #[serde(rename = "card")]
    Card(CardContent),
    #[serde(rename = "totp")]
    Totp(TotpDraft),
    #[serde(rename = "identity")]
    Identity(IdentityContent),
    #[serde(rename = "wifi")]
    Wifi(WifiContent),
    #[serde(rename = "license")]
    License(LicenseContent),
    #[serde(rename = "ssh-key")]
    SshKey(SshKeyDraft),
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TotpDraft {
    totp_input: SensitiveText,
    notes: SensitiveText,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SshKeyDraft {
    #[serde(default)]
    pub(super) import_token: String,
    #[serde(default)]
    passphrase: SensitiveText,
    user: SensitiveText,
    hosts: Vec<String>,
    notes: SensitiveText,
    #[serde(default)]
    public_key: String,
    #[serde(default)]
    fingerprint: String,
    #[serde(default)]
    algorithm: String,
    #[serde(skip)]
    pub(super) prepared: Option<SshKeyContent>,
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
            Self::Custom(value) => Ok(SecretContent::Custom(std::mem::take(value))),
            Self::Card(value) => Ok(SecretContent::Card(std::mem::take(value))),
            Self::Identity(value) => Ok(SecretContent::Identity(std::mem::take(value))),
            Self::Wifi(value) => Ok(SecretContent::Wifi(std::mem::take(value))),
            Self::License(value) => Ok(SecretContent::License(std::mem::take(value))),
            Self::Totp(draft) => Ok(SecretContent::Totp(TotpContent {
                config: parse_totp_input(&draft.totp_input.0)?
                    .ok_or_else(|| anyhow::anyhow!("authenticator configuration is required"))?,
                notes: draft.notes.take(),
            })),
            Self::SshKey(draft) => {
                let mut value = draft
                    .prepared
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("import an SSH key before saving"))?;
                value.user = draft.user.take();
                value.hosts = std::mem::take(&mut draft.hosts);
                value.notes = draft.notes.take();
                Ok(SecretContent::SshKey(value))
            }
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
    #[serde(rename = "custom")]
    Custom(&'a CustomContent),
    #[serde(rename = "card")]
    Card(&'a CardContent),
    #[serde(rename = "identity")]
    Identity(&'a IdentityContent),
    #[serde(rename = "wifi")]
    Wifi(&'a WifiContent),
    #[serde(rename = "license")]
    License(&'a LicenseContent),
    #[serde(rename = "totp")]
    Totp(TotpEditor<'a>),
    #[serde(rename = "ssh-key")]
    SshKey(SshKeyEditor<'a>),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TotpEditor<'a> {
    #[serde(serialize_with = "serialize_sensitive_text")]
    totp_input: Zeroizing<String>,
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SshKeyEditor<'a> {
    import_token: &'static str,
    public_key: &'a str,
    fingerprint: &'a str,
    algorithm: &'a str,
    user: &'a str,
    hosts: &'a [String],
    notes: &'a str,
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
            SecretContent::Custom(value) => Self::Custom(value),
            SecretContent::Card(value) => Self::Card(value),
            SecretContent::Identity(value) => Self::Identity(value),
            SecretContent::Wifi(value) => Self::Wifi(value),
            SecretContent::License(value) => Self::License(value),
            SecretContent::Totp(value) => Self::Totp(TotpEditor {
                totp_input: format_totp_input(&value.config)?,
                notes: &value.notes,
            }),
            SecretContent::SshKey(value) => Self::SshKey(SshKeyEditor {
                import_token: "",
                public_key: &value.public_key,
                fingerprint: &value.fingerprint,
                algorithm: &value.algorithm,
                user: &value.user,
                hosts: &value.hosts,
                notes: &value.notes,
            }),
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
    #[serde(rename = "custom")]
    Custom(super::expanded::ExpandedDetail<'a>),
    #[serde(rename = "card")]
    Card(super::expanded::ExpandedDetail<'a>),
    #[serde(rename = "identity")]
    Identity(super::expanded::ExpandedDetail<'a>),
    #[serde(rename = "wifi")]
    Wifi(super::expanded::ExpandedDetail<'a>),
    #[serde(rename = "license")]
    License(super::expanded::ExpandedDetail<'a>),
    #[serde(rename = "totp")]
    Totp(super::expanded::ExpandedDetail<'a>),
    #[serde(rename = "ssh-key")]
    SshKey(super::expanded::ExpandedDetail<'a>),
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
        revealed_fields: &std::collections::BTreeSet<String>,
        qr_source: Option<&'a str>,
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
            SecretContent::Custom(_) => {
                Self::Custom(super::expanded::detail(content, unix_seconds, revealed_fields, qr_source)?)
            }
            SecretContent::Card(_) => Self::Card(super::expanded::detail(content, unix_seconds, revealed_fields, qr_source)?),
            SecretContent::Identity(_) => {
                Self::Identity(super::expanded::detail(content, unix_seconds, revealed_fields, qr_source)?)
            }
            SecretContent::Wifi(_) => Self::Wifi(super::expanded::detail(content, unix_seconds, revealed_fields, qr_source)?),
            SecretContent::License(_) => {
                Self::License(super::expanded::detail(content, unix_seconds, revealed_fields, qr_source)?)
            }
            SecretContent::Totp(_) => Self::Totp(super::expanded::detail(content, unix_seconds, revealed_fields, qr_source)?),
            SecretContent::SshKey(_) => {
                Self::SshKey(super::expanded::detail(content, unix_seconds, revealed_fields, qr_source)?)
            }
        })
    }
}

fn serialize_sensitive_text<S: serde::Serializer>(
    value: &Zeroizing<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(value.as_str())
}
