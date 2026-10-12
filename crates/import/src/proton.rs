//! Proton Pass JSON translation. Provider wire types remain private here.

use crate::{
    ExportImporter, ImportError, ImportIssue, ImportPlan, ImportSummary, IssueKind,
    MAX_EXPORT_BYTES, MAX_RECORDS,
};
use kryptopass_core::{
    CardContent, CustomContent, CustomRecord, IdentityContent, IdentityDocument, ImportBatch,
    ImportFolder, ImportItem, ImportValidationError, LoginContent, NoteContent, SecretContent,
    SshKeyContent, WifiContent, WifiSecurity, parse_totp_input,
};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::collections::{BTreeMap, HashMap, HashSet};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[derive(Default)]
pub struct ProtonPassImporter;

impl ExportImporter for ProtonPassImporter {
    fn parse(&self, json: &[u8]) -> Result<ImportPlan, ImportError> {
        if json.len() > MAX_EXPORT_BYTES {
            return Err(ImportError::Limit);
        }
        let export: Export =
            serde_json::from_slice(json).map_err(|_| ImportError::InvalidExport)?;
        if export.version.is_empty() {
            return Err(ImportError::Unsupported);
        }
        let mut plan = ImportPlan {
            batch: ImportBatch::default(),
            issues: vec![],
            summary: ImportSummary::default(),
        };
        let mut seen_items = HashSet::new();
        for (vault_index, (vault_id, vault)) in export.vaults.iter().enumerate() {
            if plan.batch.folders.len()
                + plan.summary.source_items
                + vault.folders.len()
                + vault.items.len()
                + 1
                > MAX_RECORDS
            {
                return Err(ImportError::Limit);
            }
            let root = plan.batch.folders.len();
            plan.batch.folders.push(ImportFolder {
                name: vault.name.clone(),
                parent: None,
            });
            let mut folder_indices = HashMap::new();
            for folder in &vault.folders {
                let index = plan.batch.folders.len();
                if folder.folder_id.is_empty()
                    || folder_indices
                        .insert(folder.folder_id.as_str(), index)
                        .is_some()
                {
                    return Err(ImportError::Hierarchy);
                }
                plan.batch.folders.push(ImportFolder {
                    name: folder.name.clone(),
                    parent: None,
                });
            }
            for folder in &vault.folders {
                let parent = match folder
                    .parent_folder_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                {
                    Some(id) => *folder_indices.get(id).ok_or(ImportError::Hierarchy)?,
                    None => root,
                };
                plan.batch.folders[folder_indices[folder.folder_id.as_str()]].parent = Some(parent);
            }
            if !vault.description.is_empty() {
                issue(
                    &mut plan,
                    vault_index,
                    None,
                    IssueKind::VaultDescriptionNotImported,
                );
            }
            for (item_index, item) in vault.items.iter().enumerate() {
                plan.summary.source_items += 1;
                if item.item_id.is_empty()
                    || !seen_items.insert((vault_id.as_str(), item.item_id.as_str()))
                {
                    return Err(ImportError::Hierarchy);
                }
                if !item.share_id.is_empty() && &item.share_id != vault_id {
                    return Err(ImportError::Hierarchy);
                }
                if !matches!(item.state, 1 | 2) {
                    return Err(ImportError::Unsupported);
                }
                let folder = match item.folder_id.as_deref().filter(|id| !id.is_empty()) {
                    Some(id) => *folder_indices.get(id).ok_or(ImportError::Hierarchy)?,
                    None => root,
                };
                let mut notes = Zeroizing::new(item.data.metadata.note.clone());
                let mut native_other = false;
                if item.create_time != 0 || item.modify_time != 0 {
                    issue(
                        &mut plan,
                        vault_index,
                        Some(item_index),
                        IssueKind::TimestampsNotImported,
                    );
                }
                if !item.files.is_empty() {
                    issue(
                        &mut plan,
                        vault_index,
                        Some(item_index),
                        IssueKind::AttachmentsNotImported,
                    );
                }
                let mut content = match item.data.kind.as_str() {
                    "login" => {
                        let login: Login = serde_json::from_str(&item.data.content.0)
                            .map_err(|_| ImportError::InvalidExport)?;
                        if !login.extra_fields.is_empty() {
                            issue(
                                &mut plan,
                                vault_index,
                                Some(item_index),
                                IssueKind::UnsupportedItem,
                            );
                            plan.summary.skipped += 1;
                            continue;
                        }
                        let username = if !login.item_username.is_empty() {
                            &login.item_username
                        } else if !login.item_email.is_empty() {
                            &login.item_email
                        } else {
                            &login.username
                        };
                        if !login.item_username.is_empty()
                            && !login.item_email.is_empty()
                            && login.item_username != login.item_email
                        {
                            notes.push_str("\n\nProton Pass email: ");
                            notes.push_str(&login.item_email);
                        }
                        preserve(
                            &mut notes,
                            &login.passkeys,
                            "Proton Pass passkeys (reference only)",
                            &mut plan,
                            vault_index,
                            item_index,
                            IssueKind::PasskeysInNotes,
                        );
                        preserve(
                            &mut notes,
                            &login.autofill_urls,
                            "Proton Pass autofill rules (reference only)",
                            &mut plan,
                            vault_index,
                            item_index,
                            IssueKind::AutofillRulesInNotes,
                        );
                        let totp =
                            parse_totp_input(&login.totp_uri).map_err(|_| ImportError::Content)?;
                        plan.summary.logins += 1;
                        SecretContent::Login(LoginContent {
                            username: username.clone(),
                            password: login.password.clone(),
                            url: login.urls.first().cloned().unwrap_or_default(),
                            urls: login.urls.clone(),
                            notes: std::mem::take(&mut *notes),
                            totp,
                            history: Box::default(),
                        })
                    }
                    "note" => {
                        plan.summary.notes += 1;
                        SecretContent::Note(NoteContent {
                            value: std::mem::take(&mut *notes),
                            notes: String::new(),
                        })
                    }
                    "alias" => {
                        issue(
                            &mut plan,
                            vault_index,
                            Some(item_index),
                            IssueKind::AliasAsNote,
                        );
                        notes.push_str("\n\nProton Pass alias: ");
                        notes.push_str(item.alias_email.as_deref().unwrap_or_default());
                        plan.summary.notes += 1;
                        SecretContent::Note(NoteContent {
                            value: std::mem::take(&mut *notes),
                            notes: String::new(),
                        })
                    }
                    "creditCard" => {
                        match serde_json::from_str::<ProtonCard>(&item.data.content.0) {
                            Ok(card)
                                if !card.extra_fields.is_empty()
                                    || item.data.extra_fields.has_value()
                                    || (card.card_type.has_value()
                                        && !is_scalar_json(&card.card_type)) =>
                            {
                                match source_reference_content(
                                    &item.data.metadata.note,
                                    &item.data.content,
                                    "creditCard",
                                    &mut plan,
                                    vault_index,
                                    item_index,
                                ) {
                                    Some(content) => {
                                        native_other = true;
                                        content
                                    }
                                    None => continue,
                                }
                            }
                            Ok(card) => match parse_expiration(&card.expiration_date) {
                                Some((expiry_month, expiry_year)) => {
                                    if card.card_type.has_value() {
                                        preserve_card_type(
                                            &mut notes,
                                            &card.card_type,
                                            &mut plan,
                                            vault_index,
                                            item_index,
                                        );
                                    }
                                    native_other = true;
                                    SecretContent::Card(CardContent {
                                        holder: card.cardholder_name.clone(),
                                        number: card.number.clone(),
                                        expiry_month,
                                        expiry_year,
                                        cvv: card.verification_number.clone(),
                                        pin: card.pin.clone(),
                                        notes: std::mem::take(&mut *notes),
                                    })
                                }
                                None => match source_reference_content(
                                    &item.data.metadata.note,
                                    &item.data.content,
                                    "creditCard",
                                    &mut plan,
                                    vault_index,
                                    item_index,
                                ) {
                                    Some(content) => {
                                        native_other = true;
                                        content
                                    }
                                    None => continue,
                                },
                            },
                            Err(_) => match source_reference_content(
                                &item.data.metadata.note,
                                &item.data.content,
                                "creditCard",
                                &mut plan,
                                vault_index,
                                item_index,
                            ) {
                                Some(content) => {
                                    native_other = true;
                                    content
                                }
                                None => continue,
                            },
                        }
                    }
                    "identity" => {
                        match serde_json::from_str::<ProtonIdentity>(&item.data.content.0) {
                            Ok(identity)
                                if !identity.extra_fields.is_empty()
                                    || item.data.extra_fields.has_value() =>
                            {
                                match source_reference_content(
                                    &item.data.metadata.note,
                                    &item.data.content,
                                    "identity",
                                    &mut plan,
                                    vault_index,
                                    item_index,
                                ) {
                                    Some(content) => {
                                        native_other = true;
                                        content
                                    }
                                    None => continue,
                                }
                            }
                            Ok(identity) => {
                                native_other = true;
                                SecretContent::Identity(
                                    identity.into_content(std::mem::take(&mut *notes)),
                                )
                            }
                            Err(_) => match source_reference_content(
                                &item.data.metadata.note,
                                &item.data.content,
                                "identity",
                                &mut plan,
                                vault_index,
                                item_index,
                            ) {
                                Some(content) => {
                                    native_other = true;
                                    content
                                }
                                None => continue,
                            },
                        }
                    }
                    "custom" => match serde_json::from_str::<ProtonCustom>(&item.data.content.0) {
                        Ok(custom) => {
                            native_other = true;
                            SecretContent::Custom(custom.into_content(std::mem::take(&mut *notes)))
                        }
                        Err(_) => match source_reference_content(
                            &item.data.metadata.note,
                            &item.data.content,
                            "custom",
                            &mut plan,
                            vault_index,
                            item_index,
                        ) {
                            Some(content) => {
                                native_other = true;
                                content
                            }
                            None => continue,
                        },
                    },
                    "wifi" => match serde_json::from_str::<ProtonWifi>(&item.data.content.0) {
                        Ok(wifi)
                            if wifi.has_unmapped_fields() || item.data.extra_fields.has_value() =>
                        {
                            match source_reference_content(
                                &item.data.metadata.note,
                                &item.data.content,
                                "wifi",
                                &mut plan,
                                vault_index,
                                item_index,
                            ) {
                                Some(content) => {
                                    native_other = true;
                                    content
                                }
                                None => continue,
                            }
                        }
                        Ok(wifi) => match wifi.security() {
                            Some(security) => {
                                native_other = true;
                                SecretContent::Wifi(WifiContent {
                                    ssid: wifi.ssid.clone(),
                                    security,
                                    password: wifi.password.clone(),
                                    hidden: false,
                                    notes: std::mem::take(&mut *notes),
                                })
                            }
                            None => match source_reference_content(
                                &item.data.metadata.note,
                                &item.data.content,
                                "wifi",
                                &mut plan,
                                vault_index,
                                item_index,
                            ) {
                                Some(content) => {
                                    native_other = true;
                                    content
                                }
                                None => continue,
                            },
                        },
                        Err(_) => match source_reference_content(
                            &item.data.metadata.note,
                            &item.data.content,
                            "wifi",
                            &mut plan,
                            vault_index,
                            item_index,
                        ) {
                            Some(content) => {
                                native_other = true;
                                content
                            }
                            None => continue,
                        },
                    },
                    "sshKey" => match serde_json::from_str::<ProtonSshKey>(&item.data.content.0) {
                        Ok(mut ssh) => {
                            if ssh.has_unmapped_fields() || item.data.extra_fields.has_value() {
                                issue(
                                    &mut plan,
                                    vault_index,
                                    Some(item_index),
                                    IssueKind::UnsupportedItem,
                                );
                                plan.summary.skipped += 1;
                                continue;
                            }
                            let source_public_key = std::mem::take(&mut ssh.public_key);
                            let private_key = std::mem::take(&mut ssh.private_key);
                            match SshKeyContent::import(private_key, String::new()) {
                                Ok(mut key) => {
                                    if !source_public_key.is_empty()
                                        && source_public_key != key.public_key
                                    {
                                        if is_private_key_material(&source_public_key) {
                                            issue(
                                                &mut plan,
                                                vault_index,
                                                Some(item_index),
                                                IssueKind::UnsupportedItem,
                                            );
                                            plan.summary.skipped += 1;
                                            continue;
                                        }
                                        preserve_ssh_public_reference(
                                            &mut notes,
                                            &source_public_key,
                                            &mut plan,
                                            vault_index,
                                            item_index,
                                        );
                                    }
                                    key.notes = std::mem::take(&mut *notes);
                                    native_other = true;
                                    SecretContent::SshKey(key)
                                }
                                Err(_) => {
                                    issue(
                                        &mut plan,
                                        vault_index,
                                        Some(item_index),
                                        IssueKind::UnsupportedItem,
                                    );
                                    plan.summary.skipped += 1;
                                    continue;
                                }
                            }
                        }
                        Err(_) => {
                            issue(
                                &mut plan,
                                vault_index,
                                Some(item_index),
                                IssueKind::UnsupportedItem,
                            );
                            plan.summary.skipped += 1;
                            continue;
                        }
                    },
                    _ => {
                        issue(
                            &mut plan,
                            vault_index,
                            Some(item_index),
                            IssueKind::UnsupportedItem,
                        );
                        plan.summary.skipped += 1;
                        continue;
                    }
                };
                if let SecretContent::Custom(custom) = &mut content {
                    if append_custom_source_records(
                        &mut custom.records,
                        "item data",
                        &item.data.extra_fields,
                    ) {
                        issue_once(
                            &mut plan,
                            vault_index,
                            Some(item_index),
                            IssueKind::SourceFieldsPreserved,
                        );
                    }
                } else {
                    let note = match &mut content {
                        SecretContent::Login(login) => &mut login.notes,
                        SecretContent::Note(note) => &mut note.value,
                        SecretContent::Card(value) => &mut value.notes,
                        SecretContent::Totp(value) => &mut value.notes,
                        SecretContent::Identity(value) => &mut value.notes,
                        SecretContent::Wifi(value) => &mut value.notes,
                        SecretContent::License(value) => &mut value.notes,
                        SecretContent::SshKey(value) => &mut value.notes,
                        SecretContent::Custom(_) => unreachable!(),
                    };
                    preserve(
                        note,
                        &item.data.extra_fields,
                        "Proton Pass custom fields (reference only)",
                        &mut plan,
                        vault_index,
                        item_index,
                        IssueKind::ExtraFieldsInNotes,
                    );
                }
                if native_other && content.validate().is_err() {
                    issue(
                        &mut plan,
                        vault_index,
                        Some(item_index),
                        IssueKind::UnsupportedItem,
                    );
                    plan.summary.skipped += 1;
                    continue;
                }
                if native_other {
                    plan.summary.other_items += 1;
                }
                plan.summary.trashed += usize::from(item.state == 2);
                plan.batch.items.push(ImportItem {
                    title: item.data.metadata.name.clone(),
                    content,
                    folder,
                    favorite: item.pinned,
                    trashed: item.state == 2,
                });
            }
            plan.summary.vaults += 1;
        }
        plan.summary.folders = plan.batch.folders.len();
        plan.summary.warnings = plan.issues.len();
        plan.batch.validate().map_err(|error| match error {
            ImportValidationError::Limit => ImportError::Limit,
            ImportValidationError::Hierarchy => ImportError::Hierarchy,
            ImportValidationError::Content => ImportError::Content,
        })?;
        Ok(plan)
    }
}

fn issue(plan: &mut ImportPlan, vault: usize, item: Option<usize>, kind: IssueKind) {
    plan.issues.push(ImportIssue { vault, item, kind });
}

fn issue_once(plan: &mut ImportPlan, vault: usize, item: Option<usize>, kind: IssueKind) {
    if !plan
        .issues
        .iter()
        .any(|issue| issue.vault == vault && issue.item == item && issue.kind == kind)
    {
        issue(plan, vault, item, kind);
    }
}

fn preserve(
    notes: &mut String,
    raw: &SecretJson,
    label: &str,
    plan: &mut ImportPlan,
    vault: usize,
    item: usize,
    kind: IssueKind,
) {
    if !matches!(raw.0.trim(), "" | "[]" | "{}" | "null") {
        notes.push_str("\n\n");
        notes.push_str(label);
        notes.push_str(":\n");
        notes.push_str(&raw.0);
        issue(plan, vault, Some(item), kind);
    }
}

fn source_reference_content(
    notes: &str,
    raw: &SecretJson,
    source_kind: &str,
    plan: &mut ImportPlan,
    vault: usize,
    item: usize,
) -> Option<SecretContent> {
    let value = raw.0.clone();
    let content = SecretContent::Custom(CustomContent {
        records: vec![CustomRecord {
            id: record_id(1),
            key: format!("{source_kind} source"),
            multiline: value.contains(['\n', '\r']),
            value,
            concealed: true,
        }],
        notes: notes.to_owned(),
    });
    if content.validate().is_err() {
        issue(plan, vault, Some(item), IssueKind::UnsupportedItem);
        plan.summary.skipped += 1;
        return None;
    }
    issue_once(plan, vault, Some(item), IssueKind::SourceFieldsPreserved);
    Some(content)
}

fn preserve_card_type(
    notes: &mut Zeroizing<String>,
    card_type: &SecretJson,
    plan: &mut ImportPlan,
    vault: usize,
    item: usize,
) {
    notes.push_str("\n\nProton Pass cardType: ");
    notes.push_str(&card_type.0);
    issue(plan, vault, Some(item), IssueKind::ExtraFieldsInNotes);
}

fn is_scalar_json(raw: &SecretJson) -> bool {
    serde_json::from_str::<serde_json::Value>(&raw.0)
        .map(|value| value.is_string() || value.is_number() || value.is_boolean())
        .unwrap_or(false)
}

fn is_private_key_material(value: &str) -> bool {
    value.contains("PRIVATE KEY") || value.contains("openssh-key-v1")
}

fn preserve_ssh_public_reference(
    notes: &mut Zeroizing<String>,
    public_key: &str,
    plan: &mut ImportPlan,
    vault: usize,
    item: usize,
) {
    notes.push_str(
        "\n\nProton Pass SSH publicKey differed from the derived public key (reference only):\n",
    );
    notes.push_str(public_key);
    issue(plan, vault, Some(item), IssueKind::ExtraFieldsInNotes);
}

fn parse_expiration(value: &str) -> Option<(u8, u16)> {
    let (year, month) = value.split_once('-')?;
    if year.len() != 4 || month.len() != 2 {
        return None;
    }
    let year = year.parse::<u16>().ok()?;
    let month = month.parse::<u8>().ok()?;
    ((1..=12).contains(&month) && (1900..=9999).contains(&year)).then_some((month, year))
}

fn record_id(index: usize) -> String {
    format!("00000000-0000-4000-8000-{index:012x}")
}

fn key_for_field(section: &str, field: &str) -> String {
    match (section.is_empty(), field.is_empty()) {
        (false, false) => format!("{section} / {field}"),
        (false, true) => section.to_owned(),
        (true, false) => field.to_owned(),
        (true, true) => String::new(),
    }
}

fn push_custom_record(
    records: &mut Vec<CustomRecord>,
    key: String,
    value: String,
    concealed: bool,
) {
    let multiline = value.contains(['\n', '\r']);
    records.push(CustomRecord {
        id: record_id(records.len() + 1),
        key,
        value,
        multiline,
        concealed,
    });
}

fn append_custom_extra_records(
    records: &mut Vec<CustomRecord>,
    prefix: &str,
    fields: &SecretFields,
) -> bool {
    let start = records.len();
    for (name, raw) in fields.iter() {
        push_custom_record(records, format!("{prefix} / {name}"), raw.0.clone(), true);
    }
    records.len() > start
}

fn append_custom_source_records(
    records: &mut Vec<CustomRecord>,
    prefix: &str,
    raw: &SecretJson,
) -> bool {
    if !raw.has_value() {
        return false;
    }
    if let Ok(fields) = serde_json::from_str::<SecretFields>(&raw.0) {
        if append_custom_extra_records(records, prefix, &fields) {
            return true;
        }
    }
    push_custom_record(records, format!("{prefix} source"), raw.0.clone(), true);
    true
}

#[derive(Default, Zeroize, ZeroizeOnDrop)]
struct SecretJson(String);
impl<'de> Deserialize<'de> for SecretJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        Ok(Self(Box::<str>::from(raw).into()))
    }
}

#[derive(Deserialize)]
struct Export {
    version: String,
    vaults: BTreeMap<String, Vault>,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
struct Vault {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    folders: Vec<SourceFolder>,
    items: Vec<Item>,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct SourceFolder {
    folder_id: String,
    #[serde(default)]
    parent_folder_id: Option<String>,
    name: String,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct Item {
    item_id: String,
    #[serde(default)]
    share_id: String,
    #[serde(default)]
    folder_id: Option<String>,
    data: ItemData,
    state: u32,
    #[serde(default)]
    alias_email: Option<String>,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    create_time: u64,
    #[serde(default)]
    modify_time: u64,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ItemData {
    metadata: Metadata,
    #[serde(rename = "type")]
    kind: String,
    content: SecretJson,
    #[serde(default)]
    extra_fields: SecretJson,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
struct Metadata {
    name: String,
    #[serde(default)]
    note: String,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct Login {
    #[serde(default)]
    item_username: String,
    #[serde(default)]
    item_email: String,
    #[serde(default)]
    username: String,
    password: String,
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    totp_uri: String,
    #[serde(default)]
    passkeys: SecretJson,
    #[serde(default)]
    autofill_urls: SecretJson,
    #[serde(default, flatten)]
    extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ProtonCard {
    #[serde(default)]
    cardholder_name: String,
    #[serde(default)]
    number: String,
    #[serde(default)]
    verification_number: String,
    #[serde(default)]
    expiration_date: String,
    #[serde(default)]
    pin: String,
    #[serde(default)]
    card_type: SecretJson,
    #[serde(default, flatten)]
    extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ProtonIdentity {
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    phone_number: String,
    #[serde(default)]
    street_address: String,
    #[serde(default)]
    zip_or_postal_code: String,
    #[serde(default)]
    city: String,
    #[serde(default)]
    state_or_province: String,
    #[serde(default)]
    country_or_region: String,
    #[serde(default)]
    social_security_number: String,
    #[serde(default)]
    passport_number: String,
    #[serde(default)]
    license_number: String,
    #[serde(default, flatten)]
    extra_fields: SecretFields,
}

impl ProtonIdentity {
    fn into_content(mut self, notes: String) -> IdentityContent {
        let mut address = Zeroizing::new(String::new());
        for (label, value) in [
            ("streetAddress", self.street_address.as_str()),
            ("city", self.city.as_str()),
            ("stateOrProvince", self.state_or_province.as_str()),
            ("zipOrPostalCode", self.zip_or_postal_code.as_str()),
            ("countryOrRegion", self.country_or_region.as_str()),
        ] {
            if !value.is_empty() {
                if !address.is_empty() {
                    address.push('\n');
                }
                address.push_str(label);
                address.push_str(": ");
                address.push_str(value);
            }
        }
        let country = self.country_or_region.clone();
        let mut documents = Vec::new();
        for (document_type, number) in [
            (
                "socialSecurityNumber",
                std::mem::take(&mut self.social_security_number),
            ),
            ("passportNumber", std::mem::take(&mut self.passport_number)),
            ("licenseNumber", std::mem::take(&mut self.license_number)),
        ] {
            if !number.is_empty() {
                documents.push(IdentityDocument {
                    id: record_id(documents.len() + 1),
                    document_type: document_type.to_owned(),
                    number,
                    country: country.clone(),
                    expires: String::new(),
                });
            }
        }
        let email = std::mem::take(&mut self.email);
        let phone_number = std::mem::take(&mut self.phone_number);
        let emails = (!email.is_empty()).then_some(email).into_iter().collect();
        let phones = (!phone_number.is_empty())
            .then_some(phone_number)
            .into_iter()
            .collect();
        IdentityContent {
            first_name: std::mem::take(&mut self.first_name),
            last_name: std::mem::take(&mut self.last_name),
            documents,
            emails,
            phones,
            address: std::mem::take(&mut *address),
            notes,
        }
    }
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ProtonCustom {
    #[serde(default)]
    sections: Vec<ProtonCustomSection>,
    #[serde(default, flatten)]
    extra_fields: SecretFields,
}

impl ProtonCustom {
    fn into_content(mut self, notes: String) -> CustomContent {
        let mut records = Vec::new();
        append_custom_extra_records(&mut records, "Custom", &self.extra_fields);
        for mut section in std::mem::take(&mut self.sections) {
            let section_name = std::mem::take(&mut section.section_name);
            let section_key = if section_name.is_empty() {
                "Section".to_owned()
            } else {
                section_name.clone()
            };
            append_custom_extra_records(&mut records, &section_key, &section.extra_fields);
            for mut field in std::mem::take(&mut section.section_fields) {
                let mut field_data = serde_json::from_str::<ProtonCustomFieldData>(&field.data.0);
                let kind = std::mem::take(&mut field.kind);
                let mut field_key = key_for_field(&section_name, &field.field_name);
                if field_key.is_empty() {
                    field_key = format!("Field {}", records.len() + 1);
                }
                let mut key = Zeroizing::new(field_key.clone());
                let preserve_data_extras = field_data.is_ok()
                    && matches!(kind.as_str(), "text" | "hidden" | "totp" | "timestamp");
                let (value, concealed) = match &mut field_data {
                    Ok(data) if kind == "text" => (std::mem::take(&mut data.content), false),
                    Ok(data) if kind == "hidden" => (std::mem::take(&mut data.content), true),
                    Ok(data) if kind == "totp" && !data.totp_uri.is_empty() => {
                        (std::mem::take(&mut data.totp_uri), true)
                    }
                    Ok(data) if kind == "timestamp" && data.timestamp.has_value() => {
                        (std::mem::take(&mut data.timestamp.0), true)
                    }
                    _ => (std::mem::take(&mut field.data.0), true),
                };
                push_custom_record(&mut records, std::mem::take(&mut *key), value, concealed);
                if preserve_data_extras && let Ok(data) = field_data {
                    append_custom_extra_records(
                        &mut records,
                        &format!("{field_key} / data"),
                        &data.extra_fields,
                    );
                }
                append_custom_extra_records(&mut records, &field_key, &field.extra_fields);
            }
        }
        CustomContent { records, notes }
    }
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ProtonCustomSection {
    #[serde(default)]
    section_name: String,
    #[serde(default)]
    section_fields: Vec<ProtonCustomField>,
    #[serde(default, flatten)]
    extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ProtonCustomField {
    #[serde(default)]
    field_name: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    data: SecretJson,
    #[serde(default, flatten)]
    extra_fields: SecretFields,
}

#[derive(Default, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ProtonCustomFieldData {
    #[serde(default)]
    content: String,
    #[serde(default)]
    totp_uri: String,
    #[serde(default)]
    timestamp: SecretJson,
    #[serde(default, flatten)]
    extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ProtonWifi {
    #[serde(default)]
    ssid: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    security: SecretJson,
    #[serde(default)]
    sections: SecretJson,
    #[serde(default, flatten)]
    extra_fields: SecretFields,
}

impl ProtonWifi {
    fn security(&self) -> Option<WifiSecurity> {
        let raw = self.security.0.trim();
        let value = serde_json::from_str::<String>(raw).ok();
        match value.as_deref().unwrap_or(raw) {
            "1" | "WPA" | "WPA2" => Some(WifiSecurity::Wpa),
            "2" => Some(WifiSecurity::Wpa),
            "3" | "WPA3" => Some(WifiSecurity::Wpa3),
            "4" | "WEP" => Some(WifiSecurity::Wep),
            _ => None,
        }
    }

    fn has_unmapped_fields(&self) -> bool {
        self.sections.has_value() || !self.extra_fields.is_empty()
    }
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ProtonSshKey {
    #[serde(default)]
    private_key: String,
    #[serde(default)]
    public_key: String,
    #[serde(default)]
    sections: SecretJson,
    #[serde(default, flatten)]
    extra_fields: SecretFields,
}

impl ProtonSshKey {
    fn has_unmapped_fields(&self) -> bool {
        self.sections.has_value() || !self.extra_fields.is_empty()
    }
}

#[derive(Default)]
struct SecretFields(BTreeMap<String, SecretJson>);

impl<'de> Deserialize<'de> for SecretFields {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut values = BTreeMap::<String, serde_json::Value>::deserialize(deserializer)?;
        let mut fields = BTreeMap::new();
        for (key, mut value) in std::mem::take(&mut values) {
            let raw = serde_json::to_string(&value).map_err(serde::de::Error::custom)?;
            zeroize_json_value(&mut value);
            fields.insert(key, SecretJson(raw));
        }
        Ok(Self(fields))
    }
}

fn zeroize_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(string) => string.zeroize(),
        serde_json::Value::Array(values) => {
            for value in values.iter_mut() {
                zeroize_json_value(value);
            }
            values.clear();
        }
        serde_json::Value::Object(values) => {
            let values = std::mem::take(values);
            for (mut key, mut value) in values {
                key.zeroize();
                zeroize_json_value(&mut value);
            }
        }
        serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {
            *value = serde_json::Value::Null;
        }
        serde_json::Value::Null => {}
    }
}

impl Zeroize for SecretFields {
    fn zeroize(&mut self) {
        while let Some((mut key, mut value)) = self.0.pop_first() {
            key.zeroize();
            value.zeroize();
        }
    }
}

impl Drop for SecretFields {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl std::ops::Deref for SecretFields {
    type Target = BTreeMap<String, SecretJson>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SecretJson {
    fn has_value(&self) -> bool {
        !matches!(self.0.trim(), "" | "[]" | "{}" | "null" | "\"\"")
    }
}
