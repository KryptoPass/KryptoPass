//! Bridges `kryptopass-pinentry` to the paired phone
//! (`docs/aprobacion-desde-telefono.md`).
//!
//! No UI and no vault: the pinentry shows the local dialog itself, and this
//! forwards the prompt when a phone connects before the prompt expires. This
//! works while the vault is locked and keeps a vault unlock (which goes through
//! gpg-agent and the pinentry) from waiting on itself.

use std::sync::Arc;
use std::time::{Duration, Instant};

use kryptopass_approval::{Decision, Kind, Request};
use kryptopass_devices::approvals::{self, ApprovalAnswer, ApprovalAsk, ApprovalKind};

/// Shorter than the pinentry's own wait, so the phone's notification is gone
/// before the pinentry gives up on it.
const PHONE_TIMEOUT: Duration = Duration::from_secs(55);
/// How often to check that the pinentry is still waiting.
const POLL: Duration = Duration::from_millis(200);

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

fn handle(request: Request, waiting: &dyn Fn() -> bool) -> Decision {
    tracing::info!(
        kind = ?request.kind,
        key = request.key.as_deref().unwrap_or("-"),
        owner = request.owner.as_deref().unwrap_or("-"),
        retry = request.retry,
        "Approvals: request from the pinentry"
    );
    if request.retry {
        tracing::info!("Approvals: a retry is always answered locally");
        return Decision::Unavailable;
    }
    // The pinentry already refuses these; whoever else connects gets the same.
    if request.kind == Kind::Passphrase && !request.key.as_deref().is_some_and(is_keygrip) {
        tracing::info!("Approvals: a passphrase without a keygrip is never asked remotely");
        return Decision::Unavailable;
    }
    let mut ask = ApprovalAsk {
        kind: match request.kind {
            Kind::Confirm => ApprovalKind::Confirm,
            Kind::Passphrase => ApprovalKind::Passphrase,
        },
        key_info: request.key.clone().unwrap_or_default(),
        description: kryptopass_approval::unescape(&request.description),
        requester: request
            .owner
            .as_deref()
            .and_then(requester)
            .unwrap_or_default(),
        host_name: host_name(),
        expires: PHONE_TIMEOUT,
    };
    let deadline = Instant::now() + PHONE_TIMEOUT;
    let mut pending: Option<approvals::PendingApproval> = None;
    let mut peer = None;
    tracing::info!("Approvals: waiting for a phone connection or answer");
    loop {
        if !waiting() {
            tracing::info!("Approvals: answered locally first; withdrawing from the phone");
            return Decision::Unavailable;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.as_millis() == 0 {
            tracing::info!("Approvals: the phone did not answer in time");
            return Decision::Unavailable;
        }
        if pending.is_none() {
            // Reconnection gets a fresh session request with only the original
            // time remaining; a request already sent never moves to another phone.
            ask.expires = remaining;
            pending = approvals::ask(ask.clone(), peer);
            if let Some(active) = &pending {
                peer = Some(active.peer());
                tracing::info!(phone = %active.peer(), "Approvals: waiting for the phone");
            }
        }
        let Some(active) = &pending else {
            std::thread::sleep(POLL.min(remaining));
            continue;
        };
        let answer = match active.wait(POLL.min(remaining)) {
            Ok(Some(answer)) => answer,
            Ok(None) => continue,
            Err(_) => {
                pending = None;
                tracing::info!("Approvals: connection lost; waiting for the same phone");
                std::thread::sleep(POLL.min(remaining));
                continue;
            }
        };
        // The local dialog can finish while we are waiting for the phone.
        if !waiting() || Instant::now() >= deadline {
            return Decision::Unavailable;
        }
        return match answer {
            ApprovalAnswer::Approved => {
                tracing::info!("Approvals: approved on the phone");
                Decision::Approved
            }
            ApprovalAnswer::Denied => {
                tracing::info!("Approvals: denied on the phone");
                Decision::Denied
            }
            ApprovalAnswer::Passphrase(secret) => {
                tracing::info!("Approvals: passphrase from the phone");
                Decision::Passphrase(secret)
            }
            ApprovalAnswer::Unavailable => {
                tracing::info!("Approvals: the phone cannot answer");
                Decision::Unavailable
            }
        };
    }
}

/// `n/<keygrip>` or `s/<keygrip>` (SSH): 40 hex digits.
fn is_keygrip(key: &str) -> bool {
    key.strip_prefix("n/")
        .or_else(|| key.strip_prefix("s/"))
        .is_some_and(|grip| grip.len() == 40 && grip.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// `owner=<pid>/<uid> <host>` → the process name and pid, when it is local.
fn requester(owner: &str) -> Option<String> {
    let pid: u32 = owner.split(['/', ' ']).next()?.parse().ok()?;
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(format!("{} ({pid})", name.trim()))
}

fn host_name() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_owned())
        .unwrap_or_default()
}
