//! How QML's per-kind editors and details map to template fields.
//!
//! A kind is the template of the same id. Templates without their own editor
//! (imported bank accounts, passports…) use the custom editor, one record per
//! field. Saving starts from the content that was opened, so fields an editor
//! does not show survive.

use std::sync::Arc;

use anyhow::{Context, Result};
use kp_template::{Multiplicity, Template};
use kryptopass_core::vault::TemplateRegistry;
use kryptopass_core::{
    CustomValue, FieldValue, ItemContent, SensitiveText, format_totp_input, inspect_ssh_key,
    parse_totp_input,
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{DetailView, SshKeyMaterial, TotpProjection, history_projection, totp_projection};

const NOTES: &str = "notes";

/// The editor and detail a template uses.
pub(in crate::ui_adapters) fn kind_of(template: &str) -> &'static str {
    match template {
        "login" => "login",
        "note" => "note",
        "card" => "card",
        "totp" => "totp",
        "identity" => "identity",
        "wifi" => "wifi",
        "license" => "license",
        "ssh-key" => "ssh-key",
        _ => "custom",
    }
}

/// The templates and one template of them, which live for the process.
pub(in crate::ui_adapters) fn template_of(
    id: &str,
) -> Result<(&'static Arc<TemplateRegistry>, &'static Template)> {
    let templates = crate::vault_access::templates()?;
    Ok((templates, templates.template(id)?))
}

fn text<'a>(content: &'a ItemContent, field: &str) -> &'a str {
    content
        .value(field)
        .and_then(FieldValue::first)
        .map_or("", |value| value.as_str())
}

fn list<'a>(content: &'a ItemContent, field: &str) -> &'a [SensitiveText] {
    content.value(field).map_or(&[], FieldValue::texts)
}

/// The first text of a field, for copying.
pub(in crate::ui_adapters) fn field_text(content: &ItemContent, field: &str) -> SensitiveText {
    text(content, field).into()
}

/// Sets a single-value field, clearing it when empty. A field the template
/// does not have is ignored: the editor shows the same form for every version.
fn put(content: &mut ItemContent, template: &Template, field: &str, value: SensitiveText) {
    let Some(field) = template.field(field) else {
        return;
    };
    if value.is_empty() {
        content.remove(field.id.as_str());
    } else if field.multiplicity == Multiplicity::Single {
        content.set(field.id.clone(), FieldValue::One(value));
    } else {
        content.set(field.id.clone(), FieldValue::Many(vec![value]));
    }
}

fn put_list(
    content: &mut ItemContent,
    template: &Template,
    field: &str,
    mut values: Vec<SensitiveText>,
) {
    let Some(field) = template.field(field) else {
        return;
    };
    values.retain(|value| !value.is_empty());
    if values.is_empty() {
        content.remove(field.id.as_str());
    } else if field.multiplicity == Multiplicity::Single {
        if let Some(value) = values.into_iter().next() {
            content.set(field.id.clone(), FieldValue::One(value));
        }
    } else {
        content.set(field.id.clone(), FieldValue::Many(values));
    }
}

fn section_for_custom(template: &Template) -> Option<kp_template::SectionId> {
    template
        .sections
        .iter()
        .find(|section| section.custom_fields.is_some())
        .map(|section| section.id.clone())
}

fn parse_totp(input: &str) -> Option<kryptopass_core::otp::TotpConfig> {
    parse_totp_input(input).ok().flatten()
}

macro_rules! draft {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub(in crate::ui_adapters) struct $name {
            $(#[serde(default)] $field: $ty,)*
        }
    };
}

draft!(LoginDraft { username: SensitiveText, password: SensitiveText, urls: Vec<SensitiveText>, totp_input: SensitiveText, notes: SensitiveText });
draft!(NoteDraft {
    value: SensitiveText,
    notes: SensitiveText
});
draft!(TotpDraft {
    totp_input: SensitiveText,
    notes: SensitiveText
});
draft!(CardDraft {
    holder: SensitiveText,
    number: SensitiveText,
    expiry_month: u8,
    expiry_year: u16,
    cvv: SensitiveText,
    pin: SensitiveText,
    notes: SensitiveText
});
draft!(WifiDraft {
    ssid: SensitiveText,
    security: SensitiveText,
    password: SensitiveText,
    hidden: bool,
    notes: SensitiveText
});
draft!(LicenseDraft {
    product: SensitiveText,
    license_key: SensitiveText,
    holder: SensitiveText,
    email: SensitiveText,
    version: SensitiveText,
    purchased: SensitiveText,
    expires: SensitiveText,
    notes: SensitiveText
});
draft!(IdentityDraft { first_name: SensitiveText, last_name: SensitiveText, documents: Vec<DocumentDraft>, emails: Vec<SensitiveText>, phones: Vec<SensitiveText>, address: SensitiveText, notes: SensitiveText });
draft!(DocumentDraft {
    id: SensitiveText,
    document_type: SensitiveText,
    number: SensitiveText,
    country: SensitiveText,
    expires: SensitiveText
});
draft!(CustomDraft { records: Vec<RecordDraft>, notes: SensitiveText });
draft!(RecordDraft {
    id: SensitiveText,
    key: SensitiveText,
    value: SensitiveText,
    multiline: bool,
    concealed: bool
});
draft!(SshKeyDraft { import_token: String, passphrase: SensitiveText, user: SensitiveText, hosts: Vec<SensitiveText>, notes: SensitiveText, public_key: String, fingerprint: String, algorithm: String });

#[derive(Zeroize, ZeroizeOnDrop)]
pub(in crate::ui_adapters) enum DraftContent {
    Login(LoginDraft),
    Note(NoteDraft),
    Custom(CustomDraft),
    Card(CardDraft),
    Totp(TotpDraft),
    Identity(IdentityDraft),
    Wifi(WifiDraft),
    License(LicenseDraft),
    SshKey(Box<SshKeyDraft>, #[zeroize(skip)] Option<SshKeyMaterial>),
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
            Kind::SshKey => serde_json::from_str(envelope.fields.get())
                .map(|draft| Self::SshKey(Box::new(draft), None))
                .map_err(serde::de::Error::custom),
        }
    }
}

impl DraftContent {
    fn template_id(&self) -> &'static str {
        match self {
            Self::Login(_) => "login",
            Self::Note(_) => "note",
            Self::Custom(_) => "custom",
            Self::Card(_) => "card",
            Self::Totp(_) => "totp",
            Self::Identity(_) => "identity",
            Self::Wifi(_) => "wifi",
            Self::License(_) => "license",
            Self::SshKey(..) => "ssh-key",
        }
    }

    pub(super) fn ssh_import_token(&self) -> Option<&str> {
        match self {
            Self::SshKey(draft, _) => Some(&draft.import_token),
            _ => None,
        }
    }

    pub(super) fn attach_ssh(&mut self, key: SshKeyMaterial) {
        if let Self::SshKey(_, material) = self {
            *material = Some(key);
        }
    }

    /// Applies the draft on top of `base`, or on new content of its kind.
    pub(super) fn prepare(&mut self, base: Option<ItemContent>) -> Result<ItemContent> {
        let template_id = base.as_ref().map_or(self.template_id().to_owned(), |base| {
            base.template().as_str().to_owned()
        });
        let (_templates, template) = template_of(&template_id)?;
        let mut content = base.unwrap_or_else(|| ItemContent::new(template));
        let take = |value: &mut SensitiveText| std::mem::take(value);
        match self {
            Self::Login(draft) => {
                if !draft.totp_input.is_empty() {
                    parse_totp_input(&draft.totp_input)?;
                }
                put(
                    &mut content,
                    template,
                    "username",
                    take(&mut draft.username),
                );
                put(
                    &mut content,
                    template,
                    "password",
                    take(&mut draft.password),
                );
                put_list(
                    &mut content,
                    template,
                    "urls",
                    std::mem::take(&mut draft.urls),
                );
                put(&mut content, template, "totp", take(&mut draft.totp_input));
                put(&mut content, template, NOTES, take(&mut draft.notes));
            }
            Self::Note(draft) => {
                // A note is its body; text in the old separate field joins it.
                let mut body = take(&mut draft.value);
                if !draft.notes.is_empty() {
                    if !body.is_empty() {
                        body.push_str("\n\n");
                    }
                    body.push_str(&draft.notes);
                }
                put(&mut content, template, "body", body);
            }
            Self::Totp(draft) => {
                anyhow::ensure!(
                    parse_totp_input(&draft.totp_input)?.is_some(),
                    "authenticator configuration is required"
                );
                put(&mut content, template, "uri", take(&mut draft.totp_input));
                put(&mut content, template, NOTES, take(&mut draft.notes));
            }
            Self::Card(draft) => {
                put(&mut content, template, "holder", take(&mut draft.holder));
                put(&mut content, template, "number", take(&mut draft.number));
                let expiry = if draft.expiry_month > 0 && draft.expiry_year > 0 {
                    format!("{:04}-{:02}", draft.expiry_year, draft.expiry_month)
                } else {
                    String::new()
                };
                put(&mut content, template, "expiry", expiry.into());
                put(&mut content, template, "cvv", take(&mut draft.cvv));
                put(&mut content, template, "pin", take(&mut draft.pin));
                put(&mut content, template, NOTES, take(&mut draft.notes));
            }
            Self::Wifi(draft) => {
                put(&mut content, template, "ssid", take(&mut draft.ssid));
                let security = match draft.security.as_str() {
                    "open" | "none" => "none",
                    "wep" => "wep",
                    "wpa3" => "wpa3",
                    _ => "wpa",
                };
                put(&mut content, template, "security", security.into());
                put(
                    &mut content,
                    template,
                    "password",
                    take(&mut draft.password),
                );
                put(
                    &mut content,
                    template,
                    "hidden",
                    if draft.hidden {
                        "true".into()
                    } else {
                        SensitiveText::new()
                    },
                );
                put(&mut content, template, NOTES, take(&mut draft.notes));
            }
            Self::License(draft) => {
                put(&mut content, template, "product", take(&mut draft.product));
                put(
                    &mut content,
                    template,
                    "license-key",
                    take(&mut draft.license_key),
                );
                put(
                    &mut content,
                    template,
                    "licensed-to",
                    take(&mut draft.holder),
                );
                put(&mut content, template, "email", take(&mut draft.email));
                put(&mut content, template, "version", take(&mut draft.version));
                put(
                    &mut content,
                    template,
                    "purchase-date",
                    take(&mut draft.purchased),
                );
                put(
                    &mut content,
                    template,
                    "expiry-date",
                    take(&mut draft.expires),
                );
                put(&mut content, template, NOTES, take(&mut draft.notes));
            }
            Self::Identity(draft) => {
                put(
                    &mut content,
                    template,
                    "first-name",
                    take(&mut draft.first_name),
                );
                put(
                    &mut content,
                    template,
                    "last-name",
                    take(&mut draft.last_name),
                );
                put_list(
                    &mut content,
                    template,
                    "email",
                    std::mem::take(&mut draft.emails),
                );
                put_list(
                    &mut content,
                    template,
                    "phone",
                    std::mem::take(&mut draft.phones),
                );
                put(&mut content, template, "street", take(&mut draft.address));
                put(&mut content, template, NOTES, take(&mut draft.notes));
                // The template has no documents: they stay as added fields.
                if let Some(section) = section_for_custom(template) {
                    for document in std::mem::take(&mut draft.documents) {
                        for (label, value) in [
                            ("Tipo de documento", &document.document_type),
                            ("Número de documento", &document.number),
                            ("País", &document.country),
                            ("Vencimiento", &document.expires),
                        ] {
                            if !value.is_empty() {
                                let mut custom = CustomValue::new(
                                    section.clone(),
                                    crate::i18n::text(label).into(),
                                    value.clone(),
                                );
                                custom.concealed = label == "Número de documento";
                                content.add_custom(custom);
                            }
                        }
                    }
                }
            }
            Self::Custom(draft) => {
                let section = section_for_custom(template);
                let mut kept = std::collections::HashSet::new();
                for record in std::mem::take(&mut draft.records) {
                    if let Some(field) = record.id.strip_prefix("field:") {
                        if template
                            .field(field)
                            .is_some_and(|field| !field.field_type.is_derived())
                        {
                            put(&mut content, template, field, record.value.clone());
                        }
                        continue;
                    }
                    if record.key.is_empty() && record.value.is_empty() {
                        continue;
                    }
                    let existing = record
                        .id
                        .strip_prefix("custom:")
                        .and_then(|id| uuid::Uuid::parse_str(id).ok())
                        .filter(|id| content.custom().iter().any(|custom| custom.id() == *id));
                    let id = match existing {
                        Some(id) => id,
                        None => {
                            let section = section
                                .clone()
                                .context("this template does not allow added fields")?;
                            let custom = CustomValue::new(
                                section,
                                SensitiveText::new(),
                                SensitiveText::new(),
                            );
                            let id = custom.id();
                            content.add_custom(custom);
                            id
                        }
                    };
                    kept.insert(id);
                    if let Some(custom) = content.custom_mut(id) {
                        custom.key = record.key.clone();
                        custom.value = record.value.clone();
                        custom.multiline = record.multiline;
                        custom.concealed = record.concealed;
                    }
                }
                let removed: Vec<_> = content
                    .custom()
                    .iter()
                    .map(CustomValue::id)
                    .filter(|id| !kept.contains(id))
                    .collect();
                for id in removed {
                    content.remove_custom(id);
                }
                put(&mut content, template, NOTES, take(&mut draft.notes));
            }
            Self::SshKey(draft, material) => {
                if let Some(material) = material.take() {
                    put(
                        &mut content,
                        template,
                        "private-key",
                        material.private_key.clone(),
                    );
                    put(
                        &mut content,
                        template,
                        "passphrase",
                        material.passphrase.clone(),
                    );
                }
                anyhow::ensure!(
                    !text(&content, "private-key").is_empty(),
                    "import an SSH key before saving"
                );
                put(&mut content, template, "user", take(&mut draft.user));
                put_list(
                    &mut content,
                    template,
                    "hosts",
                    std::mem::take(&mut draft.hosts),
                );
                put(&mut content, template, NOTES, take(&mut draft.notes));
            }
        }
        Ok(content)
    }
}

/// What an editor opens with, as QML's per-kind forms read it.
#[derive(Serialize)]
#[serde(tag = "kind", content = "fields")]
pub(super) enum EditorContent<'a> {
    #[serde(rename = "login")]
    Login(LoginEditor<'a>),
    #[serde(rename = "note")]
    Note(NoteEditor<'a>),
    #[serde(rename = "custom")]
    Custom(CustomEditor<'a>),
    #[serde(rename = "card")]
    Card(CardEditor<'a>),
    #[serde(rename = "identity")]
    Identity(IdentityEditor<'a>),
    #[serde(rename = "wifi")]
    Wifi(WifiEditor<'a>),
    #[serde(rename = "license")]
    License(LicenseEditor<'a>),
    #[serde(rename = "totp")]
    Totp(TotpEditor<'a>),
    #[serde(rename = "ssh-key")]
    SshKey(SshKeyEditor<'a>),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoginEditor<'a> {
    username: &'a str,
    password: &'a str,
    urls: &'a [SensitiveText],
    totp_input: &'a str,
    notes: &'a str,
}

#[derive(Serialize)]
pub(super) struct NoteEditor<'a> {
    value: &'a str,
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordEditor {
    id: String,
    key: SensitiveText,
    value: SensitiveText,
    multiline: bool,
    concealed: bool,
}

#[derive(Serialize)]
pub(super) struct CustomEditor<'a> {
    records: Vec<RecordEditor>,
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CardEditor<'a> {
    holder: &'a str,
    number: &'a str,
    expiry_month: u8,
    expiry_year: u16,
    cvv: &'a str,
    pin: &'a str,
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct IdentityEditor<'a> {
    first_name: &'a str,
    last_name: &'a str,
    documents: [(); 0],
    emails: &'a [SensitiveText],
    phones: &'a [SensitiveText],
    address: &'a str,
    notes: &'a str,
}

#[derive(Serialize)]
pub(super) struct WifiEditor<'a> {
    ssid: &'a str,
    security: &'a str,
    password: &'a str,
    hidden: bool,
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LicenseEditor<'a> {
    product: &'a str,
    license_key: &'a str,
    holder: &'a str,
    email: &'a str,
    version: &'a str,
    purchased: &'a str,
    expires: &'a str,
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TotpEditor<'a> {
    totp_input: &'a str,
    notes: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SshKeyEditor<'a> {
    import_token: &'static str,
    public_key: &'a str,
    fingerprint: &'a str,
    algorithm: String,
    user: &'a str,
    hosts: &'a [SensitiveText],
    notes: &'a str,
}

impl<'a> EditorContent<'a> {
    pub(super) fn new(content: &'a ItemContent) -> Result<Self> {
        let notes = text(content, NOTES);
        Ok(match kind_of(content.template().as_str()) {
            "login" => Self::Login(LoginEditor {
                username: text(content, "username"),
                password: text(content, "password"),
                urls: list(content, "urls"),
                totp_input: text(content, "totp"),
                notes,
            }),
            "note" => Self::Note(NoteEditor {
                value: text(content, "body"),
                notes: "",
            }),
            "card" => {
                let (year, month) = text(content, "expiry").split_once('-').unwrap_or(("", ""));
                Self::Card(CardEditor {
                    holder: text(content, "holder"),
                    number: text(content, "number"),
                    expiry_month: month.parse().unwrap_or(0),
                    expiry_year: year.parse().unwrap_or(0),
                    cvv: text(content, "cvv"),
                    pin: text(content, "pin"),
                    notes,
                })
            }
            "identity" => Self::Identity(IdentityEditor {
                first_name: text(content, "first-name"),
                last_name: text(content, "last-name"),
                documents: [],
                emails: list(content, "email"),
                phones: list(content, "phone"),
                address: text(content, "street"),
                notes,
            }),
            "wifi" => Self::Wifi(WifiEditor {
                ssid: text(content, "ssid"),
                security: match text(content, "security") {
                    "none" => "open",
                    "" => "wpa",
                    other => other,
                },
                password: text(content, "password"),
                hidden: text(content, "hidden") == "true",
                notes,
            }),
            "license" => Self::License(LicenseEditor {
                product: text(content, "product"),
                license_key: text(content, "license-key"),
                holder: text(content, "licensed-to"),
                email: text(content, "email"),
                version: text(content, "version"),
                purchased: text(content, "purchase-date"),
                expires: text(content, "expiry-date"),
                notes,
            }),
            "totp" => Self::Totp(TotpEditor {
                totp_input: text(content, "uri"),
                notes,
            }),
            "ssh-key" => {
                let algorithm = inspect_ssh_key(text(content, "private-key"))
                    .map_or_else(String::new, |info| info.algorithm);
                Self::SshKey(SshKeyEditor {
                    import_token: "",
                    public_key: text(content, "public-key"),
                    fingerprint: text(content, "fingerprint"),
                    algorithm,
                    user: text(content, "user"),
                    hosts: list(content, "hosts"),
                    notes,
                })
            }
            _ => Self::Custom(CustomEditor {
                records: records(content)?,
                notes,
            }),
        })
    }
}

/// One record per editable field of the template, then the added fields.
fn records(content: &ItemContent) -> Result<Vec<RecordEditor>> {
    let (_templates, template) = template_of(content.template().as_str())?;
    let library = crate::vault_access::templates()?;
    let language = crate::vault_access::language();
    let form = kryptopass_app::Form::build(library.library(), template, &language);
    let mut result = Vec::new();
    for field in form.sections.iter().flat_map(|section| &section.fields) {
        if field.id.as_str() == NOTES || field.derived || field.multiple.is_some() {
            continue;
        }
        result.push(RecordEditor {
            id: format!("field:{}", field.id.as_str()),
            key: field.label.clone().into(),
            value: text(content, field.id.as_str()).into(),
            multiline: false,
            concealed: field.concealed,
        });
    }
    result.extend(content.custom().iter().map(|custom| RecordEditor {
        id: format!("custom:{}", custom.id()),
        key: custom.key.clone(),
        value: custom.value.clone(),
        multiline: custom.multiline,
        concealed: custom.concealed,
    }));
    Ok(result)
}

/// What a detail view shows, as QML reads it.
#[derive(Serialize)]
#[serde(tag = "kind", content = "fields")]
pub(super) enum DetailContent<'a> {
    #[serde(rename = "login")]
    Login(Box<LoginDetail<'a>>),
    #[serde(rename = "note")]
    Note(NoteEditor<'a>),
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
    history: super::HistoryProjection,
}

impl<'a> DetailContent<'a> {
    pub(super) fn new(content: &'a ItemContent, view: &DetailView<'a>) -> Result<Self> {
        let expanded = || super::expanded::detail(content, view);
        Ok(match kind_of(content.template().as_str()) {
            "login" => {
                let username = match text(content, "username") {
                    "" => text(content, "email"),
                    username => username,
                };
                Self::Login(Box::new(LoginDetail {
                    username,
                    urls: list(content, "urls"),
                    notes: text(content, NOTES),
                    password: if view.reveal_password {
                        text(content, "password")
                    } else {
                        ""
                    },
                    password_revealed: view.reveal_password,
                    totp: parse_totp(text(content, "totp"))
                        .map(|config| totp_projection(&config, view.unix_seconds))
                        .transpose()?,
                    history: history_projection(view.versions),
                }))
            }
            "note" => Self::Note(NoteEditor {
                value: text(content, "body"),
                notes: "",
            }),
            "card" => Self::Card(expanded()?),
            "identity" => Self::Identity(expanded()?),
            "wifi" => Self::Wifi(expanded()?),
            "license" => Self::License(expanded()?),
            "totp" => Self::Totp(expanded()?),
            "ssh-key" => Self::SshKey(expanded()?),
            _ => Self::Custom(expanded()?),
        })
    }
}

/// The `otpauth://` text of a stored field, normalized for display and QR.
pub(crate) fn totp_uri(content: &ItemContent) -> Result<SensitiveText> {
    let field = if kind_of(content.template().as_str()) == "login" {
        "totp"
    } else {
        "uri"
    };
    let config = parse_totp(text(content, field)).context("no valid one-time code")?;
    format_totp_input(&config)
}
