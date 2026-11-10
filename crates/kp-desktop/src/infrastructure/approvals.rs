//! Bridges `kryptopass-pinentry` to the paired phone
//! (`docs/aprobacion-desde-telefono.md`).
//!
//! No UI and no vault: the pinentry shows the local dialog itself, and this
//! only forwards the prompt to a connected phone. That keeps it working while
//! the vault is locked and keeps a vault unlock (which goes through gpg-agent
//! and the pinentry) from waiting on itself.

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
    let ask = ApprovalAsk {
        kind: match request.kind {
            Kind::Confirm => ApprovalKind::Confirm,
            Kind::Passphrase => ApprovalKind::Passphrase,
        },
        key_info: request.key.clone().unwrap_or_default(),
        description: kryptopass_approval::unescape(&request.description),
        requester: request.owner.as_deref().and_then(requester).unwrap_or_default(),
        host_name: host_name(),
        expires: PHONE_TIMEOUT,
    };
    let Some(pending) = approvals::ask(ask) else {
        tracing::info!("Approvals: no phone connected; the pinentry asks locally");
        return Decision::Unavailable;
    };
    tracing::info!(phone = %pending.peer(), "Approvals: waiting for the phone");
    let deadline = Instant::now() + PHONE_TIMEOUT;
    loop {
        if let Some(answer) = pending.wait(POLL) {
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
        if !waiting() {
            tracing::info!("Approvals: answered locally first; withdrawing from the phone");
            pending.cancel();
            return Decision::Unavailable;
        }
        if Instant::now() >= deadline {
            tracing::info!("Approvals: the phone did not answer in time");
            pending.cancel();
            return Decision::Unavailable;
        }
    }
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
