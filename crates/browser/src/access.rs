//! Requests project account names temporarily to Qt and release one explicitly selected login.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};
use kryptopass_secure_memory::{SensitiveBytes, SensitiveText};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{persistence, protocol_error, vault};

#[derive(Clone, Serialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub revision: u64,
}

pub struct Login {
    pub username: SensitiveText,
    pub password: SensitiveText,
    pub urls: Vec<SensitiveText>,
}

pub trait CredentialProvider: Send + Sync {
    /// Lists metadata only. Implementations must exclude trash and non-login entries.
    fn candidates(&self) -> Result<Vec<Candidate>>;
    /// Projects only the username for the open request, rejecting stale or inactive entries.
    fn username(
        &self,
        _candidate: &Candidate,
        _still_current: &dyn Fn() -> bool,
    ) -> Result<Option<SensitiveText>> {
        Ok(None)
    }
    /// Reads only the approved revision, rejecting stale/trash/non-login entries.
    fn login(&self, candidate: &Candidate, still_authorized: &dyn Fn() -> bool) -> Result<Login>;
}

pub(crate) struct Access {
    token: String,
    origin: String,
    candidates: Vec<Candidate>,
    usernames: BTreeMap<String, SensitiveText>,
    username_errors: BTreeSet<String>,
    preview_requested: Option<String>,
    previewing: Option<String>,
    pub(super) expires: Instant,
    decision: Option<String>,
}

/// Bind a canonical vault path to its persistent browser identity while locked.
/// The caller supplies an already private, application-owned state directory.
pub fn configure_vault(
    directory: &Path,
    database: &Path,
    provider: Arc<dyn CredentialProvider>,
) -> Result<()> {
    let database = database.canonicalize()?;
    let digest = Sha256::digest(database.as_os_str().as_encoded_bytes());
    let path = directory.join(format!("browser-{digest:x}.json"));
    let mut state = vault()
        .lock()
        .map_err(|_| anyhow::anyhow!("browser state unavailable"))?;
    if state.store_path.as_ref() == Some(&path) {
        return Ok(());
    }
    state.unlocked = false;
    state.epoch = state.epoch.wrapping_add(1);
    state.pending = None;
    state.access = None;
    state.provider = None;
    state.store_path = None;
    state.associations.clear();
    state.hash.clear();
    let store = persistence::load(&path)?;
    state.hash = store.hash;
    state.associations = store.associations;
    state.provider = Some(provider);
    state.store_path = Some(path);
    Ok(())
}

/// Accept HTTP(S) URLs with no embedded credentials; return a normalized ASCII origin.
pub fn site_origin(input: &str) -> Result<String> {
    ensure!(
        !input.is_empty()
            && input.len() <= 8192
            && !input.chars().any(|c| c.is_control() || c.is_whitespace())
            && !input.contains('\\'),
        "invalid site URL"
    );
    let url = url::Url::parse(input)?;
    ensure!(
        matches!(url.scheme(), "https" | "http")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "unsupported site URL"
    );
    Ok(url.origin().ascii_serialization())
}

pub fn pending_access() -> SensitiveText {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Preview<'a> {
        token: &'a str,
        origin: &'a str,
        preview_ready: bool,
        candidates: Vec<PreviewCandidate<'a>>,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct PreviewCandidate<'a> {
        id: &'a str,
        name: &'a str,
        revision: u64,
        username: &'a str,
        username_state: &'static str,
    }
    vault()
        .lock()
        .ok()
        .and_then(|state| {
            let access = state
                .access
                .as_ref()
                .filter(|access| access.expires > Instant::now() && access.decision.is_none())?;
            let candidates = access
                .candidates
                .iter()
                .map(|candidate| PreviewCandidate {
                    id: &candidate.id,
                    name: &candidate.name,
                    revision: candidate.revision,
                    username: access
                        .usernames
                        .get(&candidate.id)
                        .map(|username| username.as_str())
                        .unwrap_or_default(),
                    username_state: if access.usernames.contains_key(&candidate.id) {
                        "ready"
                    } else if access.username_errors.contains(&candidate.id) {
                        "error"
                    } else if access.previewing.as_deref() == Some(&candidate.id)
                        || access.preview_requested.as_deref() == Some(&candidate.id)
                    {
                        "loading"
                    } else {
                        "unloaded"
                    },
                })
                .collect();
            let mut bytes = SensitiveBytes::new();
            serde_json::to_writer(
                &mut bytes,
                &Preview {
                    token: &access.token,
                    origin: &access.origin,
                    preview_ready: true,
                    candidates,
                },
            )
            .ok()?;
            SensitiveText::from_utf8(bytes).ok()
        })
        .unwrap_or_default()
}

pub fn decide_access(token: &str, entry_id: &str, approve: bool) {
    if let Ok(mut state) = vault().lock() {
        if !state.unlocked {
            return;
        }
        if let Some(access) = state.access.as_mut() {
            if access.token == token && access.expires > Instant::now() && access.decision.is_none()
            {
                if !approve {
                    access.decision = Some(String::new());
                } else if access.usernames.contains_key(entry_id)
                    && access
                        .candidates
                        .iter()
                        .any(|candidate| candidate.id == entry_id)
                {
                    access.decision = Some(entry_id.to_owned());
                }
                if access.decision.is_some() {
                    access.usernames.clear();
                }
            }
        }
    }
}

fn authorized(epoch: u64, token: &str, selected: &str) -> bool {
    vault().lock().is_ok_and(|state| {
        state.unlocked
            && state.epoch == epoch
            && state.access.as_ref().is_some_and(|access| {
                access.token == token
                    && access.expires > Instant::now()
                    && access.decision.as_deref() == Some(selected)
            })
    })
}

#[derive(Serialize)]
pub(crate) struct LoginResponse {
    success: &'static str,
    hash: String,
    count: &'static str,
    entries: [LoginEntry; 1],
}

#[derive(Serialize)]
struct LoginEntry {
    uuid: String,
    name: String,
    login: SensitiveText,
    password: SensitiveText,
    expired: &'static str,
    totp: &'static str,
    #[serde(rename = "stringFields")]
    string_fields: [(); 0],
}

pub(crate) fn get_logins(request: &Value, data: &Value) -> Result<LoginResponse, Value> {
    match approved_login(data) {
        Ok((candidate, mut login, hash)) => Ok(LoginResponse {
            success: "true",
            hash,
            count: "1",
            entries: [LoginEntry {
                uuid: candidate.id,
                name: candidate.name,
                login: std::mem::take(&mut login.username),
                password: std::mem::take(&mut login.password),
                expired: "false",
                totp: "",
                string_fields: [],
            }],
        }),
        Err(_) => Err(protocol_error(
            request,
            6,
            "Login access denied, expired, or selected entry does not match the site",
        )),
    }
}

fn approved_login(data: &Value) -> Result<(Candidate, Login, String)> {
    let (epoch, hash, provider) = {
        let state = vault()
            .lock()
            .map_err(|_| anyhow::anyhow!("browser state unavailable"))?;
        ensure!(state.unlocked, "vault locked");
        let keys = data["keys"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("association keys required"))?;
        ensure!(
            keys.len() <= 32
                && keys.iter().any(|key| {
                    key["id"]
                        .as_str()
                        .and_then(|id| state.associations.get(id))
                        .zip(key["key"].as_str())
                        .is_some_and(|(stored, supplied)| stored == supplied)
                }),
            "unknown browser association"
        );
        (
            state.epoch,
            state.hash.clone(),
            state
                .provider
                .clone()
                .ok_or_else(|| anyhow::anyhow!("vault provider missing"))?,
        )
    };
    let origin = site_origin(
        data["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("site URL required"))?,
    )?;
    if let Some(submit) = data
        .get("submitUrl")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
    {
        ensure!(
            site_origin(submit)? == origin,
            "cross-origin submission is not supported"
        );
    }
    let mut candidates = provider.candidates()?;
    ensure!(candidates.len() <= 10000, "too many credentials");
    candidates.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    let token = Uuid::new_v4().to_string();
    {
        let mut state = vault()
            .lock()
            .map_err(|_| anyhow::anyhow!("browser state unavailable"))?;
        ensure!(state.unlocked && state.epoch == epoch, "vault changed");
        ensure!(
            state.pending.is_none()
                && state
                    .access
                    .as_ref()
                    .is_none_or(|access| access.expires <= Instant::now()),
            "another browser request is pending"
        );
        if candidates.is_empty() {
            bail!("no login candidates");
        }
        state.access = Some(Access {
            token: token.clone(),
            origin: origin.clone(),
            candidates: candidates.clone(),
            usernames: BTreeMap::new(),
            username_errors: BTreeSet::new(),
            preview_requested: None,
            previewing: None,
            expires: Instant::now() + Duration::from_secs(60),
            decision: None,
        });
    }
    let selected = loop {
        load_requested_username(epoch, &token, &provider);
        std::thread::sleep(Duration::from_millis(50));
        let mut state = vault()
            .lock()
            .map_err(|_| anyhow::anyhow!("browser state unavailable"))?;
        ensure!(state.unlocked && state.epoch == epoch, "vault changed");
        let access = state
            .access
            .as_ref()
            .filter(|access| access.token == token)
            .ok_or_else(|| anyhow::anyhow!("request cancelled"))?;
        if access.expires <= Instant::now() {
            state.access = None;
            bail!("request expired");
        }
        if let Some(decision) = access.decision.clone() {
            // Keep the slot occupied until decrypt completes, so concurrent requests cannot
            // make the desktop approve another entry while this one is being read.
            break decision;
        }
    };
    let result = (|| {
        let allowed = vault().lock().is_ok_and(|state| {
            state.access.as_ref().is_some_and(|access| {
                access.token == token
                    && access
                        .candidates
                        .iter()
                        .any(|candidate| candidate.id == selected)
            })
        });
        ensure!(allowed, "selected account is unavailable");
        let candidate = candidates
            .into_iter()
            .find(|candidate| candidate.id == selected)
            .ok_or_else(|| anyhow::anyhow!("request rejected"))?;
        ensure!(
            authorized(epoch, &token, &selected),
            "request no longer authorized"
        );
        let login = provider.login(&candidate, &|| authorized(epoch, &token, &selected))?;
        ensure!(
            authorized(epoch, &token, &selected),
            "request no longer authorized"
        );
        ensure!(
            login
                .urls
                .iter()
                .any(|url| site_origin(url).is_ok_and(|saved| saved == origin)),
            "selected credential has no matching site"
        );
        Ok((candidate, login, hash))
    })();
    clear_access(&token);
    result
}

fn preview_current(epoch: u64, token: &str) -> bool {
    vault().lock().is_ok_and(|state| {
        state.unlocked
            && state.epoch == epoch
            && state.access.as_ref().is_some_and(|access| {
                access.token == token
                    && access.expires > Instant::now()
                    && access.decision.is_none()
            })
    })
}

pub fn request_username(token: &str, entry_id: &str) {
    if let Ok(mut state) = vault().lock() {
        if !state.unlocked {
            return;
        }
        if let Some(access) = state.access.as_mut().filter(|access| {
            access.token == token && access.expires > Instant::now() && access.decision.is_none()
        }) {
            if access
                .candidates
                .iter()
                .any(|candidate| candidate.id == entry_id)
                && !access.usernames.contains_key(entry_id)
                && !access.username_errors.contains(entry_id)
                && access.previewing.as_deref() != Some(entry_id)
            {
                access.preview_requested = Some(entry_id.to_owned());
            }
        }
    }
}

fn load_requested_username(epoch: u64, token: &str, provider: &Arc<dyn CredentialProvider>) {
    let candidate = {
        let Ok(mut state) = vault().lock() else {
            return;
        };
        if !state.unlocked || state.epoch != epoch {
            return;
        }
        let Some(access) = state.access.as_mut().filter(|access| {
            access.token == token && access.expires > Instant::now() && access.decision.is_none()
        }) else {
            return;
        };
        let Some(id) = access.preview_requested.take() else {
            return;
        };
        access.previewing = Some(id.clone());
        access
            .candidates
            .iter()
            .find(|candidate| candidate.id == id)
            .cloned()
    };
    let Some(candidate) = candidate else {
        return;
    };
    let username = provider.username(&candidate, &|| preview_current(epoch, token));
    let Ok(mut state) = vault().lock() else {
        return;
    };
    if !state.unlocked || state.epoch != epoch {
        return;
    }
    let Some(access) = state.access.as_mut().filter(|access| {
        access.token == token && access.expires > Instant::now() && access.decision.is_none()
    }) else {
        return;
    };
    access.previewing = None;
    match username {
        Ok(username) => {
            access
                .usernames
                .insert(candidate.id, username.unwrap_or_default());
        }
        Err(_) => {
            access.username_errors.insert(candidate.id);
        }
    }
}

fn clear_access(token: &str) {
    if let Ok(mut state) = vault().lock() {
        if state
            .access
            .as_ref()
            .is_some_and(|access| access.token == token)
        {
            state.access = None;
        }
    }
}
