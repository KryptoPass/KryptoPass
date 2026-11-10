//! Answers `kryptopass-pinentry` (docs/aprobacion-desde-telefono.md, step 2).
//!
//! A `CONFIRM` waits here until the person decides in the app's dialog; a
//! passphrase is never answered yet, so the pinentry shows its own dialog.
//! Nothing here touches the vault: approvals work while it is locked.

use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use kryptopass_approval::{Decision, Kind, Request};
use serde_json::json;

/// Shorter than the pinentry's own wait, so a missed dialog becomes the local
/// pinentry instead of an error in gpg-agent.
const DECISION_TIMEOUT: Duration = Duration::from_secs(55);

struct Pending {
    id: u64,
    request: Request,
    decision: Option<bool>,
}

#[derive(Default)]
struct State {
    next_id: u64,
    pending: Option<Pending>,
}

fn state() -> &'static (Mutex<State>, Condvar) {
    static STATE: OnceLock<(Mutex<State>, Condvar)> = OnceLock::new();
    STATE.get_or_init(Default::default)
}

/// Starts listening; the returned server stops when dropped.
pub fn start() -> Option<kryptopass_approval::Server> {
    match kryptopass_approval::start(Arc::new(handle)) {
        Ok(server) => {
            tracing::info!("Approvals: listening for kryptopass-pinentry");
            Some(server)
        }
        Err(error) => {
            tracing::warn!(%error, "Approvals: could not listen; the pinentry will use its own dialog");
            None
        }
    }
}

fn handle(request: Request) -> Decision {
    tracing::info!(
        kind = ?request.kind,
        key = request.key.as_deref().unwrap_or("-"),
        owner = request.owner.as_deref().unwrap_or("-"),
        retry = request.retry,
        "Approvals: request from the pinentry"
    );
    if request.kind == Kind::Passphrase {
        tracing::info!("Approvals: no passphrase source yet; the pinentry asks locally");
        return Decision::Unavailable;
    }
    let (lock, changed) = state();
    let Ok(mut guard) = lock.lock() else {
        return Decision::Unavailable;
    };
    if guard.pending.is_some() {
        tracing::info!("Approvals: another request is open; the pinentry asks locally");
        return Decision::Unavailable;
    }
    guard.next_id += 1;
    let id = guard.next_id;
    guard.pending = Some(Pending {
        id,
        request,
        decision: None,
    });
    let deadline = Instant::now() + DECISION_TIMEOUT;
    let decision = loop {
        let decided = guard
            .pending
            .as_ref()
            .filter(|pending| pending.id == id)
            .and_then(|pending| pending.decision);
        if let Some(approve) = decided {
            break Some(approve);
        }
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            break None;
        };
        guard = match changed.wait_timeout(guard, left) {
            Ok((guard, _)) => guard,
            Err(_) => return Decision::Unavailable,
        };
    };
    guard.pending = None;
    changed.notify_all();
    match decision {
        Some(true) => {
            tracing::info!(id, "Approvals: confirmed in the app");
            Decision::Approved
        }
        Some(false) => {
            tracing::info!(id, "Approvals: denied in the app");
            Decision::Denied
        }
        None => {
            tracing::info!(
                id,
                "Approvals: nobody decided in time; the pinentry asks locally"
            );
            Decision::Unavailable
        }
    }
}

/// The open request as JSON for the dialog, or an empty string.
pub fn pending_json() -> String {
    let (lock, _) = state();
    let Ok(guard) = lock.lock() else {
        return String::new();
    };
    let Some(pending) = guard
        .pending
        .as_ref()
        .filter(|pending| pending.decision.is_none())
    else {
        return String::new();
    };
    let request = &pending.request;
    json!({
        "id": pending.id.to_string(),
        "title": request.title.as_deref().map(kryptopass_approval::unescape),
        "description": kryptopass_approval::unescape(&request.description),
        "key": request.key,
        "ssh": request.key.as_deref().is_some_and(|key| key.starts_with("s/")),
        "requester": request.owner.as_deref().and_then(requester),
    })
    .to_string()
}

/// Records the person's answer for the open request with this id.
pub fn decide(id: &str, approve: bool) {
    let (lock, changed) = state();
    let Ok(mut guard) = lock.lock() else {
        return;
    };
    if let Some(pending) = guard
        .pending
        .as_mut()
        .filter(|pending| pending.id.to_string() == id && pending.decision.is_none())
    {
        pending.decision = Some(approve);
        changed.notify_all();
    }
}

/// `owner=<pid>/<uid> <host>` → the process name and pid, when it is local.
fn requester(owner: &str) -> Option<String> {
    let pid: u32 = owner.split(['/', ' ']).next()?.parse().ok()?;
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(format!("{} ({pid})", name.trim()))
}
