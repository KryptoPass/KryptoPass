//! Requests expose only candidate metadata to Qt and release one explicitly selected login.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{persistence, protocol_error, vault};

#[derive(Clone, Serialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub revision: u64,
}

pub struct Login {
    pub username: Zeroizing<String>,
    pub password: Zeroizing<String>,
    pub urls: Vec<Zeroizing<String>>,
}

pub trait CredentialProvider: Send + Sync {
    /// Lists metadata only. Implementations must exclude trash and non-login entries.
    fn candidates(&self) -> Result<Vec<Candidate>>;
    /// Reads only the approved revision, rejecting stale/trash/non-login entries.
    fn login(&self, candidate: &Candidate, still_authorized: &dyn Fn() -> bool) -> Result<Login>;
}

pub(crate) struct Access {
    token: String,
    origin: String,
    candidates: Vec<Candidate>,
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

pub fn pending_access() -> String {
    vault().lock().ok().and_then(|state| {
        state.access.as_ref().filter(|access| access.expires > Instant::now() && access.decision.is_none())
            .map(|access| json!({"token": access.token, "origin": access.origin, "candidates": access.candidates}).to_string())
    }).unwrap_or_default()
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
                } else if access
                    .candidates
                    .iter()
                    .any(|candidate| candidate.id == entry_id)
                {
                    access.decision = Some(entry_id.to_owned());
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

pub(crate) fn get_logins(request: &Value, data: &Value) -> Value {
    match approved_login(data) {
        Ok((candidate, login, hash)) => {
            json!({"success": "true", "hash": hash, "count": "1", "entries": [{
                "uuid": candidate.id, "name": candidate.name, "login": &*login.username, "password": &*login.password,
                "expired": "false", "totp": "", "stringFields": []
            }]})
        }
        Err(_) => protocol_error(
            request,
            6,
            "Login access denied, expired, or selected entry does not match the site",
        ),
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
            expires: Instant::now() + Duration::from_secs(60),
            decision: None,
        });
    }
    let selected = loop {
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
    if let Ok(mut state) = vault().lock() {
        if state
            .access
            .as_ref()
            .is_some_and(|access| access.token == token)
        {
            state.access = None;
        }
    }
    result
}
