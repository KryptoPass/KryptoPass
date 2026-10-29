//! Process-lifetime owner for the durable LAN resume listener.
//!
//! The public DeviceId in signaling is only a selector. Each request is checked
//! against SQLite before WebRTC and checked again by `PeerHost` before a new
//! Noise XX reconnect. The owner is independent of the Qt dialog lifecycle.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration as StdDuration, Instant as StdInstant},
};

use kryptopass_devices::{
    ConnectionState, DeviceDirectory, DeviceDirectorySnapshot, SessionCloseReason, SessionEvent,
    SessionHandle, SessionOptions, SyntheticKeyTransferSend, SyntheticTransferControl,
    SyntheticTransferError, SyntheticTransferProgress,
};
use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_lan::{
    NetworkPolicyError, NetworkPolicyGuard, NetworkUse, ResumeAttempt, ResumeListener,
};
use tokio::{
    runtime::Builder,
    sync::{broadcast, mpsc, oneshot, watch},
    time::{Duration, Instant, interval, timeout},
};

#[path = "internet_resume.rs"]
mod internet;

const MAX_OWNED_SESSIONS: usize = 16;
const NEGOTIATION_FAILURE_LIMIT: usize = 10;
const NEGOTIATION_FAILURE_WINDOW: StdDuration = StdDuration::from_secs(300);
const FAILURE_COOLDOWN: StdDuration = StdDuration::from_secs(300);
const LISTENER_RETRY_INTERVAL: Duration = Duration::from_secs(30);
const MDNS_REFRESH_INTERVAL: Duration = Duration::from_secs(15);
const SNAPSHOT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const PENDING_ATTEMPT_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const SESSION_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Ready profile handles needed by the UI and outgoing pairing controller.
#[derive(Clone)]
pub struct ResumeOwnerReady {
    /// Persisted authorized peer identities.
    pub trusted_peers: Vec<PublicIdentity>,
    /// Shared observable directory used by both incoming and outgoing sessions.
    pub devices: DeviceDirectory,
    /// Durable LAN resume port advertised only inside authenticated SessionHello.
    /// Zero means the profile opened but the listener could not bind.
    pub reconnect_port: u16,
}

/// Latest process-owner state. A watch channel coalesces UI updates and bounds
/// notification memory even when the GUI event queue is busy.
#[derive(Clone, Default)]
pub struct ResumeOwnerSnapshot {
    /// Current profile and trust state, once an existing profile is available.
    pub ready: Option<ResumeOwnerReady>,
    /// Most recent real DeviceDirectory snapshot.
    pub devices: Option<Arc<DeviceDirectorySnapshot>>,
    /// Set when the existing profile could not be read; no replacement is made.
    pub profile_error: bool,
    /// Coalesced, secret-free synthetic transfer progress by trusted peer.
    pub synthetic_transfers: BTreeMap<DeviceId, (u64, SyntheticTransferProgress)>,
}

/// Handle to the process-owned listener and inbound session supervisor.
#[derive(Clone)]
pub struct ResumeProcessOwner {
    inner: Arc<ResumeOwnerInner>,
}

struct ResumeOwnerInner {
    commands: mpsc::Sender<OwnerCommand>,
    shutdown: watch::Sender<bool>,
    policy: Arc<Mutex<AcceptPolicy>>,
    finished: Arc<AtomicBool>,
    snapshots: watch::Sender<ResumeOwnerSnapshot>,
}

#[derive(Default)]
struct AcceptPolicy {
    suppressed: HashSet<DeviceId>,
    active: HashSet<DeviceId>,
    negotiating: HashSet<DeviceId>,
    failures: HashMap<DeviceId, VecDeque<StdInstant>>,
    cooling_until: HashMap<DeviceId, StdInstant>,
}

enum OwnerCommand {
    EnsureReady(oneshot::Sender<Result<ResumeOwnerReady, super::PeerError>>),
    RefreshTrust(oneshot::Sender<Result<ResumeOwnerReady, super::PeerError>>),
    RefreshNetworkPolicy,
    Suppress(DeviceId),
    Renegotiate(DeviceId),
    Allow(DeviceId),
    SessionStable(DeviceId, u64),
    SessionEnded(DeviceId, u64),
    RegisterSyntheticTransfer(
        SyntheticTransferControl,
        oneshot::Sender<Result<(), SyntheticTransferError>>,
    ),
    StartSyntheticTransfer(
        DeviceId,
        u64,
        Box<SyntheticKeyTransferSend>,
        oneshot::Sender<Result<(), SyntheticTransferError>>,
    ),
    CancelSyntheticTransfer(
        DeviceId,
        u64,
        [u8; 16],
        oneshot::Sender<Result<(), SyntheticTransferError>>,
    ),
    SyntheticTransferProgressChanged(DeviceId, u64, SyntheticTransferProgress),
    UnregisterSyntheticTransfer(DeviceId, u64),
    SyntheticTransferControlFor(
        DeviceId,
        u64,
        oneshot::Sender<Option<SyntheticTransferControl>>,
    ),
    IncomingFinished(
        DeviceId,
        u64,
        Box<Result<kryptopass_pairing::EstablishedSession<super::HostTransport>, AttemptFailure>>,
    ),
    RelayIncoming(Box<ResumeAttempt>),
    Shutdown,
}

#[derive(Clone, Copy)]
enum AttemptFailure {
    Cancelled,
    Failed,
}

struct PendingAttempt {
    id: u64,
    cancel: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

struct OwnedSession {
    handle: SessionHandle,
}

/// Gives the Sync binding of a peer when a reconnected session starts, or
/// `None` to run it without Sync. Called on the owner's thread.
pub type SyncBinder =
    Arc<dyn Fn(DeviceId) -> Option<kryptopass_devices::SyncSessionBinding> + Send + Sync>;

struct OwnerRuntime {
    directory_path: PathBuf,
    sync: SyncBinder,
    host: Option<Arc<super::PeerHost>>,
    devices: Option<DeviceDirectory>,
    ready: Option<ResumeOwnerReady>,
    listener: Option<Arc<ResumeListener>>,
    sessions: HashMap<DeviceId, OwnedSession>,
    synthetic_controls: HashMap<DeviceId, SyntheticTransferControl>,
    synthetic_transfers: BTreeMap<DeviceId, (u64, SyntheticTransferProgress)>,
    pending_attempts: HashMap<DeviceId, PendingAttempt>,
    next_attempt_id: u64,
    last_snapshot_revision: u64,
    last_listener_bind: Option<Instant>,
    command_sender: mpsc::Sender<OwnerCommand>,
    relay_workers: HashMap<DeviceId, internet::RelayWorker>,
    network_settings: Option<super::NetworkPrivacySettings>,
    network_settings_error: bool,
    network_guard: Option<NetworkPolicyGuard>,
    route_snapshot_unavailable: bool,
}

impl ResumeProcessOwner {
    /// Subscribe to coalesced profile and DeviceDirectory changes.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<ResumeOwnerSnapshot> {
        self.inner.snapshots.subscribe()
    }

    /// Opens the existing profile, binds the process listener if possible, and
    /// returns its shared directory. This method never creates or repairs data.
    pub async fn ensure_ready(&self) -> Result<ResumeOwnerReady, super::PeerError> {
        let (reply, response) = oneshot::channel();
        self.inner
            .commands
            .send(OwnerCommand::EnsureReady(reply))
            .await
            .map_err(|_| super::PeerError::Profile)?;
        response.await.map_err(|_| super::PeerError::Profile)?
    }

    /// Reloads exact trust records from SQLite after a successful pairing.
    pub async fn refresh_trust(&self) -> Result<ResumeOwnerReady, super::PeerError> {
        let (reply, response) = oneshot::channel();
        self.inner
            .commands
            .send(OwnerCommand::RefreshTrust(reply))
            .await
            .map_err(|_| super::PeerError::Profile)?;
        response.await.map_err(|_| super::PeerError::Profile)?
    }

    /// Registers the cloneable capability for an outgoing session, leaving
    /// transport ownership with the original session worker.
    pub async fn register_synthetic_transfer_control(
        &self,
        control: SyntheticTransferControl,
    ) -> Result<(), SyntheticTransferError> {
        let (reply, response) = oneshot::channel();
        self.inner
            .commands
            .send(OwnerCommand::RegisterSyntheticTransfer(control, reply))
            .await
            .map_err(|_| SyntheticTransferError::WrongSession)?;
        response
            .await
            .map_err(|_| SyntheticTransferError::WrongSession)?
    }

    /// Routes an approved preparation to the exact active peer generation.
    pub async fn start_synthetic_key_transfer(
        &self,
        peer: DeviceId,
        expected_generation: u64,
        prepared: SyntheticKeyTransferSend,
    ) -> Result<(), SyntheticTransferError> {
        let (reply, response) = oneshot::channel();
        self.inner
            .commands
            .send(OwnerCommand::StartSyntheticTransfer(
                peer,
                expected_generation,
                Box::new(prepared),
                reply,
            ))
            .await
            .map_err(|_| SyntheticTransferError::WrongSession)?;
        response
            .await
            .map_err(|_| SyntheticTransferError::WrongSession)?
    }

    /// Queues cancellation before disconnect/forget commands when called by
    /// the local UI. The actor still checks peer, generation and transfer ID.
    pub async fn cancel_synthetic_key_transfer(
        &self,
        peer: DeviceId,
        expected_generation: u64,
        transfer_id: [u8; 16],
    ) -> Result<(), SyntheticTransferError> {
        let (reply, response) = oneshot::channel();
        self.inner
            .commands
            .send(OwnerCommand::CancelSyntheticTransfer(
                peer,
                expected_generation,
                transfer_id,
                reply,
            ))
            .await
            .map_err(|_| SyntheticTransferError::WrongSession)?;
        response
            .await
            .map_err(|_| SyntheticTransferError::WrongSession)?
    }

    /// Returns the scoped transfer capability of the exact peer generation, used
    /// to answer and complete an inbound key offer from that phone.
    pub async fn synthetic_transfer_control(
        &self,
        peer: DeviceId,
        expected_generation: u64,
    ) -> Option<SyntheticTransferControl> {
        let (reply, response) = oneshot::channel();
        self.inner
            .commands
            .send(OwnerCommand::SyntheticTransferControlFor(
                peer,
                expected_generation,
                reply,
            ))
            .await
            .ok()?;
        response.await.ok().flatten()
    }

    /// Drops a closed outgoing session's route while retaining its last
    /// coalesced progress for the UI until a new generation replaces it.
    pub fn unregister_synthetic_transfer_control(&self, peer: DeviceId, generation: u64) {
        let _ = self
            .inner
            .commands
            .try_send(OwnerCommand::UnregisterSyntheticTransfer(peer, generation));
    }

    /// Notify the process owner that local privacy settings were persisted.
    pub fn refresh_network_policy(&self) {
        let _ = self
            .inner
            .commands
            .try_send(OwnerCommand::RefreshNetworkPolicy);
    }

    /// Suppresses future inbound reconnects immediately and drains any
    /// process-owned session for this peer. Used after explicit local disconnect.
    pub fn suppress_peer(&self, peer: DeviceId) {
        if let Ok(mut policy) = self.inner.policy.lock() {
            policy.suppressed.insert(peer);
        }
        let _ = self.inner.commands.try_send(OwnerCommand::Suppress(peer));
    }

    /// Ends the live session with `peer` as superseded so the phone reconnects at once and
    /// the new session reads the current Sync binding. Used after Sync is turned on or off,
    /// because a session only learns about its binding when it starts.
    pub fn renegotiate_peer(&self, peer: DeviceId) {
        let _ = self
            .inner
            .commands
            .try_send(OwnerCommand::Renegotiate(peer));
    }

    /// Clears explicit-disconnect suppression after the user chooses Reconnect.
    pub fn allow_peer(&self, peer: DeviceId) {
        if let Ok(mut policy) = self.inner.policy.lock() {
            policy.suppressed.remove(&peer);
        }
        let _ = self.inner.commands.try_send(OwnerCommand::Allow(peer));
    }

    /// Requests listener/session shutdown and waits for all owned sessions to
    /// drain. The caller must not forget or reset trust unless this returns true.
    pub fn shutdown_and_wait(&self, timeout: StdDuration) -> bool {
        let _ = self.inner.commands.try_send(OwnerCommand::Shutdown);
        self.inner.shutdown.send_replace(true);
        let deadline = StdInstant::now() + timeout;
        while !self.inner.finished.load(Ordering::Acquire) {
            if StdInstant::now() >= deadline {
                return false;
            }
            thread::sleep(StdDuration::from_millis(20));
        }
        true
    }

    /// Stops the listener without blocking the caller. Process exit also drops
    /// this handle and signals the same cancellation watch.
    pub fn shutdown(&self) {
        let _ = self.inner.commands.try_send(OwnerCommand::Shutdown);
        self.inner.shutdown.send_replace(true);
    }
}

/// Start one process-scoped owner. Existing profiles are opened read-only with
/// respect to identity creation; a newly initialized profile is activated by
/// the explicit pairing worker calling `ensure_ready` after initialization.
pub fn spawn_resume_process_owner(
    directory: PathBuf,
    sync: SyncBinder,
) -> Result<ResumeProcessOwner, super::PeerError> {
    let (commands, receiver) = mpsc::channel(32);
    let (shutdown, cancelled) = watch::channel(false);
    let (snapshots, _snapshot_receiver) = watch::channel(ResumeOwnerSnapshot::default());
    let policy = Arc::new(Mutex::new(AcceptPolicy::default()));
    let finished = Arc::new(AtomicBool::new(false));
    let thread_policy = Arc::clone(&policy);
    let thread_finished = Arc::clone(&finished);
    let thread_snapshots = snapshots.clone();
    let thread_commands = commands.clone();
    let thread_last_selector = Arc::new(Mutex::new(None));
    thread::Builder::new()
        .name("kp-lan-resume-owner".to_owned())
        .spawn(move || {
            let runtime = Builder::new_current_thread().enable_all().build();
            if let Ok(runtime) = runtime {
                runtime.block_on(run_owner(
                    OwnerProfile { directory, sync },
                    receiver,
                    cancelled,
                    thread_policy,
                    thread_snapshots,
                    thread_commands,
                    thread_last_selector,
                ));
            }
            thread_finished.store(true, Ordering::Release);
        })
        .map_err(|_| super::PeerError::Link)?;
    Ok(ResumeProcessOwner {
        inner: Arc::new(ResumeOwnerInner {
            commands,
            shutdown,
            policy,
            finished,
            snapshots,
        }),
    })
}

impl ResumeOwnerInner {
    fn stop(&self) {
        let _ = self.commands.try_send(OwnerCommand::Shutdown);
        self.shutdown.send_replace(true);
    }
}

impl Drop for ResumeOwnerInner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The profile an owner serves and how it binds Sync for its sessions.
struct OwnerProfile {
    directory: PathBuf,
    sync: SyncBinder,
}

async fn run_owner(
    profile: OwnerProfile,
    mut commands: mpsc::Receiver<OwnerCommand>,
    mut cancelled: watch::Receiver<bool>,
    policy: Arc<Mutex<AcceptPolicy>>,
    snapshots: watch::Sender<ResumeOwnerSnapshot>,
    command_sender: mpsc::Sender<OwnerCommand>,
    last_selector: Arc<Mutex<Option<DeviceId>>>,
) {
    let mut state = OwnerRuntime {
        directory_path: profile.directory,
        sync: profile.sync,
        host: None,
        devices: None,
        ready: None,
        listener: None,
        sessions: HashMap::new(),
        synthetic_controls: HashMap::new(),
        synthetic_transfers: BTreeMap::new(),
        pending_attempts: HashMap::new(),
        next_attempt_id: 1,
        last_snapshot_revision: 0,
        last_listener_bind: None,
        command_sender,
        relay_workers: HashMap::new(),
        network_settings: None,
        network_settings_error: false,
        network_guard: None,
        route_snapshot_unavailable: false,
    };
    let mut interface_refresh = interval(MDNS_REFRESH_INTERVAL);
    let mut listener_retry = interval(LISTENER_RETRY_INTERVAL);
    let mut snapshot_poll = interval(SNAPSHOT_POLL_INTERVAL);
    let mut network_policy_poll = interval(Duration::from_millis(250));
    interface_refresh.tick().await;
    listener_retry.tick().await;
    snapshot_poll.tick().await;
    network_policy_poll.tick().await;
    state.reload_network_policy();

    // Startup is read-only. If the directory is missing, wait for an explicit
    // pairing operation to initialize it and then call ensure_ready.
    let profile_error = match super::PeerHost::system_profile_needs_pairing(&state.directory_path) {
        Ok(true) => false,
        Ok(false) => state.ensure_ready(&snapshots).await.is_err(),
        Err(_) => true,
    };
    if profile_error {
        snapshots.send_modify(|snapshot| snapshot.profile_error = true);
    }

    state.sync_relay_workers(&policy);
    loop {
        let accept_blocked = state.listener.is_none() || !state.pending_attempts.is_empty();
        let accept = if accept_blocked {
            Box::pin(std::future::pending())
        } else {
            state.accept_future(Arc::clone(&policy), Arc::clone(&last_selector))
        };
        tokio::pin!(accept);
        let mut accepted_result = None;
        loop {
            tokio::select! {
                _ = cancelled.changed() => {
                    if *cancelled.borrow() {
                        break;
                    }
                }
                command = commands.recv() => {
                    let Some(command) = command else { break; };
                    if state.handle_command(command, &snapshots, &policy).await {
                        break;
                    }
                    if state.reload_network_policy() || state.network_route_invalidated() {
                        state.reconfigure_network(&snapshots, &policy, &last_selector).await;
                        break;
                    }
                    state.sync_relay_workers(&policy);
                    let selected = last_selector
                        .lock()
                        .ok()
                        .and_then(|selector| *selector);
                    let suppressed_during_negotiation = selected.filter(|peer| {
                        policy
                            .lock()
                            .is_ok_and(|current| current.suppressed.contains(peer))
                    });
                    if let Some(peer) = suppressed_during_negotiation {
                        if let Ok(mut selector) = last_selector.lock()
                            && *selector == Some(peer)
                        {
                            *selector = None;
                        }
                        // A local disconnect also cancels an already-authorized
                        // signaling exchange before it can finish WebRTC.
                        break;
                    }
                    if accept_blocked && state.listener.is_some() && state.pending_attempts.is_empty() {
                        break;
                    }
                }
                result = &mut accept => {
                    accepted_result = Some(result);
                    break;
                }
                _ = interface_refresh.tick() => {
                    state.sync_relay_workers(&policy);
                    if let (Some(listener), Some(host)) = (state.listener.as_ref(), state.host.as_ref()) {
                        listener.refresh_mdns(*host.public_identity().device_id.as_bytes());
                    }
                }
                _ = listener_retry.tick() => {
                    state.retry_listener().await;
                    state.publish_ready(&snapshots);
                    if accept_blocked && state.listener.is_some() && state.pending_attempts.is_empty() {
                        break;
                    }
                }
                _ = snapshot_poll.tick() => {
                    if let Some(devices) = state.devices.as_ref() {
                        let snapshot = devices.snapshot();
                        if snapshot.revision != state.last_snapshot_revision {
                            state.last_snapshot_revision = snapshot.revision;
                            snapshots.send_modify(|value| {
                                value.ready = state.ready.clone();
                                value.devices = Some(snapshot);
                                value.synthetic_transfers = state.synthetic_transfers.clone();
                            });
                        }
                    }
                }
                _ = network_policy_poll.tick() => {
                    if state.reload_network_policy() || state.network_route_invalidated() {
                        state.reconfigure_network(&snapshots, &policy, &last_selector).await;
                        break;
                    }
                }
            }
            if *cancelled.borrow() {
                break;
            }
        }
        if *cancelled.borrow() {
            break;
        }
        if let Some(Ok(attempt)) = accepted_result {
            if let Ok(mut selector) = last_selector.lock() {
                let _ = selector.take();
            }
            state.start_attempt(attempt, &policy);
        } else if let Some(Err(error)) = accepted_result {
            state.record_failed_selector(error, &policy, &last_selector);
        }
    }

    state.drain_relay_workers().await;
    for pending in state.pending_attempts.values() {
        let _ = pending.cancel.send(true);
    }
    // Reject future completion messages before draining tasks. A completed
    // handshake must close its session itself if it can no longer hand it to
    // this actor, and cannot block behind a full queue during shutdown.
    commands.close();
    while let Ok(command) = commands.try_recv() {
        if let OwnerCommand::IncomingFinished(_, _, result) = command {
            if let Ok(mut session) = *result {
                let _ = session.close().await;
            }
        }
    }
    for (_, pending) in state.pending_attempts.drain() {
        let mut task = pending.task;
        if timeout(PENDING_ATTEMPT_DRAIN_TIMEOUT, &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }
    let mut drains = tokio::task::JoinSet::new();
    for (_, mut session) in state.sessions.drain() {
        let _ = session.handle.disconnect(SessionCloseReason::AppClosing);
        drains.spawn(async move {
            let _ = timeout(SESSION_DRAIN_TIMEOUT, session.handle.wait_closed()).await;
        });
    }
    while drains.join_next().await.is_some() {}
    state.listener = None;
    snapshots.send_modify(|snapshot| {
        snapshot.ready = None;
        snapshot.devices = None;
    });
}

impl OwnerRuntime {
    fn reload_network_policy(&mut self) -> bool {
        let loaded = super::load_network_privacy_settings(&self.directory_path);
        let (settings, load_error) = match loaded {
            Ok(settings) => (Some(settings), false),
            Err(_) => (None, true),
        };
        let changed =
            self.network_settings != settings || self.network_settings_error != load_error;
        if changed {
            if let Some(previous) = self.network_guard.as_ref() {
                previous.invalidate();
            }
            self.network_settings = settings;
            self.network_settings_error = load_error;
            self.network_guard = settings.map(|settings| NetworkPolicyGuard::new(settings.policy));
            self.route_snapshot_unavailable = false;
        }
        changed
    }

    fn network_route_invalidated(&mut self) -> bool {
        let error = self
            .network_guard
            .as_ref()
            .and_then(|guard| guard.ensure_current().err());
        match error {
            Some(NetworkPolicyError::RouteSnapshotUnavailable) => {
                let changed = !self.route_snapshot_unavailable;
                self.route_snapshot_unavailable = true;
                changed
            }
            Some(_) => {
                self.route_snapshot_unavailable = false;
                true
            }
            None => {
                let recovered = self.route_snapshot_unavailable;
                self.route_snapshot_unavailable = false;
                recovered
            }
        }
    }

    async fn reconfigure_network(
        &mut self,
        snapshots: &watch::Sender<ResumeOwnerSnapshot>,
        policy: &Arc<Mutex<AcceptPolicy>>,
        last_selector: &Arc<Mutex<Option<DeviceId>>>,
    ) {
        // A route change invalidates the prior generation. A settings change
        // already replaced the guard in reload_network_policy.
        if self.network_route_invalidated() {
            if let Some(previous) = self.network_guard.take() {
                previous.invalidate();
            }
            self.network_guard = self
                .network_settings
                .map(|settings| NetworkPolicyGuard::new(settings.policy));
            self.route_snapshot_unavailable = false;
        }
        self.listener = None;
        if let Some(ready) = self.ready.as_mut() {
            ready.reconnect_port = 0;
        }
        self.drain_relay_workers().await;

        let pending = std::mem::take(&mut self.pending_attempts);
        for attempt in pending.values() {
            let _ = attempt.cancel.send(true);
        }
        let mut pending_drains = tokio::task::JoinSet::new();
        for (_, attempt) in pending {
            let mut task = attempt.task;
            pending_drains.spawn(async move {
                if timeout(PENDING_ATTEMPT_DRAIN_TIMEOUT, &mut task)
                    .await
                    .is_err()
                {
                    task.abort();
                    let _ = task.await;
                }
            });
        }
        while pending_drains.join_next().await.is_some() {}
        if let Ok(mut selector) = last_selector.lock() {
            *selector = None;
        }

        // Each WebRTC transport shares the invalidated guard and closes
        // locally without trying to reconnect on another interface.
        let sessions = std::mem::take(&mut self.sessions);
        let mut session_drains = tokio::task::JoinSet::new();
        for (_, mut session) in sessions {
            session_drains.spawn(async move {
                let _ = timeout(SESSION_DRAIN_TIMEOUT, session.handle.wait_closed()).await;
            });
        }
        while session_drains.join_next().await.is_some() {}
        if let Ok(mut current) = policy.lock() {
            current.active.clear();
            current.negotiating.clear();
        }

        if self.host.is_some() {
            self.bind_listener().await;
        }
        self.sync_relay_workers(policy);
        self.publish_ready(snapshots);
    }

    async fn ensure_ready(
        &mut self,
        snapshots: &watch::Sender<ResumeOwnerSnapshot>,
    ) -> Result<ResumeOwnerReady, super::PeerError> {
        if self.ready.is_some() {
            if self.listener.is_none() {
                self.bind_listener().await;
                self.publish_ready(snapshots);
            }
            return self.ready.clone().ok_or(super::PeerError::Profile);
        }
        let host = Arc::new(super::PeerHost::open_existing_system_profile(
            &self.directory_path,
        )?);
        let peers = host.trusted_peers()?;
        let local_id = host.public_identity().device_id;
        let devices = DeviceDirectory::new(local_id);
        devices
            .replace_trusted(peers.clone())
            .map_err(|_| super::PeerError::Session)?;
        self.host = Some(Arc::clone(&host));
        self.devices = Some(devices.clone());
        self.last_snapshot_revision = devices.snapshot().revision;
        self.ready = Some(ResumeOwnerReady {
            trusted_peers: peers,
            devices,
            reconnect_port: 0,
        });
        self.bind_listener().await;
        let ready = self.ready.clone().ok_or(super::PeerError::Profile)?;
        snapshots.send_replace(ResumeOwnerSnapshot {
            ready: Some(ready.clone()),
            devices: Some(ready.devices.snapshot()),
            profile_error: false,
            synthetic_transfers: self.synthetic_transfers.clone(),
        });
        Ok(ready)
    }

    async fn refresh_trust(
        &mut self,
        snapshots: &watch::Sender<ResumeOwnerSnapshot>,
    ) -> Result<ResumeOwnerReady, super::PeerError> {
        let host = self
            .host
            .as_ref()
            .cloned()
            .ok_or(super::PeerError::Profile)?;
        let devices = self
            .devices
            .as_ref()
            .cloned()
            .ok_or(super::PeerError::Profile)?;
        if self.listener.is_none() {
            self.bind_listener().await;
        }
        let peers = host.trusted_peers()?;
        devices
            .replace_trusted(peers.clone())
            .map_err(|_| super::PeerError::Session)?;
        let ready = ResumeOwnerReady {
            trusted_peers: peers,
            devices,
            reconnect_port: self.listener.as_ref().map_or(0, |listener| listener.port()),
        };
        self.ready = Some(ready.clone());
        self.last_snapshot_revision = ready.devices.snapshot().revision;
        snapshots.send_replace(ResumeOwnerSnapshot {
            ready: Some(ready.clone()),
            devices: Some(ready.devices.snapshot()),
            profile_error: false,
            synthetic_transfers: self.synthetic_transfers.clone(),
        });
        Ok(ready)
    }

    async fn bind_listener(&mut self) {
        let Some(host) = self.host.as_ref() else {
            tracing::info!(reason = "no_profile", "LAN resume listener not started");
            return;
        };
        let Some(settings) = self.network_settings else {
            tracing::warn!(
                reason = "privacy_settings_unavailable",
                "LAN resume listener not started"
            );
            self.listener = None;
            if let Some(ready) = self.ready.as_mut() {
                ready.reconnect_port = 0;
            }
            return;
        };
        let Some(guard) = self.network_guard.as_ref().cloned() else {
            tracing::warn!(
                reason = "no_network_guard",
                "LAN resume listener not started"
            );
            self.listener = None;
            if let Some(ready) = self.ready.as_mut() {
                ready.reconnect_port = 0;
            }
            return;
        };
        if let Err(error) = guard.preflight(NetworkUse::Lan) {
            tracing::warn!(reason = ?error, "LAN resume listener blocked by network policy");
            self.listener = None;
            if let Some(ready) = self.ready.as_mut() {
                ready.reconnect_port = 0;
            }
            return;
        }
        let assigned = kryptopass_lan::local_addresses().unwrap_or_default();
        let Some(bind_address) = kryptopass_lan::select_local_bind_address(
            settings.lan_bind_ip,
            &assigned,
            kryptopass_lan::os_selected_local_address(),
        ) else {
            tracing::warn!(
                reason = "no_lan_address",
                assigned = assigned.len(),
                "LAN resume listener not started"
            );
            self.listener = None;
            if let Some(ready) = self.ready.as_mut() {
                ready.reconnect_port = 0;
            }
            return;
        };
        let identity = host.public_identity();
        self.last_listener_bind = Some(Instant::now());
        match ResumeListener::bind_with_privacy(*identity.device_id.as_bytes(), bind_address, guard)
            .await
        {
            Ok(listener) => {
                let port = listener.port();
                self.listener = Some(Arc::new(listener));
                if let Some(ready) = self.ready.as_mut() {
                    ready.reconnect_port = port;
                }
            }
            Err(error) => {
                tracing::warn!(reason = ?error, "LAN resume listener bind failed");
                self.listener = None;
                if let Some(ready) = self.ready.as_mut() {
                    ready.reconnect_port = 0;
                }
            }
        }
    }

    async fn retry_listener(&mut self) {
        if self.listener.is_some() || self.host.is_none() {
            return;
        }
        if self
            .last_listener_bind
            .is_some_and(|last| last.elapsed() < LISTENER_RETRY_INTERVAL)
        {
            return;
        }
        self.bind_listener().await;
    }

    fn publish_ready(&self, snapshots: &watch::Sender<ResumeOwnerSnapshot>) {
        if let (Some(ready), Some(devices)) = (self.ready.as_ref(), self.devices.as_ref()) {
            let snapshot = devices.snapshot();
            snapshots.send_modify(|value| {
                value.ready = Some(ready.clone());
                value.devices = Some(snapshot);
                value.synthetic_transfers = self.synthetic_transfers.clone();
            });
        }
    }

    fn register_synthetic_transfer(
        &mut self,
        control: SyntheticTransferControl,
        snapshots: &watch::Sender<ResumeOwnerSnapshot>,
    ) -> Result<(), SyntheticTransferError> {
        let peer = control.peer_id();
        let generation = control.generation();
        let device = self
            .devices
            .as_ref()
            .and_then(|devices| devices.snapshot().get(&peer).copied())
            .ok_or(SyntheticTransferError::WrongSession)?;
        if device.connection_generation != generation
            || !matches!(
                device.connection,
                ConnectionState::Connecting | ConnectionState::Connected
            )
        {
            return Err(SyntheticTransferError::WrongSession);
        }

        let progress = control.synthetic_key_transfer_progress();
        self.synthetic_controls.insert(peer, control.clone());
        self.synthetic_transfers
            .insert(peer, (generation, progress));
        snapshots.send_modify(|snapshot| {
            snapshot.synthetic_transfers = self.synthetic_transfers.clone();
        });

        let mut updates = control.subscribe_synthetic_key_transfer();
        let commands = self.command_sender.clone();
        tokio::spawn(async move {
            loop {
                if updates.changed().await.is_err() {
                    return;
                }
                let progress = *updates.borrow_and_update();
                if commands
                    .send(OwnerCommand::SyntheticTransferProgressChanged(
                        peer, generation, progress,
                    ))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        });
        Ok(())
    }

    fn accept_future(
        &self,
        policy: Arc<Mutex<AcceptPolicy>>,
        last_selector: Arc<Mutex<Option<DeviceId>>>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<ResumeAttempt, kryptopass_lan::LanError>>
                + Send,
        >,
    > {
        let Some(listener) = self.listener.as_ref().cloned() else {
            return Box::pin(std::future::pending());
        };
        let Some(host) = self.host.as_ref().cloned() else {
            return Box::pin(std::future::pending());
        };
        let Some(devices) = self.devices.as_ref().cloned() else {
            return Box::pin(std::future::pending());
        };
        Box::pin(async move {
            listener
                .accept_authorized(move |device_bytes| {
                    let peer = DeviceId::from_bytes(device_bytes);
                    let permitted_by_policy = policy.lock().is_ok_and(|mut current| {
                        current.prune_failures(peer);
                        !current.suppressed.contains(&peer)
                            && !current.active.contains(&peer)
                            && !current.negotiating.contains(&peer)
                            && current.active.len() + current.negotiating.len() < MAX_OWNED_SESSIONS
                            && current
                                .cooling_until
                                .get(&peer)
                                .is_none_or(|until| *until <= StdInstant::now())
                    });
                    if !permitted_by_policy {
                        tracing::info!(
                            "LAN resume rejected: policy (suppressed, busy or cooling down)"
                        );
                        return false;
                    }
                    let snapshot = devices.snapshot();
                    let connection = snapshot.get(&peer).map(|device| device.connection);
                    let trusted_in_directory = connection.as_ref().is_some_and(|connection| {
                        matches!(
                            connection,
                            ConnectionState::Disconnected | ConnectionState::Failed
                        )
                    });
                    let known_to_host = host.lookup(peer).is_ok();
                    let authorized = trusted_in_directory && known_to_host;
                    if !authorized {
                        tracing::info!(
                            connection = ?connection,
                            known_to_host,
                            "LAN resume rejected: device not in a resumable state"
                        );
                    }
                    if authorized {
                        if let Ok(mut current) = policy.lock() {
                            if current.suppressed.contains(&peer)
                                || current.active.contains(&peer)
                                || current.negotiating.contains(&peer)
                                || current.active.len() + current.negotiating.len()
                                    >= MAX_OWNED_SESSIONS
                            {
                                return false;
                            }
                            current.negotiating.insert(peer);
                            if let Ok(mut selected) = last_selector.lock() {
                                *selected = Some(peer);
                            }
                        } else {
                            return false;
                        }
                    }
                    authorized
                })
                .await
        })
    }

    fn start_attempt(&mut self, attempt: ResumeAttempt, policy: &Arc<Mutex<AcceptPolicy>>) {
        let peer_id = DeviceId::from_bytes(attempt.initiator_device_id);
        if !attempt.transport.network_policy_current()
            || self
                .network_guard
                .as_ref()
                .is_none_or(|guard| guard.ensure_current().is_err())
        {
            clear_negotiating(policy, peer_id);
            return;
        }
        let Some(network_guard) = self.network_guard.as_ref().cloned() else {
            clear_negotiating(policy, peer_id);
            return;
        };
        let Some(host) = self.host.as_ref().cloned() else {
            clear_negotiating(policy, peer_id);
            return;
        };
        let Some(devices) = self.devices.as_ref().cloned() else {
            clear_negotiating(policy, peer_id);
            return;
        };
        let peer = match host.lookup(peer_id) {
            Ok(peer) if peer.device_id == peer_id => peer,
            _ => {
                clear_negotiating(policy, peer_id);
                record_failure(policy, peer_id);
                return;
            }
        };
        if !self.can_accept_peer(peer_id, peer, policy) {
            clear_negotiating(policy, peer_id);
            return;
        }
        let id = self.next_attempt_id;
        self.next_attempt_id = self.next_attempt_id.wrapping_add(1).max(1);
        let (cancel_sender, cancel_receiver) = watch::channel(false);
        let commands = self.command_sender.clone();
        let task_policy = Arc::clone(policy);
        let task_host = Arc::clone(&host);
        let task_devices = devices.clone();
        let task = tokio::spawn(async move {
            let result = open_incoming_attempt(
                task_host,
                task_devices,
                task_policy,
                network_guard,
                peer,
                attempt,
                cancel_receiver,
            )
            .await;
            if let Err(error) = commands
                .send(OwnerCommand::IncomingFinished(
                    peer_id,
                    id,
                    Box::new(result),
                ))
                .await
            {
                if let OwnerCommand::IncomingFinished(_, _, result) = error.0 {
                    if let Ok(mut session) = *result {
                        let _ = session.close().await;
                    }
                }
            }
        });
        self.pending_attempts.insert(
            peer_id,
            PendingAttempt {
                id,
                cancel: cancel_sender,
                task,
            },
        );
    }

    fn can_accept_peer(
        &self,
        peer_id: DeviceId,
        expected: PublicIdentity,
        policy: &Arc<Mutex<AcceptPolicy>>,
    ) -> bool {
        let Some(devices) = self.devices.as_ref() else {
            return false;
        };
        let policy_allows = policy.lock().is_ok_and(|current| {
            !current.suppressed.contains(&peer_id)
                && !current.active.contains(&peer_id)
                && current.active.len() < MAX_OWNED_SESSIONS
        });
        policy_allows
            && devices.snapshot().get(&peer_id).is_some_and(|device| {
                device.identity == expected
                    && matches!(
                        device.connection,
                        ConnectionState::Disconnected | ConnectionState::Failed
                    )
            })
    }

    fn watch_session(&self, peer_id: DeviceId, generation: u64, handle: &SessionHandle) {
        let mut events = handle.subscribe_events();
        let mut lifecycle = handle.subscribe_lifecycle();
        let commands = self.command_sender.clone();
        tokio::spawn(async move {
            let mut stable_timer_started = false;
            let current_lifecycle = *lifecycle.borrow_and_update();
            match current_lifecycle {
                Some(SessionEvent::Connected { .. }) => {
                    stable_timer_started = true;
                    schedule_stability_check(&commands, peer_id, generation);
                }
                Some(SessionEvent::Disconnected { .. } | SessionEvent::Failed { .. }) => {
                    let _ = commands
                        .send(OwnerCommand::SessionEnded(peer_id, generation))
                        .await;
                    return;
                }
                None => {}
            }

            loop {
                tokio::select! {
                    event = events.recv() => {
                        match event {
                            Ok(SessionEvent::Connected { .. }) if !stable_timer_started => {
                                stable_timer_started = true;
                                schedule_stability_check(&commands, peer_id, generation);
                            }
                            Ok(SessionEvent::Disconnected { .. } | SessionEvent::Failed { .. }) => {
                                let _ = commands.send(OwnerCommand::SessionEnded(peer_id, generation)).await;
                                return;
                            }
                            Ok(SessionEvent::Connected { .. }) => {}
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(broadcast::error::RecvError::Closed) => return,
                        }
                    }
                    changed = lifecycle.changed() => {
                        if changed.is_err() { return; }
                        let current_lifecycle = *lifecycle.borrow_and_update();
                        match current_lifecycle {
                            Some(SessionEvent::Connected { .. }) => {
                                if !stable_timer_started {
                                    stable_timer_started = true;
                                    schedule_stability_check(&commands, peer_id, generation);
                                }
                            }
                            Some(SessionEvent::Disconnected { .. } | SessionEvent::Failed { .. }) => {
                                let _ = commands.send(OwnerCommand::SessionEnded(peer_id, generation)).await;
                                return;
                            }
                            None => {}
                        }
                    }
                }
            }
        });
    }

    async fn handle_command(
        &mut self,
        command: OwnerCommand,
        snapshots: &watch::Sender<ResumeOwnerSnapshot>,
        policy: &Arc<Mutex<AcceptPolicy>>,
    ) -> bool {
        match command {
            OwnerCommand::EnsureReady(reply) => {
                let _ = reply.send(self.ensure_ready(snapshots).await);
            }
            OwnerCommand::RefreshTrust(reply) => {
                let result = if self.ready.is_none() {
                    self.ensure_ready(snapshots).await
                } else {
                    self.refresh_trust(snapshots).await
                };
                let _ = reply.send(result);
            }
            OwnerCommand::RefreshNetworkPolicy => {}
            OwnerCommand::Suppress(peer) => {
                if let Some(pending) = self.pending_attempts.remove(&peer) {
                    let _ = pending.cancel.send(true);
                    clear_negotiating(policy, peer);
                    let mut task = pending.task;
                    if timeout(PENDING_ATTEMPT_DRAIN_TIMEOUT, &mut task)
                        .await
                        .is_err()
                    {
                        task.abort();
                        let _ = task.await;
                    }
                }
                if let Some(mut session) = self.sessions.remove(&peer) {
                    let _ = session.handle.disconnect(SessionCloseReason::UserRequested);
                    let _ = timeout(SESSION_DRAIN_TIMEOUT, session.handle.wait_closed()).await;
                    if let Ok(mut current) = policy.lock() {
                        current.active.remove(&peer);
                    }
                }
            }
            OwnerCommand::Renegotiate(peer) => {
                if let Some(session) = self.sessions.get(&peer) {
                    tracing::info!("Sync binding changed: renegotiating the live session");
                    let _ = session.handle.disconnect(SessionCloseReason::Superseded);
                }
            }
            OwnerCommand::Allow(peer) => {
                if let Ok(mut current) = policy.lock() {
                    current.suppressed.remove(&peer);
                }
            }
            OwnerCommand::SessionStable(peer, generation) => {
                if self
                    .sessions
                    .get(&peer)
                    .is_some_and(|session| session.handle.generation() == generation)
                {
                    if let Ok(mut current) = policy.lock() {
                        current.failures.remove(&peer);
                        current.cooling_until.remove(&peer);
                    }
                }
            }
            OwnerCommand::SessionEnded(peer, generation) => {
                if self
                    .sessions
                    .get(&peer)
                    .is_some_and(|session| session.handle.generation() == generation)
                {
                    if let Some(mut session) = self.sessions.remove(&peer) {
                        let _ = timeout(SESSION_DRAIN_TIMEOUT, session.handle.wait_closed()).await;
                    }
                    if let Ok(mut current) = policy.lock() {
                        current.active.remove(&peer);
                    }
                    self.publish_ready(snapshots);
                }
            }
            OwnerCommand::RegisterSyntheticTransfer(control, reply) => {
                let result = self.register_synthetic_transfer(control, snapshots);
                let _ = reply.send(result);
            }
            OwnerCommand::StartSyntheticTransfer(peer, generation, prepared, reply) => {
                let result = self
                    .synthetic_controls
                    .get(&peer)
                    .filter(|control| control.generation() == generation)
                    .ok_or(SyntheticTransferError::WrongSession)
                    .and_then(|control| {
                        control.start_synthetic_key_transfer(generation, *prepared)
                    });
                let _ = reply.send(result);
            }
            OwnerCommand::CancelSyntheticTransfer(peer, generation, transfer_id, reply) => {
                let result = self
                    .synthetic_controls
                    .get(&peer)
                    .filter(|control| control.generation() == generation)
                    .ok_or(SyntheticTransferError::WrongSession)
                    .and_then(|control| {
                        control.cancel_synthetic_key_transfer(generation, transfer_id)
                    });
                let _ = reply.send(result);
            }
            OwnerCommand::SyntheticTransferProgressChanged(peer, generation, progress) => {
                if self
                    .synthetic_controls
                    .get(&peer)
                    .is_some_and(|control| control.generation() == generation)
                {
                    self.synthetic_transfers
                        .insert(peer, (generation, progress));
                    snapshots.send_modify(|snapshot| {
                        snapshot.synthetic_transfers = self.synthetic_transfers.clone();
                    });
                }
            }
            OwnerCommand::SyntheticTransferControlFor(peer, generation, reply) => {
                let control = self
                    .synthetic_controls
                    .get(&peer)
                    .filter(|control| control.generation() == generation)
                    .cloned();
                let _ = reply.send(control);
            }
            OwnerCommand::UnregisterSyntheticTransfer(peer, generation) => {
                if self
                    .synthetic_controls
                    .get(&peer)
                    .is_some_and(|control| control.generation() == generation)
                {
                    self.synthetic_controls.remove(&peer);
                }
            }
            OwnerCommand::IncomingFinished(peer, attempt_id, result) => {
                let matches_pending = self
                    .pending_attempts
                    .get(&peer)
                    .is_some_and(|pending| pending.id == attempt_id);
                if !matches_pending {
                    if let Ok(mut session) = *result {
                        let _ = session.close().await;
                    }
                    return false;
                }
                if let Some(pending) = self.pending_attempts.remove(&peer) {
                    let mut task = pending.task;
                    if timeout(PENDING_ATTEMPT_DRAIN_TIMEOUT, &mut task)
                        .await
                        .is_err()
                    {
                        task.abort();
                        let _ = task.await;
                    }
                }
                clear_negotiating(policy, peer);
                match *result {
                    Err(AttemptFailure::Cancelled) => {}
                    Err(AttemptFailure::Failed) => record_failure(policy, peer),
                    Ok(mut session) => {
                        let Some(host) = self.host.as_ref().cloned() else {
                            let _ = session.close().await;
                            return false;
                        };
                        let Some(devices) = self.devices.as_ref().cloned() else {
                            let _ = session.close().await;
                            return false;
                        };
                        let expected = session.peer_identity();
                        if host.lookup(peer).ok() != Some(expected)
                            || !self.can_accept_peer(peer, expected, policy)
                        {
                            let _ = session.close().await;
                            return false;
                        }
                        let options = SessionOptions {
                            reconnect_port: self
                                .listener
                                .as_ref()
                                .map_or(0, |listener| listener.port()),
                        };
                        let sync = (self.sync)(peer);
                        match host.start_device_session(devices, session, expected, options, sync) {
                            Ok(handle) => {
                                let generation = handle.generation();
                                let control = handle.synthetic_key_transfer_control();
                                if let Ok(mut current) = policy.lock() {
                                    current.active.insert(peer);
                                    current.failures.remove(&peer);
                                    current.cooling_until.remove(&peer);
                                }
                                self.watch_session(peer, generation, &handle);
                                let _ = self.register_synthetic_transfer(control, snapshots);
                                self.sessions.insert(peer, OwnedSession { handle });
                                self.publish_ready(snapshots);
                            }
                            Err(_) => record_failure(policy, peer),
                        }
                    }
                }
            }
            OwnerCommand::RelayIncoming(attempt) => {
                self.start_attempt(*attempt, policy);
            }
            OwnerCommand::Shutdown => return true,
        }
        false
    }

    fn record_failed_selector(
        &self,
        error: kryptopass_lan::LanError,
        policy: &Arc<Mutex<AcceptPolicy>>,
        last_selector: &Arc<Mutex<Option<DeviceId>>>,
    ) {
        let peer = last_selector
            .lock()
            .ok()
            .and_then(|mut selector| selector.take());
        if let Some(peer) = peer {
            clear_negotiating(policy, peer);
            if !matches!(error, kryptopass_lan::LanError::NetworkPolicy) {
                record_failure(policy, peer);
            }
        }
    }
}

fn schedule_stability_check(
    commands: &mpsc::Sender<OwnerCommand>,
    peer_id: DeviceId,
    generation: u64,
) {
    let stable_commands = commands.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let _ = stable_commands
            .send(OwnerCommand::SessionStable(peer_id, generation))
            .await;
    });
}

fn clear_negotiating(policy: &Arc<Mutex<AcceptPolicy>>, peer: DeviceId) {
    if let Ok(mut current) = policy.lock() {
        current.negotiating.remove(&peer);
    }
}

fn record_failure(policy: &Arc<Mutex<AcceptPolicy>>, peer: DeviceId) {
    if let Ok(mut current) = policy.lock() {
        current.prune_failures(peer);
        let failures = current.failures.entry(peer).or_default();
        failures.push_back(StdInstant::now());
        if failures.len() >= NEGOTIATION_FAILURE_LIMIT {
            current
                .cooling_until
                .insert(peer, StdInstant::now() + FAILURE_COOLDOWN);
            current.failures.remove(&peer);
        }
    }
}

async fn open_incoming_attempt(
    host: Arc<super::PeerHost>,
    devices: DeviceDirectory,
    policy: Arc<Mutex<AcceptPolicy>>,
    network_guard: NetworkPolicyGuard,
    expected: PublicIdentity,
    attempt: ResumeAttempt,
    mut cancelled: watch::Receiver<bool>,
) -> Result<kryptopass_pairing::EstablishedSession<super::HostTransport>, AttemptFailure> {
    let peer = expected.device_id;
    if *cancelled.borrow()
        || network_guard.ensure_current().is_err()
        || !attempt.transport.network_policy_current()
        || host.lookup(peer).ok() != Some(expected)
    {
        return Err(AttemptFailure::Cancelled);
    }
    let reconnect = host.open_reconnect_session(
        attempt.session_id,
        attempt.transport,
        expected,
        cancelled.clone(),
    );
    let outcome = tokio::select! {
        _ = cancelled.changed() => return Err(AttemptFailure::Cancelled),
        _ = network_guard.wait_for_change(Duration::from_millis(250)) => return Err(AttemptFailure::Cancelled),
        result = timeout(Duration::from_secs(30), reconnect) => match result {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(super::PeerError::Cancelled)) => return Err(AttemptFailure::Cancelled),
            Ok(Err(_)) | Err(_) => return Err(AttemptFailure::Failed),
        },
    };
    let mut session = outcome.session;
    let policy_allows = policy.lock().is_ok_and(|current| {
        current.negotiating.contains(&peer)
            && !current.suppressed.contains(&peer)
            && !current.active.contains(&peer)
    });
    let directory_allows = devices.snapshot().get(&peer).is_some_and(|device| {
        device.identity == expected
            && matches!(
                device.connection,
                ConnectionState::Disconnected | ConnectionState::Failed
            )
    });
    // Recheck SQLite after Noise: the same complete identity must still be
    // authorized before its new carrier can enter the DeviceDirectory.
    if *cancelled.borrow()
        || network_guard.ensure_current().is_err()
        || !policy_allows
        || !directory_allows
        || host.lookup(peer).ok() != Some(expected)
    {
        let _ = session.close().await;
        return Err(AttemptFailure::Cancelled);
    }
    Ok(session)
}

impl AcceptPolicy {
    fn prune_failures(&mut self, peer: DeviceId) {
        if self
            .cooling_until
            .get(&peer)
            .is_some_and(|until| *until <= StdInstant::now())
        {
            self.cooling_until.remove(&peer);
        }
        if let Some(failures) = self.failures.get_mut(&peer) {
            while failures
                .front()
                .is_some_and(|at| at.elapsed() >= NEGOTIATION_FAILURE_WINDOW)
            {
                let _ = failures.pop_front();
            }
            if failures.is_empty() {
                self.failures.remove(&peer);
            }
        }
    }
}

#[cfg(test)]
#[path = "resume_owner_lifecycle_tests.rs"]
mod lifecycle_tests;
