//! Converts secret-store records into the JSON contracts consumed by QML.

use kryptopass_core::{
    Folder, FolderId, KeyFingerprint, LoginContent, LoginHistory, LoginHistoryEventKind,
    NoteContent, RecipientSelection, RecipientSet, SecretContent, SecretId, SecretMetadata,
    TotpConfig, format_totp_input, generate_totp, parse_totp_input,
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

#[derive(serde::Serialize, Zeroize, ZeroizeOnDrop)]
struct TotpProjection {
    #[serde(rename = "currentCode")]
    current_code: String,
    #[serde(rename = "nextCode")]
    next_code: String,
    period: u64,
    #[serde(rename = "remainingSeconds")]
    remaining_seconds: u64,
    #[serde(rename = "validUntil")]
    valid_until: u64,
    #[serde(rename = "nextValidFrom")]
    next_valid_from: u64,
}

#[derive(serde::Serialize)]
struct HistoryProjection {
    events: Vec<HistoryEventProjection>,
    #[serde(rename = "passwordRotations")]
    password_rotations: Vec<HistoryAtProjection>,
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
pub(super) fn store_snapshot(folders: Vec<Folder>, secrets: Vec<SecretMetadata>) -> StoreSnapshot {
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
    #[serde(default)]
    urls: Option<Vec<SensitiveText>>,
    #[serde(rename = "totpInput", default)]
    totp_input: SensitiveText,
    notes: SensitiveText,
    value: SensitiveText,
    recipients: Vec<String>,
}

#[derive(Default, Zeroize, ZeroizeOnDrop)]
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
        "login" if draft.value.0.is_empty() => {
            let urls = match draft.urls.take() {
                Some(urls) => urls.into_iter().map(|mut value| value.take()).collect(),
                None => {
                    let legacy_url = draft.url.take();
                    if legacy_url.is_empty() {
                        Vec::new()
                    } else {
                        vec![legacy_url]
                    }
                }
            };
            let url = urls.first().cloned().unwrap_or_default();
            let totp = parse_totp_input(&draft.totp_input.0)?;
            SecretContent::Login(LoginContent {
                username: draft.username.take(),
                password: draft.password.take(),
                url,
                notes: draft.notes.take(),
                urls,
                totp,
                history: Box::default(),
            })
        }
        "note"
            if draft.username.0.is_empty()
                && draft.password.0.is_empty()
                && draft.url.0.is_empty()
                && draft.urls.as_ref().is_none_or(Vec::is_empty)
                && draft.totp_input.0.is_empty() =>
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
    unix_seconds: u64,
) -> anyhow::Result<Zeroizing<String>> {
    let (kind, username, url, urls, notes, value, password, password_revealed, totp, history) =
        match content {
            SecretContent::Login(login) => {
                let totp = login
                    .totp
                    .as_ref()
                    .map(|config| totp_projection(config, unix_seconds))
                    .transpose()?;
                let history = Some(history_projection(&login.history));
                (
                    "login",
                    login.username.as_str(),
                    login.url.as_str(),
                    login.urls.as_slice(),
                    login.notes.as_str(),
                    "",
                    if reveal_password {
                        login.password.as_str()
                    } else {
                        ""
                    },
                    reveal_password,
                    totp,
                    history,
                )
            }
            SecretContent::Note(note) => (
                "note",
                "",
                "",
                &[] as &[String],
                note.notes.as_str(),
                note.value.as_str(),
                "",
                false,
                None,
                None,
            ),
        };
    serde_json::to_string(&ContentProjection {
        id: id.0.to_string(),
        kind,
        password_revealed,
        username,
        url,
        urls,
        notes,
        value,
        password,
        totp,
        history,
    })
    .map(Zeroizing::new)
    .map_err(Into::into)
}

pub(super) fn editor_projection(entry: &OpenedEntry) -> anyhow::Result<Zeroizing<String>> {
    let (kind, username, password, url, urls, totp_input, notes, value) = match &entry.content {
        SecretContent::Login(login) => {
            let totp_input = login
                .totp
                .as_ref()
                .map(format_totp_input)
                .transpose()?
                .unwrap_or_else(Zeroizing::default);
            (
                "login",
                login.username.as_str(),
                login.password.as_str(),
                login.url.as_str(),
                login.urls.as_slice(),
                totp_input,
                login.notes.as_str(),
                "",
            )
        }
        SecretContent::Note(note) => (
            "note",
            "",
            "",
            "",
            &[] as &[String],
            Zeroizing::default(),
            note.notes.as_str(),
            note.value.as_str(),
        ),
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
        urls,
        totp_input: totp_input.as_str(),
        notes,
        value,
        recipients: &entry.recipients,
    })
    .map(Zeroizing::new)
    .map_err(Into::into)
}

pub(super) fn new_editor_projection() -> Zeroizing<String> {
    Zeroizing::new(
        r#"{"id":"","revision":null,"folder":"","title":"","kind":"login","username":"","password":"","url":"","urls":[],"totpInput":"","notes":"","value":"","recipients":[]}"#.to_owned(),
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
    urls: &'a [String],
    notes: &'a str,
    value: &'a str,
    password: &'a str,
    totp: Option<TotpProjection>,
    history: Option<HistoryProjection>,
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
    urls: &'a [String],
    #[serde(rename = "totpInput")]
    totp_input: &'a str,
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

    HistoryProjection {
        events,
        password_rotations,
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
