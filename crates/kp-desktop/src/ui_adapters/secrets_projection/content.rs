//! Variant-specific editor input and read-only projections.

use kryptopass_core::{
    CardContent, CustomContent, IdentityContent, LicenseContent, LoginContent, NoteContent,
    SecretContent, SshKeyContent, TotpContent, WifiContent, format_totp_input, parse_totp_input,
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{
    HistoryProjection, SensitiveText, TotpProjection, history_projection, totp_projection,
};

#[derive(Zeroize, ZeroizeOnDrop)]
pub(super) enum DraftContent {
    Login(LoginDraft),
    Note(NoteDraft),
    Custom(CustomContent),
    Card(CardContent),
    Totp(TotpDraft),
    Identity(IdentityContent),
    Wifi(WifiContent),
    License(LicenseContent),
    SshKey(Box<SshKeyDraft>),
}

impl<'de> Deserialize<'de> for DraftContent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        enum Kind {
            Login,
            Note,
            Custom,
            Card,
            Totp,
            Identity,
            Wifi,
            License,
            SshKey,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope<'a> {
            kind: Kind,
            #[serde(borrow)]
            fields: &'a serde_json::value::RawValue,
        }
        let envelope = Envelope::deserialize(deserializer)?;
        macro_rules! decode {
            ($variant:ident) => {
                serde_json::from_str(envelope.fields.get())
                    .map(Self::$variant)
                    .map_err(serde::de::Error::custom)
            };
        }
        match envelope.kind {
            Kind::Login => decode!(Login),
            Kind::Note => decode!(Note),
            Kind::Custom => decode!(Custom),
            Kind::Card => decode!(Card),
            Kind::Totp => decode!(Totp),
            Kind::Identity => decode!(Identity),
            Kind::Wifi => decode!(Wifi),
            Kind::License => decode!(License),
            Kind::SshKey => decode!(SshKey),
        }
    }
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
    hosts: Vec<SensitiveText>,
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
    urls: Vec<SensitiveText>,
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
                let totp = parse_totp_input(&draft.totp_input)?;
                Ok(SecretContent::Login(LoginContent {
                    username: std::mem::take(&mut draft.username),
                    password: std::mem::take(&mut draft.password),
                    urls: std::mem::take(&mut draft.urls),
                    totp,
                    notes: std::mem::take(&mut draft.notes),
                    history: Box::default(),
                }))
            }
            Self::Note(draft) => Ok(SecretContent::Note(NoteContent {
                value: std::mem::take(&mut draft.value),
                notes: std::mem::take(&mut draft.notes),
            })),
            Self::Custom(value) => Ok(SecretContent::Custom(std::mem::take(value))),
            Self::Card(value) => Ok(SecretContent::Card(std::mem::take(value))),
            Self::Identity(value) => Ok(SecretContent::Identity(std::mem::take(value))),
            Self::Wifi(value) => Ok(SecretContent::Wifi(std::mem::take(value))),
            Self::License(value) => Ok(SecretContent::License(std::mem::take(value))),
            Self::Totp(draft) => Ok(SecretContent::Totp(TotpContent {
                config: parse_totp_input(&draft.totp_input)?
                    .ok_or_else(|| anyhow::anyhow!("authenticator configuration is required"))?,
                notes: std::mem::take(&mut draft.notes),
            })),
            Self::SshKey(draft) => {
                let mut value = draft
                    .prepared
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("import an SSH key before saving"))?;
                value.user = std::mem::take(&mut draft.user);
                value.hosts = std::mem::take(&mut draft.hosts);
                value.notes = std::mem::take(&mut draft.notes);
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
pub(super) struct TotpEditor<'a> {
    #[serde(serialize_with = "serialize_sensitive_text")]
    totp_input: SensitiveText,
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SshKeyEditor<'a> {
    import_token: &'static str,
    public_key: &'a str,
    fingerprint: &'a str,
    algorithm: &'a str,
    user: &'a str,
    hosts: &'a [SensitiveText],
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoginEditor<'a> {
    username: &'a str,
    password: &'a str,
    urls: &'a [SensitiveText],
    #[serde(serialize_with = "serialize_sensitive_text")]
    totp_input: SensitiveText,
    notes: &'a str,
}

impl<'a> EditorContent<'a> {
    pub(super) fn new(content: &'a SecretContent) -> anyhow::Result<Self> {
        Ok(match content {
            SecretContent::Login(login) => Self::Login(LoginEditor {
                username: &login.username,
                password: &login.password,
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
    urls: &'a [SensitiveText],
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
            SecretContent::Custom(_) => Self::Custom(super::expanded::detail(
                content,
                unix_seconds,
                revealed_fields,
                qr_source,
            )?),
            SecretContent::Card(_) => Self::Card(super::expanded::detail(
                content,
                unix_seconds,
                revealed_fields,
                qr_source,
            )?),
            SecretContent::Identity(_) => Self::Identity(super::expanded::detail(
                content,
                unix_seconds,
                revealed_fields,
                qr_source,
            )?),
            SecretContent::Wifi(_) => Self::Wifi(super::expanded::detail(
                content,
                unix_seconds,
                revealed_fields,
                qr_source,
            )?),
            SecretContent::License(_) => Self::License(super::expanded::detail(
                content,
                unix_seconds,
                revealed_fields,
                qr_source,
            )?),
            SecretContent::Totp(_) => Self::Totp(super::expanded::detail(
                content,
                unix_seconds,
                revealed_fields,
                qr_source,
            )?),
            SecretContent::SshKey(_) => Self::SshKey(super::expanded::detail(
                content,
                unix_seconds,
                revealed_fields,
                qr_source,
            )?),
        })
    }
}

fn serialize_sensitive_text<S: serde::Serializer>(
    value: &SensitiveText,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(value.as_str())
}
