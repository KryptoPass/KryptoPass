//! Approval requests to a connected phone (`docs/aprobacion-desde-telefono.md`).
//!
//! Each live session whose peer advertised `approval_version` 1 registers a
//! route here. [`ask`] sends a request through the newest route and returns a
//! [`PendingApproval`] to wait on from any thread. When the session ends, its
//! answer channels close so the bridge can wait for the same phone to reconnect.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kryptopass_identity::DeviceId;
use kryptopass_protocol::{
    APPROVAL_APPROVED, APPROVAL_DENIED, APPROVAL_KIND_CONFIRM, APPROVAL_KIND_PASSPHRASE,
    APPROVAL_PASSPHRASE, APPROVAL_UNAVAILABLE, proto,
};
use tokio::sync::mpsc;
use tokio::time::Instant;
use zeroize::{Zeroize, Zeroizing};

/// Approval extension version this implementation speaks.
pub const APPROVAL_VERSION: u32 = 1;
/// Requests open at once on one session; more are answered "unavailable".
const MAX_OPEN: usize = 4;
const COMMAND_CAPACITY: usize = 8;

/// What the phone is asked.
#[derive(Clone, Debug)]
pub struct ApprovalAsk {
    pub kind: ApprovalKind,
    pub key_info: String,
    pub description: String,
    pub requester: String,
    pub host_name: String,
    pub expires: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalKind {
    Confirm,
    Passphrase,
}

impl ApprovalKind {
    const fn wire(self) -> u32 {
        match self {
            Self::Confirm => APPROVAL_KIND_CONFIRM,
            Self::Passphrase => APPROVAL_KIND_PASSPHRASE,
        }
    }
}

/// The phone's answer. A passphrase is wiped when dropped.
#[derive(Debug, PartialEq, Eq)]
pub enum ApprovalAnswer {
    Approved,
    Denied,
    Passphrase(Zeroizing<Vec<u8>>),
    Unavailable,
}

pub(crate) enum ApprovalCommand {
    Ask {
        id: [u8; 16],
        ask: ApprovalAsk,
        reply: SyncSender<ApprovalAnswer>,
        deadline: Instant,
        cancelled: Arc<AtomicBool>,
    },
    Cancel([u8; 16]),
}

struct Route {
    peer: DeviceId,
    generation: u64,
    commands: mpsc::Sender<ApprovalCommand>,
}

static ROUTES: Mutex<Vec<Route>> = Mutex::new(Vec::new());

/// A request sent to a phone, waiting for its answer.
pub struct PendingApproval {
    id: [u8; 16],
    peer: DeviceId,
    answers: Receiver<ApprovalAnswer>,
    commands: mpsc::Sender<ApprovalCommand>,
    cancelled: Arc<AtomicBool>,
}

impl PendingApproval {
    /// The phone that was asked.
    #[must_use]
    pub const fn peer(&self) -> DeviceId {
        self.peer
    }

    /// Waits up to `timeout`. `Ok(None)` means still waiting; a closed session
    /// returns an error, distinct from the phone explicitly being unavailable.
    pub fn wait(&self, timeout: Duration) -> Result<Option<ApprovalAnswer>, RecvTimeoutError> {
        match self.answers.recv_timeout(timeout) {
            Ok(answer) => Ok(Some(answer)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Withdraws the request; the phone removes its notification.
    pub fn cancel(self) {
        drop(self);
    }
}

impl Drop for PendingApproval {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.commands.try_send(ApprovalCommand::Cancel(self.id));
    }
}

/// Sends `ask` to the most recently connected phone that supports approvals.
/// Once a phone has been asked, pass its identity to keep retries on that phone.
/// `None` when there is none, or its queue is full.
pub fn ask(ask: ApprovalAsk, peer: Option<DeviceId>) -> Option<PendingApproval> {
    let (peer, commands) = {
        let routes = ROUTES.lock().ok()?;
        let route = routes
            .iter()
            .rev()
            .find(|route| peer.is_none_or(|peer| route.peer == peer))?;
        (route.peer, route.commands.clone())
    };
    let mut id = [0_u8; 16];
    while id.iter().all(|byte| *byte == 0) {
        getrandom::fill(&mut id).ok()?;
    }
    let (reply, answers) = sync_channel(1);
    let cancelled = Arc::new(AtomicBool::new(false));
    let deadline = Instant::now() + ask.expires.min(Duration::from_secs(120));
    commands
        .try_send(ApprovalCommand::Ask {
            id,
            ask,
            reply,
            deadline,
            cancelled: cancelled.clone(),
        })
        .ok()?;
    Some(PendingApproval {
        id,
        peer,
        answers,
        commands,
        cancelled,
    })
}

/// Whether some connected phone can answer approvals right now.
#[must_use]
pub fn available() -> bool {
    ROUTES.lock().is_ok_and(|routes| !routes.is_empty())
}

/// Removes the route when the session that registered it ends.
pub(crate) struct RouteGuard {
    peer: DeviceId,
    generation: u64,
}

impl Drop for RouteGuard {
    fn drop(&mut self) {
        if let Ok(mut routes) = ROUTES.lock() {
            routes
                .retain(|route| !(route.peer == self.peer && route.generation == self.generation));
        }
        tracing::info!("Approvals: phone route closed");
    }
}

/// Per-session state, alive while the session runs.
pub(crate) struct ApprovalRuntime {
    pub(crate) commands: Option<mpsc::Receiver<ApprovalCommand>>,
    open: HashMap<[u8; 16], Open>,
    _route: Option<RouteGuard>,
}

struct Open {
    kind: ApprovalKind,
    reply: SyncSender<ApprovalAnswer>,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}

impl ApprovalRuntime {
    /// A route only exists when the peer advertised the extension.
    pub(crate) fn new(peer: DeviceId, generation: u64, remote_version: u32) -> Self {
        if remote_version != APPROVAL_VERSION {
            return Self {
                commands: None,
                open: HashMap::new(),
                _route: None,
            };
        }
        let (sender, receiver) = mpsc::channel(COMMAND_CAPACITY);
        if let Ok(mut routes) = ROUTES.lock() {
            routes.retain(|route| route.peer != peer);
            routes.push(Route {
                peer,
                generation,
                commands: sender,
            });
        }
        tracing::info!("Approvals: phone route open");
        Self {
            commands: Some(receiver),
            open: HashMap::new(),
            _route: Some(RouteGuard { peer, generation }),
        }
    }

    /// Whether the peer may send approval messages at all.
    pub(crate) fn enabled(&self) -> bool {
        self._route.is_some()
    }

    /// Turns a local command into the message to send, if any.
    pub(crate) fn command(
        &mut self,
        command: ApprovalCommand,
    ) -> Option<(proto::MessageType, ApprovalMessage)> {
        match command {
            ApprovalCommand::Ask {
                id,
                ask,
                reply,
                deadline,
                cancelled,
            } => {
                let expires = deadline.saturating_duration_since(Instant::now());
                if cancelled.load(Ordering::Acquire) || expires.as_millis() == 0 {
                    let _ = reply.try_send(ApprovalAnswer::Unavailable);
                    return None;
                }
                if self.open.len() >= MAX_OPEN {
                    tracing::info!("Approvals: too many open requests; unavailable");
                    let _ = reply.try_send(ApprovalAnswer::Unavailable);
                    return None;
                }
                self.open.insert(
                    id,
                    Open {
                        kind: ask.kind,
                        reply,
                        deadline,
                        cancelled,
                    },
                );
                tracing::info!(kind = ?ask.kind, "Approvals: request sent to the phone");
                Some((
                    proto::MessageType::ApprovalRequest,
                    ApprovalMessage::Request(proto::ApprovalRequest {
                        request_id: id.to_vec(),
                        kind: ask.kind.wire(),
                        key_info: truncate(ask.key_info, 128),
                        description: truncate(ask.description, 2048),
                        requester: truncate(ask.requester, 256),
                        host_name: truncate(ask.host_name, 128),
                        expires_in_millis: u32::try_from(expires.as_millis()).unwrap_or(120_000),
                    }),
                ))
            }
            ApprovalCommand::Cancel(id) => {
                self.open.remove(&id)?;
                tracing::info!("Approvals: request withdrawn");
                Some((
                    proto::MessageType::ApprovalCancel,
                    ApprovalMessage::Cancel(proto::ApprovalCancel {
                        request_id: id.to_vec(),
                    }),
                ))
            }
        }
    }

    /// The phone answered. Unknown ids are ignored: they may have expired.
    pub(crate) fn response(&mut self, mut response: proto::ApprovalResponse) {
        let id = <[u8; 16]>::try_from(response.request_id.as_slice()).ok();
        let open = id.and_then(|id| self.open.remove(&id));
        let Some(open) = open else {
            response.passphrase.zeroize();
            tracing::info!("Approvals: answer for an unknown or expired request ignored");
            return;
        };
        if open.cancelled.load(Ordering::Acquire) || Instant::now() >= open.deadline {
            response.passphrase.zeroize();
            let _ = open.reply.try_send(ApprovalAnswer::Unavailable);
            return;
        }
        let answer = match (open.kind, response.decision) {
            (ApprovalKind::Confirm, APPROVAL_APPROVED) => ApprovalAnswer::Approved,
            (_, APPROVAL_DENIED) => ApprovalAnswer::Denied,
            (ApprovalKind::Passphrase, APPROVAL_PASSPHRASE) => {
                ApprovalAnswer::Passphrase(Zeroizing::new(std::mem::take(&mut response.passphrase)))
            }
            (_, APPROVAL_UNAVAILABLE) => ApprovalAnswer::Unavailable,
            (kind, decision) => {
                tracing::warn!(
                    ?kind,
                    decision,
                    "Approvals: answer does not fit the request"
                );
                ApprovalAnswer::Unavailable
            }
        };
        response.passphrase.zeroize();
        tracing::info!(
            answer = match &answer {
                ApprovalAnswer::Approved => "approved",
                ApprovalAnswer::Denied => "denied",
                ApprovalAnswer::Passphrase(_) => "passphrase",
                ApprovalAnswer::Unavailable => "unavailable",
            },
            "Approvals: phone answered"
        );
        let _ = open.reply.try_send(answer);
    }

    /// The phone withdrew a request (expired or dismissed there).
    pub(crate) fn remote_cancel(&mut self, cancel: &proto::ApprovalCancel) {
        let id = <[u8; 16]>::try_from(cancel.request_id.as_slice()).ok();
        if let Some(open) = id.and_then(|id| self.open.remove(&id)) {
            tracing::info!("Approvals: the phone withdrew a request");
            let _ = open.reply.try_send(ApprovalAnswer::Unavailable);
        }
    }

    /// Drops requests past their deadline; the waiting side gets "unavailable".
    pub(crate) fn expire(&mut self) {
        let now = Instant::now();
        self.open.retain(|_, open| {
            let alive = open.deadline > now;
            if !alive {
                let _ = open.reply.try_send(ApprovalAnswer::Unavailable);
            }
            alive
        });
    }

    /// The earliest deadline, to wake the session loop for [`Self::expire`].
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.open.values().map(|open| open.deadline).min()
    }
}

pub(crate) enum ApprovalMessage {
    Request(proto::ApprovalRequest),
    Cancel(proto::ApprovalCancel),
}

pub(crate) async fn receive_command(
    commands: &mut Option<mpsc::Receiver<ApprovalCommand>>,
) -> Option<ApprovalCommand> {
    match commands {
        Some(commands) => commands.recv().await,
        None => std::future::pending().await,
    }
}

/// Cuts at a character boundary so the field stays valid UTF-8.
fn truncate(mut text: String, maximum: usize) -> String {
    if text.len() > maximum {
        let mut end = maximum;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}
