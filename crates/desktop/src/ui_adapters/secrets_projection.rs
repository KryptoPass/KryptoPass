//! Common metadata, sessions and typed content projections consumed by QML.

mod content;
mod expanded;

use kryptopass_core::{
    EntryIcon, Folder, FolderId, KeyFingerprint, LoginHistory, LoginHistoryEventKind,
    RecipientSelection, RecipientSet, SecretContent, SecretId, SecretLink, SecretMetadata,
    TotpConfig, generate_totp,
};
use kryptopass_secure_memory::{SensitiveBytes, SensitiveText};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use content::{DetailContent, DraftContent, EditorContent};

/// Serialized records for the two metadata-only list properties.
pub(super) struct StoreSnapshot {
    pub(super) entries: Vec<String>,
    pub(super) folders: Vec<String>,
    pub(super) favicon_hosts: Vec<String>,
}

#[derive(Deserialize)]
struct EntryRevisionRecord {
    id: String,
    revision: u64,
}

pub(super) fn snapshot_revision(snapshot: &StoreSnapshot, id: SecretId) -> Option<u64> {
    let id = id.0.to_string();
    snapshot
        .entries
        .iter()
        .filter_map(|record| serde_json::from_str::<EntryRevisionRecord>(record).ok())
        .find_map(|record| (record.id == id).then_some(record.revision))
}

pub(super) struct OpenedEntry {
    pub(super) id: SecretId,
    pub(super) folder: Option<FolderId>,
    pub(super) title: String,
    pub(super) revision: u64,
    pub(super) recipients: Vec<String>,
    pub(super) icon: Option<EntryIcon>,
    pub(super) related_entry: Option<SecretId>,
    pub(super) content: SecretContent,
}

pub(super) struct PreparedSave {
    pub(super) id: Option<SecretId>,
    pub(super) folder: Option<FolderId>,
    pub(super) title: String,
    pub(super) recipients: RecipientSelection,
    pub(super) content: SecretContent,
    pub(super) expected_revision: Option<u64>,
    pub(super) icon_update: Option<Option<EntryIcon>>,
    pub(super) link_update: Option<Option<SecretId>>,
}

#[derive(serde::Serialize, Zeroize, ZeroizeOnDrop)]
struct TotpProjection {
    #[serde(rename = "currentCode")]
    current_code: SensitiveText,
    #[serde(rename = "nextCode")]
    next_code: SensitiveText,
    period: u64,
    #[serde(rename = "remainingSeconds")]
    remaining_seconds: u64,
    #[serde(rename = "validUntil")]
    valid_until: u64,
    #[serde(rename = "nextValidFrom")]
    next_valid_from: u64,
    issuer: Option<SensitiveText>,
    account: Option<SensitiveText>,
    #[zeroize(skip)]
    algorithm: &'static str,
    digits: u8,
}

#[derive(serde::Serialize)]
struct HistoryProjection {
    events: Vec<HistoryEventProjection>,
    #[serde(rename = "passwordRotations")]
    password_rotations: Vec<HistoryAtProjection>,
    versions: Vec<VersionMetadataProjection>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionMetadataProjection {
    index: usize,
    at: u64,
    password_only: bool,
}

#[derive(serde::Serialize)]
struct HistoryEventProjection {
    kind: &'static str,
    at: u64,
}

#[derive(serde::Serialize)]
struct HistoryAtProjection {
    at: u64,
}

/// Projects list data without copying encrypted payload bytes into QML.
pub(super) fn store_snapshot_with_links(
    folders: Vec<Folder>,
    secrets: Vec<SecretMetadata>,
    links: Vec<SecretLink>,
) -> StoreSnapshot {
    let mut companions = std::collections::HashMap::new();
    for link in links {
        companions.insert(link.first, link.second);
        companions.insert(link.second, link.first);
    }
    let favicon_hosts = secrets
        .iter()
        .filter_map(|secret| match secret.icon.as_ref() {
            Some(EntryIcon::Favicon { host }) => Some(host.clone()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let folders = folders
        .into_iter()
        .map(|folder| {
            json!({
                "id": folder.id.0.to_string(),
                "parent": folder.parent_id.map_or_else(String::new, |id| id.0.to_string()),
                "name": folder.name,
                "position": folder.sort_position,
                "icon": folder.appearance.icon(),
                "color": folder.appearance.color(),
            })
            .to_string()
        })
        .collect();

    let entries = secrets
        .into_iter()
        .map(|secret| {
            let display_icon = secret.icon.as_ref();
            json!({
                "id": secret.id.0.to_string(),
                "folder": secret.folder_id.map_or_else(String::new, |id| id.0.to_string()),
                "title": secret.title,
                "relatedEntryId": companions.get(&secret.id).map_or_else(String::new, |id| id.0.to_string()),
                "kind": secret.kind.as_str(),
                "format": "Texto",
                "iconSource": crate::infrastructure::entry_icons::source_for(display_icon),
                "icon": secret.icon,
                "revision": secret.revision,
                "recipients": secret.recipients.as_slice().iter().map(|key| key.as_str()).collect::<Vec<_>>(),
                "favorite": secret.favorite,
                "trashed": secret.trashed,
                "value": "",
                "notes": "",
                "details": {},
            })
            .to_string()
        })
        .collect();

    StoreSnapshot {
        entries,
        folders,
        favicon_hosts,
    }
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(super) struct EntryDraft {
    id: String,
    #[serde(default)]
    #[zeroize(skip)]
    icon: DraftIcon,
    revision: Option<u64>,
    folder: String,
    title: String,
    content: DraftContent,
    #[serde(rename = "iconSource", default)]
    _icon_source: String,
    recipients: Vec<String>,
    #[serde(rename = "relatedEntryId", default)]
    related_entry_id: Option<String>,
}

#[derive(Default)]
struct DraftIcon {
    present: bool,
    value: Option<EntryIcon>,
}
impl<'de> Deserialize<'de> for DraftIcon {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<EntryIcon>::deserialize(deserializer).map(|value| Self {
            present: true,
            value,
        })
    }
}

impl EntryDraft {
    pub(super) fn ssh_import_token(&self) -> Option<&str> {
        match &self.content {
            DraftContent::SshKey(value) => Some(&value.import_token),
            _ => None,
        }
    }
    pub(super) fn attach_ssh(&mut self, content: kryptopass_core::SshKeyContent) {
        if let DraftContent::SshKey(value) = &mut self.content {
            value.prepared = Some(content);
        }
    }
}

pub(super) fn content_field<'a>(
    content: &'a SecretContent,
    id: &str,
) -> Option<(expanded::SensitiveValue<'a>, bool)> {
    expanded::field(content, id)
}

pub(super) fn draft_matches_session(
    draft: &EntryDraft,
    session_id: Option<SecretId>,
    session_revision: Option<u64>,
) -> bool {
    match session_id {
        Some(id) => {
            draft.id == id.0.to_string()
                && session_revision.is_some()
                && draft.revision == session_revision
        }
        None => draft.id.is_empty() && session_revision.is_none() && draft.revision.is_none(),
    }
}

pub(super) fn prepare_save(
    draft: &mut EntryDraft,
    session_id: Option<SecretId>,
    session_revision: Option<u64>,
) -> anyhow::Result<PreparedSave> {
    if let Some(icon) = &draft.icon.value {
        icon.validate()?;
    }
    let folder = parse_optional_folder_id(&draft.folder)?;
    let link_update = draft
        .related_entry_id
        .as_deref()
        .map(|value| -> anyhow::Result<Option<SecretId>> {
            if value.is_empty() {
                Ok(None)
            } else {
                let id = Uuid::parse_str(value)
                    .ok()
                    .filter(|id| !id.is_nil())
                    .map(SecretId)
                    .ok_or_else(|| anyhow::anyhow!("related entry identifier is invalid"))?;
                anyhow::ensure!(Some(id) != session_id, "a secret cannot link to itself");
                Ok(Some(id))
            }
        })
        .transpose()?;
    let mut recipients = Vec::with_capacity(draft.recipients.len());
    for fingerprint in draft.recipients.drain(..) {
        recipients.push(KeyFingerprint::parse(&fingerprint)?);
    }
    anyhow::ensure!(
        !recipients.is_empty(),
        "at least one explicit recipient is required"
    );
    let recipients = RecipientSelection::Explicit(RecipientSet::new(recipients)?);

    let content = draft.content.prepare()?;
    content.validate()?;

    Ok(PreparedSave {
        id: session_id,
        folder,
        title: std::mem::take(&mut draft.title),
        recipients,
        content,
        expected_revision: session_revision,
        icon_update: draft.icon.present.then(|| draft.icon.value.take()),
        link_update,
    })
}

pub(super) fn content_projection(
    id: SecretId,
    content: &SecretContent,
    reveal_password: bool,
    unix_seconds: u64,
) -> anyhow::Result<SensitiveText> {
    content_projection_with_state(
        id,
        content,
        reveal_password,
        unix_seconds,
        &std::collections::BTreeSet::new(),
        None,
    )
}

pub(super) fn content_projection_with_state(
    id: SecretId,
    content: &SecretContent,
    reveal_password: bool,
    unix_seconds: u64,
    revealed_fields: &std::collections::BTreeSet<String>,
    qr_source: Option<&str>,
) -> anyhow::Result<SensitiveText> {
    sensitive_json(&ContentProjection {
        id: id.0.to_string(),
        content: DetailContent::new(
            content,
            reveal_password,
            unix_seconds,
            revealed_fields,
            qr_source,
        )?,
    })
}

pub(super) fn editor_projection(entry: &OpenedEntry) -> anyhow::Result<SensitiveText> {
    sensitive_json(&EditorProjection {
        id: entry.id.0.to_string(),
        revision: Some(entry.revision),
        folder: entry.folder.map_or_else(String::new, |id| id.0.to_string()),
        title: &entry.title,
        icon: &entry.icon,
        icon_source: crate::infrastructure::entry_icons::source_for(entry.icon.as_ref()),
        content: EditorContent::new(&entry.content)?,
        recipients: &entry.recipients,
        related_entry_id: entry
            .related_entry
            .map_or_else(String::new, |id| id.0.to_string()),
    })
}

pub(super) fn new_editor_projection() -> anyhow::Result<SensitiveText> {
    let content = SecretContent::Login(kryptopass_core::LoginContent::default());
    let projection = EditorProjection {
        id: String::new(),
        revision: None,
        folder: String::new(),
        title: "",
        icon: &None,
        icon_source: String::new(),
        content: EditorContent::new(&content)?,
        recipients: &[],
        related_entry_id: String::new(),
    };
    sensitive_json(&projection)
}

#[derive(serde::Serialize)]
struct ContentProjection<'a> {
    id: String,
    content: DetailContent<'a>,
}

#[derive(serde::Serialize)]
struct EditorProjection<'a> {
    id: String,
    revision: Option<u64>,
    folder: String,
    title: &'a str,
    icon: &'a Option<EntryIcon>,
    #[serde(rename = "iconSource")]
    icon_source: String,
    content: EditorContent<'a>,
    recipients: &'a [String],
    #[serde(rename = "relatedEntryId")]
    related_entry_id: String,
}

fn parse_optional_folder_id(value: &str) -> anyhow::Result<Option<FolderId>> {
    if value.is_empty() {
        Ok(None)
    } else {
        Uuid::parse_str(value)
            .ok()
            .map(FolderId)
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("folder identifier is invalid"))
    }
}

fn totp_projection(config: &TotpConfig, unix_seconds: u64) -> anyhow::Result<TotpProjection> {
    let snapshot = generate_totp(config, unix_seconds)?;
    let period = snapshot.period.max(1);
    let valid_until = unix_seconds
        .saturating_sub(unix_seconds % period)
        .saturating_add(period);

    Ok(TotpProjection {
        current_code: snapshot.current.clone(),
        next_code: snapshot.next.clone(),
        period: snapshot.period,
        remaining_seconds: snapshot.remaining,
        valid_until,
        next_valid_from: valid_until,
        issuer: config.issuer.clone(),
        account: config.account.clone(),
        algorithm: match config.algorithm {
            kryptopass_core::TotpAlgorithm::Sha1 => "SHA1",
            kryptopass_core::TotpAlgorithm::Sha256 => "SHA256",
            kryptopass_core::TotpAlgorithm::Sha512 => "SHA512",
        },
        digits: config.digits,
    })
}

fn history_projection(history: &LoginHistory) -> HistoryProjection {
    let events = history
        .events()
        .iter()
        .map(|event| HistoryEventProjection {
            kind: history_event_kind_name(&event.kind),
            at: event.timestamp,
        })
        .collect();
    let password_rotations = history
        .password_change_timestamps()
        .map(|timestamp| HistoryAtProjection { at: timestamp })
        .collect();

    let versions = history
        .version_metadata()
        .into_iter()
        .enumerate()
        .map(|(index, (at, password_only))| VersionMetadataProjection {
            index,
            at,
            password_only,
        })
        .collect();
    HistoryProjection {
        events,
        password_rotations,
        versions,
    }
}

fn history_event_kind_name(kind: &LoginHistoryEventKind) -> &'static str {
    match kind {
        LoginHistoryEventKind::Create => "create",
        LoginHistoryEventKind::LoginUse => "loginUse",
        LoginHistoryEventKind::PasswordRotated => "passwordRotated",
        LoginHistoryEventKind::Edited => "edited",
        LoginHistoryEventKind::CopyUsername => "copyUsername",
        LoginHistoryEventKind::CopyPassword => "copyPassword",
        LoginHistoryEventKind::CopyTotp => "copyTotp",
        LoginHistoryEventKind::CopyNextTotp => "copyNextTotp",
    }
}

#[cfg(test)]
#[path = "secrets_projection_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "expanded_projection_tests.rs"]
mod expanded_tests;

pub(super) fn sensitive_json(value: &impl serde::Serialize) -> anyhow::Result<SensitiveText> {
    let mut bytes = SensitiveBytes::default();
    serde_json::to_writer(&mut bytes, value)?;
    Ok(SensitiveText::from_utf8(bytes)?)
}
