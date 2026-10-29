//! Source entries to template content.
//!
//! Nothing the source holds is dropped: a value a template field rejects (a
//! date in another format, say) is kept as an added field under the field's
//! label, extra source fields become added fields, and an entry that still
//! does not fit its template is imported with the `custom` template, every
//! value an added field. Only an entry that does not fit even that is left
//! out, and counted.

use std::collections::HashMap;

use kp_template::{Library, Template};
use kryptopass_core::util::{clean_line, has_control_characters};
use kryptopass_core::vault::{ImportBatch, ImportFolder, ImportItem, TemplateRegistry};
use kryptopass_core::{
    CustomValue, FieldValue, IssueCode, IssueTarget, ItemContent, SensitiveText,
};
use kryptopass_import::{
    Entry, EntryContent, EntryField, EntryStatistics, EntryWarning, IdentityDocument,
    ImportedEntries, OneTimePassword, WifiSecurity,
};

use crate::error::AppError;

/// Bounds the store applies to titles, folder names and added-field keys.
const MAX_TITLE_BYTES: usize = 512;
const MAX_FOLDER_NAME_BYTES: usize = 256;
const MAX_CUSTOM_KEY_BYTES: usize = 512;

/// An export converted for review, ready for
/// [`Vault::import`](kryptopass_core::vault::Vault::import).
pub struct PreparedImport {
    pub batch: ImportBatch,
    /// What the source held, as the importer counted it.
    pub source: EntryStatistics,
    pub warnings: Vec<EntryWarning>,
    pub adaptations: Adaptations,
}

/// How the conversion had to bend the source, for the review screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adaptations {
    /// Aliases, which have no template, imported as logins with their email.
    pub aliases_as_logins: u32,
    /// Folder descriptions, imported as a note inside their folder.
    pub folder_descriptions: u32,
    /// Values a template field rejected, kept as added fields.
    pub values_as_custom: u32,
    /// Entries that did not fit their template, imported as custom items.
    pub entries_as_custom: u32,
    /// Entries that fit no template and were left out.
    pub rejected: u32,
}

/// Converts parsed entries to an import batch, with labels and fallback
/// titles in `language`.
pub fn prepare_import(
    entries: ImportedEntries,
    templates: &TemplateRegistry,
    language: &str,
) -> Result<PreparedImport, AppError> {
    let source = entries.statistics().clone();
    let (folders, entries, warnings) = entries.into_parts();
    let mut converter = Converter {
        library: templates.library(),
        language,
        adaptations: Adaptations::default(),
    };

    let indices: HashMap<_, _> = folders
        .iter()
        .enumerate()
        .map(|(index, folder)| (folder.id, index))
        .collect();
    let mut batch = ImportBatch::default();
    for (index, folder) in folders.into_iter().enumerate() {
        let name = clean_line(folder.name.as_str(), MAX_FOLDER_NAME_BYTES)
            .unwrap_or_else(|| (index + 1).to_string());
        if !folder.description.is_empty()
            && let Some(item) = converter.folder_description(&name, folder.description, index)
        {
            batch.items.push(item);
        }
        batch.folders.push(ImportFolder {
            name,
            parent: folder
                .parent
                .and_then(|parent| indices.get(&parent).copied()),
        });
    }
    for entry in entries {
        let folder = entry.folder.and_then(|id| indices.get(&id).copied());
        if let Some(mut item) = converter.entry(entry) {
            item.folder = folder;
            batch.items.push(item);
        }
    }

    let adaptations = converter.adaptations;
    tracing::info!(
        folders = batch.folders.len(),
        items = batch.items.len(),
        as_custom = adaptations.entries_as_custom,
        rejected = adaptations.rejected,
        "import prepared"
    );
    Ok(PreparedImport {
        batch,
        source,
        warnings,
        adaptations,
    })
}

struct Converter<'a> {
    library: &'a Library,
    language: &'a str,
    adaptations: Adaptations,
}

impl<'a> Converter<'a> {
    fn template(&self, id: &str) -> Option<&'a Template> {
        self.library.template(id)
    }

    fn label(&self, template: &str, field: &str) -> String {
        self.template(template)
            .and_then(|template| template.field(field))
            .and_then(|field| self.library.text(&field.label, self.language))
            .unwrap_or(field)
            .to_owned()
    }

    fn folder_description(
        &mut self,
        name: &str,
        description: SensitiveText,
        folder: usize,
    ) -> Option<ImportItem> {
        let mut fill = Fill::new(self.template("note")?);
        fill.one("body", description);
        let content = self.finish(fill)?;
        self.adaptations.folder_descriptions += 1;
        Some(ImportItem {
            folder: Some(folder),
            title: name.to_owned(),
            content,
            favorite: false,
            trashed: false,
            written_millis: None,
        })
    }

    fn entry(&mut self, entry: Entry) -> Option<ImportItem> {
        let Entry {
            title,
            content,
            favorite,
            trashed,
            created_at,
            modified_at,
            additional_fields,
            ..
        } = entry;
        let mut fill = self.fill(content)?;
        for field in additional_fields {
            fill.source_field(field);
        }
        let template = fill.template;
        let title = clean_line(title.as_str(), MAX_TITLE_BYTES).unwrap_or_else(|| {
            self.library
                .text(&template.meta.name, self.language)
                .unwrap_or(template.id.as_str())
                .to_owned()
        });
        let content = self.finish(fill)?;
        Some(ImportItem {
            folder: None,
            title,
            content,
            favorite,
            trashed,
            written_millis: modified_at
                .or(created_at)
                .map(|seconds| seconds.saturating_mul(1000)),
        })
    }

    /// The source values placed in the fields of their template.
    fn fill(&mut self, content: EntryContent) -> Option<Fill<'a>> {
        let fill = match content {
            EntryContent::Login(login) => {
                let mut fill = Fill::new(self.template("login")?);
                fill.one("username", login.username);
                fill.one("email", login.email);
                fill.one("password", login.password);
                if let Some(otp) = login.otp {
                    fill.one("totp", otp_value(otp));
                }
                fill.many("urls", login.urls);
                fill.one("notes", login.notes);
                fill
            }
            EntryContent::Note(note) => {
                let mut fill = Fill::new(self.template("note")?);
                fill.one("body", note.value);
                fill
            }
            EntryContent::Alias(alias) => {
                self.adaptations.aliases_as_logins += 1;
                let mut fill = Fill::new(self.template("login")?);
                fill.one("email", alias.email);
                fill.one("notes", alias.notes);
                fill
            }
            EntryContent::Card(card) => {
                let mut fill = Fill::new(self.template("card")?);
                fill.one("holder", card.holder);
                fill.one("number", without_separators(card.number));
                if (1..=12).contains(&card.expiry_month) && card.expiry_year > 0 {
                    let expiry = format!("{:04}-{:02}", card.expiry_year, card.expiry_month);
                    fill.one("expiry", SensitiveText::from(expiry));
                }
                fill.one("cvv", card.cvv);
                fill.one("pin", card.pin);
                fill.one("notes", card.notes);
                fill
            }
            EntryContent::Identity(identity) => {
                let labels = DocumentLabels {
                    number: self.label("passport", "document-number"),
                    country: self.label("passport", "country"),
                    expires: self.label("passport", "expiry-date"),
                };
                let mut fill = Fill::new(self.template("identity")?);
                fill.one("first-name", identity.first_name);
                fill.one("last-name", identity.last_name);
                fill.many("email", identity.emails);
                fill.many("phone", identity.phones);
                fill.one("street", identity.address);
                fill.one("notes", identity.notes);
                for document in identity.documents {
                    fill.document(document, &labels);
                }
                fill
            }
            EntryContent::Custom(custom) => {
                let mut fill = Fill::new(self.template("custom")?);
                for field in custom.fields {
                    fill.source_field(field);
                }
                fill.one("notes", custom.notes);
                fill
            }
            EntryContent::Wifi(wifi) => {
                let mut fill = Fill::new(self.template("wifi")?);
                fill.one("ssid", wifi.ssid);
                fill.one(
                    "security",
                    SensitiveText::from(match wifi.security {
                        WifiSecurity::Open => "none",
                        WifiSecurity::Wep => "wep",
                        WifiSecurity::Wpa => "wpa",
                        WifiSecurity::Wpa3 => "wpa3",
                    }),
                );
                fill.one("password", wifi.password);
                fill.one(
                    "hidden",
                    SensitiveText::from(if wifi.hidden { "true" } else { "false" }),
                );
                fill.one("notes", wifi.notes);
                fill
            }
            EntryContent::SshKey(key) => {
                let mut fill = Fill::new(self.template("ssh-key")?);
                // The algorithm is read from the key itself; it only says
                // something when the private key is missing.
                if key.private_key.is_empty() && !key.algorithm.is_empty() {
                    let label = SensitiveText::from(self.label("ssh-key", "public-key"));
                    fill.extra(label, key.algorithm, false, false);
                }
                fill.one("private-key", key.private_key);
                fill.one("public-key", key.public_key);
                fill.one("fingerprint", key.fingerprint);
                fill.one("user", key.user);
                fill.one("notes", key.notes);
                fill
            }
            EntryContent::Totp(totp) => {
                let mut fill = Fill::new(self.template("totp")?);
                fill.one("uri", otp_value(totp.secret));
                fill.one("notes", totp.notes);
                fill
            }
            EntryContent::License(license) => {
                let mut fill = Fill::new(self.template("license")?);
                fill.one("product", license.product);
                fill.one("version", license.version);
                fill.one("license-key", license.license_key);
                fill.one("licensed-to", license.holder);
                fill.one("email", license.email);
                fill.one("purchase-date", license.purchased);
                fill.one("expiry-date", license.expires);
                fill.one("notes", license.notes);
                fill
            }
        };
        Some(fill)
    }

    /// Valid content from `fill`: rejected values become added fields, and
    /// content that still does not fit becomes a custom item.
    fn finish(&mut self, fill: Fill<'a>) -> Option<ItemContent> {
        let template = fill.template;
        let mut content = fill.into_content(self.library, self.language);
        content.normalize(template);
        let moved = self.keep_rejected_as_custom(&mut content, template);
        if content.validate(template).is_ok() {
            self.adaptations.values_as_custom += moved;
            return Some(content);
        }
        if let Some(custom) = self.as_custom(&content, template) {
            self.adaptations.entries_as_custom += 1;
            return Some(custom);
        }
        self.adaptations.rejected += 1;
        tracing::debug!(template = template.id.as_str(), "import entry rejected");
        None
    }

    /// Moves every field value the template rejects into an added field,
    /// when the template has a section for them. Returns how many moved.
    fn keep_rejected_as_custom(&self, content: &mut ItemContent, template: &Template) -> u32 {
        let Some(section) = custom_section(template) else {
            return 0;
        };
        let Err(issues) = content.validate(template) else {
            return 0;
        };
        let mut moved = 0;
        for issue in issues.0 {
            let IssueTarget::Field(id) = issue.target else {
                continue;
            };
            if issue.code == IssueCode::REQUIRED {
                continue;
            }
            let (Some(field), Some(value)) = (
                template.field(id.as_str()),
                content.value(id.as_str()).map(values_of),
            ) else {
                continue;
            };
            content.remove(id.as_str());
            let label = self
                .library
                .text(&field.label, self.language)
                .unwrap_or(id.as_str());
            let concealed = field.conceal || field.field_type.is_concealed();
            let multiline = field.field_type.is_multiline();
            for value in value {
                let mut custom =
                    CustomValue::new(section.clone(), SensitiveText::from(label), value);
                custom.concealed = concealed;
                custom.multiline = multiline;
                content.add_custom(custom);
            }
            moved += 1;
        }
        moved
    }

    /// The same values as a custom item, every one an added field.
    fn as_custom(&self, content: &ItemContent, from: &Template) -> Option<ItemContent> {
        let template = self.template("custom")?;
        let section = custom_section(template)?;
        let mut custom = ItemContent::new(template);
        for value in content.values() {
            let field = from.field(value.id().as_str());
            let label = field
                .and_then(|field| self.library.text(&field.label, self.language))
                .unwrap_or(value.id().as_str());
            let concealed =
                field.is_some_and(|field| field.conceal || field.field_type.is_concealed());
            let multiline = field.is_some_and(|field| field.field_type.is_multiline());
            for item in values_of(value.value()) {
                let mut added = CustomValue::new(section.clone(), SensitiveText::from(label), item);
                added.concealed = concealed;
                added.multiline = multiline;
                custom.add_custom(added);
            }
        }
        for added in content.custom() {
            let mut added = added.clone();
            added.section = section.clone();
            custom.add_custom(added);
        }
        custom.normalize(template);
        custom.validate(template).is_ok().then_some(custom)
    }
}

/// Labels for identity documents, borrowed from the passport template.
struct DocumentLabels {
    number: String,
    country: String,
    expires: String,
}

/// Content being filled: template fields, plus values for added fields.
struct Fill<'t> {
    template: &'t Template,
    content: ItemContent,
    extra: Vec<Extra>,
}

struct Extra {
    key: SensitiveText,
    value: SensitiveText,
    concealed: bool,
    multiline: bool,
}

impl<'t> Fill<'t> {
    fn new(template: &'t Template) -> Self {
        Self {
            template,
            content: ItemContent::new(template),
            extra: Vec::new(),
        }
    }

    fn one(&mut self, field: &str, value: SensitiveText) {
        if value.is_empty() {
            return;
        }
        match self.template.field(field) {
            Some(field) => self.content.set(field.id.clone(), FieldValue::One(value)),
            None => self.extra(SensitiveText::from(field), value, false, false),
        }
    }

    fn many(&mut self, field: &str, values: Vec<SensitiveText>) {
        let values: Vec<_> = values
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect();
        if values.is_empty() {
            return;
        }
        match self.template.field(field) {
            Some(field) => self.content.set(field.id.clone(), FieldValue::Many(values)),
            None => {
                for value in values {
                    self.extra(SensitiveText::from(field), value, false, false);
                }
            }
        }
    }

    fn extra(
        &mut self,
        key: SensitiveText,
        value: SensitiveText,
        concealed: bool,
        multiline: bool,
    ) {
        if value.is_empty() {
            return;
        }
        self.extra.push(Extra {
            key,
            value,
            concealed,
            multiline,
        });
    }

    fn source_field(&mut self, field: EntryField) {
        let EntryField {
            label,
            value,
            concealed,
            multiline,
        } = field;
        self.extra(label, value, concealed, multiline);
    }

    fn document(&mut self, document: IdentityDocument, labels: &DocumentLabels) {
        let IdentityDocument {
            document_type,
            number,
            country,
            expires,
        } = document;
        let key = if document_type.is_empty() {
            SensitiveText::from(labels.number.as_str())
        } else {
            document_type
        };
        self.extra(key, number, true, false);
        self.extra(
            SensitiveText::from(labels.country.as_str()),
            country,
            false,
            false,
        );
        self.extra(
            SensitiveText::from(labels.expires.as_str()),
            expires,
            false,
            false,
        );
    }

    /// The content with the extra values as added fields, or appended to the
    /// first multiline field when the template takes no added fields.
    fn into_content(self, library: &Library, language: &str) -> ItemContent {
        let Self {
            template,
            mut content,
            extra,
        } = self;
        if extra.is_empty() {
            return content;
        }
        if let Some(section) = custom_section(template) {
            let fallback = library.section_title(section.as_str(), language);
            for Extra {
                key,
                value,
                concealed,
                multiline,
            } in extra
            {
                let key = if valid_key(&key) {
                    key
                } else {
                    SensitiveText::from(fallback)
                };
                let mut custom = CustomValue::new(section.clone(), key, value);
                custom.concealed = concealed;
                custom.multiline = multiline || value_is_multiline(&custom.value);
                content.add_custom(custom);
            }
            return content;
        }
        let Some(field) = template
            .fields()
            .find(|field| field.field_type.is_multiline() && !field.field_type.is_concealed())
        else {
            return content;
        };
        let mut text = match content.value(field.id.as_str()) {
            Some(FieldValue::One(text)) => text.clone(),
            _ => SensitiveText::new(),
        };
        for Extra { key, value, .. } in extra {
            if !text.is_empty() {
                text.push_str("\n");
            }
            text.push_str(key.as_str());
            text.push_str(": ");
            text.push_str(value.as_str());
        }
        content.set(field.id.clone(), FieldValue::One(text));
        content
    }
}

/// The first section that takes added fields.
fn custom_section(template: &Template) -> Option<kp_template::SectionId> {
    template
        .sections
        .iter()
        .find(|section| section.custom_fields.is_some())
        .map(|section| section.id.clone())
}

fn valid_key(key: &SensitiveText) -> bool {
    !key.is_empty()
        && key.len() <= MAX_CUSTOM_KEY_BYTES
        && !has_control_characters(key.as_str(), false)
}

fn value_is_multiline(value: &SensitiveText) -> bool {
    value.as_str().contains('\n')
}

fn values_of(value: &FieldValue) -> Vec<SensitiveText> {
    match value {
        FieldValue::One(value) => vec![value.clone()],
        FieldValue::Many(values) => values.clone(),
    }
}

/// A URI or a loose Base32 secret; the `totp` field converts the latter
/// when it is normalized.
fn otp_value(otp: OneTimePassword) -> SensitiveText {
    match otp {
        OneTimePassword::Uri(uri) => uri,
        OneTimePassword::Base32(secret) => secret,
    }
}

/// A card number with spaces and dashes removed; anything else is left for
/// validation to judge.
fn without_separators(number: SensitiveText) -> SensitiveText {
    if !number.as_str().contains([' ', '-']) {
        return number;
    }
    let mut digits = SensitiveText::with_capacity(number.len());
    let mut buffer = [0_u8; 4];
    for character in number.as_str().chars().filter(|c| !matches!(c, ' ' | '-')) {
        digits.push_str(character.encode_utf8(&mut buffer));
    }
    digits
}
