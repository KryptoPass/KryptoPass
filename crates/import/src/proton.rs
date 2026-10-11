//! Proton Pass JSON translation. Provider wire types remain private here.

use crate::{
    ExportImporter, ImportError, ImportIssue, ImportPlan, ImportSummary, IssueKind,
    MAX_EXPORT_BYTES, MAX_RECORDS,
};
use kryptopass_core::{
    ImportBatch, ImportFolder, ImportItem, ImportValidationError, LoginContent, NoteContent,
    SecretContent, parse_totp_input,
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
                let note = match &mut content {
                    SecretContent::Login(login) => &mut login.notes,
                    SecretContent::Note(note) => &mut note.value,
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
}
