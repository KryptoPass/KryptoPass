//! Common metadata, sessions and typed content projections consumed by QML.

mod content;
mod expanded;

use kryptopass_core::vault::{ItemMeta, OpaqueItem, VersionId, VersionInfo};
use kryptopass_core::{
    EntryIcon, Folder, FolderId, ItemContent, KeyFingerprint, RecipientSet, SecretId, SecretLink,
    generate_totp, otp::TotpAlgorithm, otp::TotpConfig,
};
use kryptopass_secure_memory::{SensitiveBytes, SensitiveText};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use content::{DetailContent, DraftContent, EditorContent};
pub(super) use content::{field_text, kind_of, template_of, totp_uri};

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
    pub(super) content: ItemContent,
}

pub(super) struct PreparedSave {
    pub(super) id: Option<SecretId>,
    pub(super) folder: Option<FolderId>,
    pub(super) title: String,
    pub(super) recipients: RecipientSet,
    pub(super) content: ItemContent,
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

/// An item's earlier versions as the history list shows them. Usage events are
/// no longer recorded, so `events` stays empty.
#[derive(serde::Serialize, Default)]
pub(super) struct HistoryProjection {
    events: Vec<serde_json::Value>,
    versions: Vec<VersionMetadataProjection>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionMetadataProjection {
    index: usize,
    at: u64,
}

/// The earlier versions of an item, newest first: readable ones without the
/// head, which the detail already shows.
pub(super) fn earlier_versions(
    head: VersionId,
    history: Vec<VersionInfo>,
) -> Vec<(VersionId, u64)> {
    let mut versions: Vec<_> = history
        .into_iter()
        .filter(|version| version.id != head && version.state.is_some())
        .map(|version| (version.clock, version.id))
        .collect();
    versions.sort_by(|a, b| b.cmp(a));
    versions
        .into_iter()
        .map(|(clock, id)| (id, clock.millis / 1000))
        .collect()
}

fn history_projection(versions: &[(VersionId, u64)]) -> HistoryProjection {
    HistoryProjection {
        events: Vec::new(),
        versions: versions
            .iter()
            .enumerate()
            .map(|(index, (_, at))| VersionMetadataProjection { index, at: *at })
            .collect(),
    }
}

/// Projects list data without copying encrypted payload bytes into QML.
pub(super) fn store_snapshot(
    folders: Vec<Folder>,
    items: Vec<ItemMeta>,
    links: Vec<SecretLink>,
    opaque: Vec<OpaqueItem>,
) -> StoreSnapshot {
    let mut companions = std::collections::HashMap::new();
    for link in links {
        companions.insert(link.first, link.second);
        companions.insert(link.second, link.first);
    }
    let favicon_hosts = items
        .iter()
        .filter_map(|item| match item.state.icon.as_ref() {
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

    let mut entries: Vec<String> = items
        .into_iter()
        .map(|item| {
            let state = &item.state;
            let display_icon = state.icon.as_ref();
            json!({
                "id": item.id.0.to_string(),
                "folder": state.folder.map_or_else(String::new, |id| id.0.to_string()),
                "title": state.title,
                "relatedEntryId": companions.get(&item.id).map_or_else(String::new, |id| id.0.to_string()),
                "kind": kind_of(state.template.as_str()),
                "format": "Texto",
                "iconSource": crate::infrastructure::entry_icons::source_for(display_icon),
                "icon": state.icon,
                "revision": item.revision,
                "recipients": state.recipients.as_slice().iter().map(|key| key.as_str()).collect::<Vec<_>>(),
                "favorite": state.favorite,
                "trashed": state.trashed,
                "conflicted": item.conflicted,
                "readable": true,
                "value": "",
                "notes": "",
                "details": {},
            })
            .to_string()
        })
        .collect();
    // Without a readable version there is no title: the list says so instead
    // of hiding what this device stores but cannot open.
    entries.extend(opaque.into_iter().map(|item| {
        json!({
            "id": item.id.0.to_string(),
            "folder": "",
            "title": crate::i18n::text("No legible en este dispositivo"),
            "relatedEntryId": "",
            "kind": "custom",
            "format": "Texto",
            "iconSource": "",
            "icon": null,
            "revision": 0,
            "recipients": item.recipients.as_slice().iter().map(|key| key.as_str()).collect::<Vec<_>>(),
            "favorite": false,
            "trashed": false,
            "conflicted": false,
            "readable": false,
            "value": "",
            "notes": "",
            "details": {},
        })
        .to_string()
    }));

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
        self.content.ssh_import_token()
    }

    /// Replaces the private key with one imported in this editor session.
    pub(super) fn attach_ssh(&mut self, key: SshKeyMaterial) {
        self.content.attach_ssh(key);
    }
}

/// A private key read from a file, waiting for the editor to save it.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub(super) struct SshKeyMaterial {
    pub(super) private_key: SensitiveText,
    pub(super) passphrase: SensitiveText,
}

/// A concealed or plain value of a detail row, and whether it is concealed.
pub(super) fn content_field(content: &ItemContent, id: &str) -> Option<(SensitiveText, bool)> {
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

/// Turns the editor's draft into a write. `base` is the content the editor
/// opened: fields the editor does not show are kept as they are.
pub(super) fn prepare_save(
    draft: &mut EntryDraft,
    session_id: Option<SecretId>,
    session_revision: Option<u64>,
    base: Option<ItemContent>,
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
    let recipients = RecipientSet::new(recipients)?;
    let content = draft.content.prepare(base)?;

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

pub(super) struct DetailView<'a> {
    pub(super) reveal_password: bool,
    pub(super) unix_seconds: u64,
    pub(super) revealed_fields: &'a std::collections::BTreeSet<String>,
    pub(super) qr_source: Option<&'a str>,
    pub(super) versions: &'a [(VersionId, u64)],
}

pub(super) fn content_projection(
    id: SecretId,
    content: &ItemContent,
    view: &DetailView<'_>,
) -> anyhow::Result<SensitiveText> {
    sensitive_json(&ContentProjection {
        id: id.0.to_string(),
        content: DetailContent::new(content, view)?,
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
    let content = ItemContent::new(template_of("login")?.1);
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
            TotpAlgorithm::Sha1 => "SHA1",
            TotpAlgorithm::Sha256 => "SHA256",
            TotpAlgorithm::Sha512 => "SHA512",
        },
        digits: config.digits,
    })
}

pub(super) fn sensitive_json(value: &impl serde::Serialize) -> anyhow::Result<SensitiveText> {
    let mut bytes = SensitiveBytes::default();
    serde_json::to_writer(&mut bytes, value)?;
    Ok(SensitiveText::from_utf8(bytes)?)
}
