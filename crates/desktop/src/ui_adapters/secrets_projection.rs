//! Converts secret-store records into the JSON contracts consumed by QML.

use kryptopass_core::{
    Folder, FolderId, KeyFingerprint, LoginContent, NoteContent, RecipientSelection, RecipientSet,
    SecretContent, SecretId, SecretMetadata,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Serialized records for the two metadata-only list properties.
pub(super) struct StoreSnapshot {
    pub(super) entries: Vec<String>,
    pub(super) folders: Vec<String>,
}

pub(super) struct OpenedEntry {
    pub(super) id: SecretId,
    pub(super) folder: Option<FolderId>,
    pub(super) title: String,
    pub(super) revision: u64,
    pub(super) recipients: Vec<String>,
    pub(super) content: SecretContent,
}

pub(super) struct PreparedSave {
    pub(super) id: Option<SecretId>,
    pub(super) folder: Option<FolderId>,
    pub(super) title: String,
    pub(super) recipients: RecipientSelection,
    pub(super) content: SecretContent,
    pub(super) expected_revision: Option<u64>,
}

/// Projects list data without copying encrypted payload bytes into QML.
pub(super) fn store_snapshot(folders: Vec<Folder>, secrets: Vec<SecretMetadata>) -> StoreSnapshot {
    let folders = folders
        .into_iter()
        .map(|folder| {
            json!({
                "id": folder.id.0.to_string(),
                "parent": folder.parent_id.map_or_else(String::new, |id| id.0.to_string()),
                "name": folder.name,
                "icon": folder.appearance.icon(),
                "color": folder.appearance.color(),
            })
            .to_string()
        })
        .collect();

    let entries = secrets
        .into_iter()
        .map(|secret| {
            json!({
                "id": secret.id.0.to_string(),
                "folder": secret.folder_id.map_or_else(String::new, |id| id.0.to_string()),
                "title": secret.title,
                "kind": secret.kind.as_str(),
                "format": "Texto",
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

    StoreSnapshot { entries, folders }
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(super) struct EntryDraft {
    id: String,
    revision: Option<u64>,
    folder: String,
    title: SensitiveText,
    kind: String,
    username: SensitiveText,
    password: SensitiveText,
    url: SensitiveText,
    notes: SensitiveText,
    value: SensitiveText,
    recipients: Vec<String>,
}

#[derive(Zeroize, ZeroizeOnDrop)]
struct SensitiveText(String);

impl<'de> Deserialize<'de> for SensitiveText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self)
    }
}

impl SensitiveText {
    fn take(&mut self) -> String {
        std::mem::take(&mut self.0)
    }
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
    let folder = parse_optional_folder_id(&draft.folder)?;
    let mut recipients = Vec::with_capacity(draft.recipients.len());
    for fingerprint in draft.recipients.drain(..) {
        recipients.push(KeyFingerprint::parse(&fingerprint)?);
    }
    anyhow::ensure!(
        !recipients.is_empty(),
        "at least one explicit recipient is required"
    );
    let recipients = RecipientSelection::Explicit(RecipientSet::new(recipients)?);

    let content = match draft.kind.as_str() {
        "login" if draft.value.0.is_empty() => SecretContent::Login(LoginContent {
            username: draft.username.take(),
            password: draft.password.take(),
            url: draft.url.take(),
            notes: draft.notes.take(),
        }),
        "note"
            if draft.username.0.is_empty()
                && draft.password.0.is_empty()
                && draft.url.0.is_empty() =>
        {
            SecretContent::Note(NoteContent {
                value: draft.value.take(),
                notes: draft.notes.take(),
            })
        }
        _ => anyhow::bail!("unsupported or inconsistent secret content"),
    };

    Ok(PreparedSave {
        id: session_id,
        folder,
        title: draft.title.take(),
        recipients,
        content,
        expected_revision: session_revision,
    })
}

pub(super) fn content_projection(
    id: SecretId,
    content: &SecretContent,
    reveal_password: bool,
) -> serde_json::Result<Zeroizing<String>> {
    let (kind, username, url, notes, value, password, password_revealed) = match content {
        SecretContent::Login(login) => (
            "login",
            login.username.as_str(),
            login.url.as_str(),
            login.notes.as_str(),
            "",
            if reveal_password {
                login.password.as_str()
            } else {
                ""
            },
            reveal_password,
        ),
        SecretContent::Note(note) => (
            "note",
            "",
            "",
            note.notes.as_str(),
            note.value.as_str(),
            "",
            false,
        ),
    };
    serde_json::to_string(&ContentProjection {
        id: id.0.to_string(),
        kind,
        password_revealed,
        username,
        url,
        notes,
        value,
        password,
    })
    .map(Zeroizing::new)
}

pub(super) fn editor_projection(entry: &OpenedEntry) -> serde_json::Result<Zeroizing<String>> {
    let (kind, username, password, url, notes, value) = match &entry.content {
        SecretContent::Login(login) => (
            "login",
            login.username.as_str(),
            login.password.as_str(),
            login.url.as_str(),
            login.notes.as_str(),
            "",
        ),
        SecretContent::Note(note) => ("note", "", "", "", note.notes.as_str(), note.value.as_str()),
    };
    serde_json::to_string(&EditorProjection {
        id: entry.id.0.to_string(),
        revision: entry.revision,
        folder: entry.folder.map_or_else(String::new, |id| id.0.to_string()),
        title: &entry.title,
        kind,
        username,
        password,
        url,
        notes,
        value,
        recipients: &entry.recipients,
    })
    .map(Zeroizing::new)
}

pub(super) fn new_editor_projection() -> Zeroizing<String> {
    Zeroizing::new(
        r#"{"id":"","revision":null,"folder":"","title":"","kind":"login","username":"","password":"","url":"","notes":"","value":"","recipients":[]}"#.to_owned(),
    )
}

#[derive(serde::Serialize)]
struct ContentProjection<'a> {
    id: String,
    kind: &'static str,
    #[serde(rename = "passwordRevealed")]
    password_revealed: bool,
    username: &'a str,
    url: &'a str,
    notes: &'a str,
    value: &'a str,
    password: &'a str,
}

#[derive(serde::Serialize)]
struct EditorProjection<'a> {
    id: String,
    revision: u64,
    folder: String,
    title: &'a str,
    kind: &'static str,
    username: &'a str,
    password: &'a str,
    url: &'a str,
    notes: &'a str,
    value: &'a str,
    recipients: &'a [String],
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
