//! Trusted-device directory and active authenticated-session ownership.
//!
//! This crate contains no UI, persistence, GnuPG, or carrier implementation.
//! Applications feed it validated trusted identities and retain the returned
//! session handle for the lifetime of a connection.

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_pairing::{EstablishedSession, PairingError};
use kryptopass_protocol::{MessagePayload, proto};
use kryptopass_transport::PeerTransport;
pub use kryptopass_transport::{ConnectionFamily, ConnectionInfo, ConnectionRoute};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Instant, sleep_until};

/// Maximum number of trusted peers accepted by the public read model.
pub const MAX_TRUSTED_DEVICES: usize = 1024;
/// Active-session extension version understood by this implementation.
pub const ACTIVE_SESSION_VERSION: u32 = 1;
/// Interval between keepalive requests when the previous request was answered.
pub const SESSION_PING_INTERVAL: Duration = Duration::from_secs(3);
/// Maximum time allowed for the outstanding keepalive request.
pub const SESSION_PING_TIMEOUT: Duration = Duration::from_secs(6);
/// Maximum time allowed for the initial authenticated session hello.
pub const SESSION_HELLO_TIMEOUT: Duration = Duration::from_secs(30);

const SESSION_COMMAND_CAPACITY: usize = 1;
const SESSION_EVENT_CAPACITY: usize = 16;
const MAX_CONTROL_MESSAGES_PER_SECOND: usize = 32;
const SESSION_CONTROL_SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Trust state exposed by the directory. Only persisted trusted peers enter it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustState {
    /// The peer was supplied by the application's persisted trust source.
    Trusted,
}

/// Current connection state, kept independent from trust state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    /// No authenticated session is active.
    Disconnected,
    /// An authenticated session is exchanging its session hello.
    Connecting,
    /// Both endpoints authenticated the session hello.
    Connected,
    /// The authenticated peer has requested a new LAN carrier.
    Reconnecting,
    /// The last connection attempt failed.
    Failed,
}

/// Options advertised when a pairing outcome becomes an active session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionOptions {
    /// Local LAN resume port carried inside the authenticated session hello;
    /// zero means that this endpoint does not accept LAN resume connections.
    pub reconnect_port: u16,
}

/// One public device row in the read model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceSnapshot {
    /// Observed path of this connection generation; cleared outside Connected.
    pub connection_info: ConnectionInfo,
    /// Validated public identity of the trusted device.
    pub identity: PublicIdentity,
    /// Persisted authorization state.
    pub trust: TrustState,
    /// Volatile active-session state.
    pub connection: ConnectionState,
    /// Token that changes whenever a new owner supersedes the prior session.
    pub connection_generation: u64,
}

/// Immutable, bounded snapshot suitable for observers and UI projections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceDirectorySnapshot {
    /// Increments whenever a snapshot changes.
    pub revision: u64,
    /// Trusted peers in stable device-ID order.
    pub devices: Vec<DeviceSnapshot>,
}

impl DeviceDirectorySnapshot {
    /// Finds a trusted device by its stable identifier.
    #[must_use]
    pub fn get(&self, device_id: &DeviceId) -> Option<&DeviceSnapshot> {
        self.devices
            .binary_search_by_key(device_id, |device| device.identity.device_id)
            .ok()
            .and_then(|index| self.devices.get(index))
    }
}

/// Failures updating the trusted-device projection.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum DeviceDirectoryError {
    /// The trusted list exceeds its hard limit.
    #[error("trusted device list exceeds the maximum")]
    TooManyDevices,
    /// A public identity fails its key or derived-ID checks.
    #[error("trusted device identity is invalid")]
    InvalidIdentity,
    /// The local device cannot be trusted as its own peer.
    #[error("local device cannot appear in the trusted peer list")]
    SelfTrust,
    /// The input list contains the same device more than once.
    #[error("trusted device list contains a duplicate")]
    DuplicateDevice,
    /// The requested connection-state update targets an untrusted device.
    #[error("device is not in the trusted directory")]
    PeerNotTrusted,
    /// Snapshot revision reached its numeric limit.
    #[error("device directory revision is exhausted")]
    RevisionExhausted,
    /// The supplied local identity differs from the directory's local ID.
    #[error("local device ID does not match the directory")]
    LocalDeviceMismatch,
    /// Only a session owner that completed both hellos may publish Connected.
    #[error("connected state is managed by the active-session owner")]
    ConnectedStateManagedBySession,
    /// A Tokio runtime is required to own an active session.
    #[error("active session requires a Tokio runtime")]
    RuntimeUnavailable,
    /// A prior directory mutation panicked while holding its short lock.
    #[error("device directory mutation lock is poisoned")]
    MutationLockPoisoned,
}

/// Thread-safe trusted-device read model with watch-based snapshot observation.
#[derive(Clone)]
pub struct DeviceDirectory {
    local_device_id: DeviceId,
    snapshots: watch::Sender<Arc<DeviceDirectorySnapshot>>,
    mutations: Arc<Mutex<DirectoryMutationState>>,
}

#[derive(Default)]
struct DirectoryMutationState {
    next_generation: u64,
}

impl fmt::Debug for DeviceDirectory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceDirectory")
            .field("local_device_id", &"[redacted]")
            .field("device_count", &self.snapshot().devices.len())
            .finish()
    }
}

impl DeviceDirectory {
    /// Creates an empty directory for one local device.
    #[must_use]
    pub fn new(local_device_id: DeviceId) -> Self {
        let initial = Arc::new(DeviceDirectorySnapshot {
            revision: 0,
            devices: Vec::new(),
        });
        let (snapshots, _receiver) = watch::channel(initial);
        Self {
            local_device_id,
            snapshots,
            mutations: Arc::new(Mutex::new(DirectoryMutationState::default())),
        }
    }

    /// Atomically replaces the read model with a fully validated trusted list.
    /// Connection state is retained only for the exact same public identity.
    pub fn replace_trusted(&self, peers: Vec<PublicIdentity>) -> Result<(), DeviceDirectoryError> {
        let _mutation = self.lock_mutations()?;
        if peers.len() > MAX_TRUSTED_DEVICES {
            return Err(DeviceDirectoryError::TooManyDevices);
        }
        let current = self.snapshot();
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or(DeviceDirectoryError::RevisionExhausted)?;
        let mut seen = HashSet::with_capacity(peers.len());
        let mut devices = Vec::with_capacity(peers.len());
        for identity in peers {
            identity
                .validate()
                .map_err(|_| DeviceDirectoryError::InvalidIdentity)?;
            if identity.device_id.as_bytes().iter().all(|byte| *byte == 0) {
                return Err(DeviceDirectoryError::InvalidIdentity);
            }
            if identity.device_id == self.local_device_id {
                return Err(DeviceDirectoryError::SelfTrust);
            }
            if !seen.insert(identity.device_id) {
                return Err(DeviceDirectoryError::DuplicateDevice);
            }
            let connection = current
                .get(&identity.device_id)
                .filter(|previous| previous.identity == identity)
                .map_or(ConnectionState::Disconnected, |previous| {
                    previous.connection
                });
            let connection_generation = current
                .get(&identity.device_id)
                .filter(|previous| previous.identity == identity)
                .map_or(0, |previous| previous.connection_generation);
            let connection_info = current
                .get(&identity.device_id)
                .filter(|previous| {
                    previous.identity == identity && connection == ConnectionState::Connected
                })
                .map_or_else(ConnectionInfo::default, |previous| previous.connection_info);
            devices.push(DeviceSnapshot {
                connection_info,
                identity,
                trust: TrustState::Trusted,
                connection,
                connection_generation,
            });
        }
        devices.sort_by_key(|device| device.identity.device_id);
        self.snapshots
            .send_replace(Arc::new(DeviceDirectorySnapshot {
                revision: next_revision,
                devices,
            }));
        Ok(())
    }

    /// Updates volatile connection state without changing trust.
    pub fn update_connection_state(
        &self,
        device_id: DeviceId,
        connection: ConnectionState,
    ) -> Result<(), DeviceDirectoryError> {
        if connection == ConnectionState::Connected {
            return Err(DeviceDirectoryError::ConnectedStateManagedBySession);
        }
        let mut mutation = self.lock_mutations()?;
        let current = self.snapshot();
        let mut next = (*current).clone();
        let device = next
            .devices
            .binary_search_by_key(&device_id, |device| device.identity.device_id)
            .ok()
            .and_then(|index| next.devices.get_mut(index))
            .ok_or(DeviceDirectoryError::PeerNotTrusted)?;
        if device.connection == connection {
            return Ok(());
        }
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or(DeviceDirectoryError::RevisionExhausted)?;
        device.connection_generation = Self::allocate_generation(&mut mutation)?;
        device.connection = connection;
        if connection != ConnectionState::Connected {
            device.connection_info = ConnectionInfo::default();
        }
        next.revision = next_revision;
        self.snapshots.send_replace(Arc::new(next));
        Ok(())
    }

    /// Returns the latest immutable directory snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Arc<DeviceDirectorySnapshot> {
        Arc::clone(&self.snapshots.borrow())
    }

    /// Subscribes to changes; each notification contains the latest snapshot.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<Arc<DeviceDirectorySnapshot>> {
        self.snapshots.subscribe()
    }

    /// Returns this directory's local device identifier.
    #[must_use]
    pub const fn local_device_id(&self) -> DeviceId {
        self.local_device_id
    }

    fn contains_exact_peer(&self, peer: &PublicIdentity) -> bool {
        self.snapshot()
            .get(&peer.device_id)
            .is_some_and(|device| device.trust == TrustState::Trusted && device.identity == *peer)
    }

    fn begin_session(&self, peer: &PublicIdentity) -> Result<u64, DeviceDirectoryError> {
        let mut mutation = self.lock_mutations()?;
        let current = self.snapshot();
        let mut next = (*current).clone();
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or(DeviceDirectoryError::RevisionExhausted)?;
        let device = next
            .devices
            .binary_search_by_key(&peer.device_id, |device| device.identity.device_id)
            .ok()
            .and_then(|index| next.devices.get_mut(index))
            .filter(|device| device.trust == TrustState::Trusted && device.identity == *peer)
            .ok_or(DeviceDirectoryError::PeerNotTrusted)?;
        let generation = Self::allocate_generation(&mut mutation)?;
        device.connection_generation = generation;
        device.connection = ConnectionState::Connecting;
        device.connection_info = ConnectionInfo::default();
        next.revision = next_revision;
        self.snapshots.send_replace(Arc::new(next));
        Ok(generation)
    }

    fn update_connection_state_for_generation(
        &self,
        peer: &PublicIdentity,
        generation: u64,
        connection: ConnectionState,
    ) -> Result<(), DeviceDirectoryError> {
        let _mutation = self.lock_mutations()?;
        let current = self.snapshot();
        let mut next = (*current).clone();
        let device = next
            .devices
            .binary_search_by_key(&peer.device_id, |device| device.identity.device_id)
            .ok()
            .and_then(|index| next.devices.get_mut(index))
            .filter(|device| {
                device.trust == TrustState::Trusted
                    && device.identity == *peer
                    && device.connection_generation == generation
            })
            .ok_or(DeviceDirectoryError::PeerNotTrusted)?;
        if device.connection == connection {
            return Ok(());
        }
        device.connection = connection;
        if connection != ConnectionState::Connected {
            device.connection_info = ConnectionInfo::default();
        }
        next.revision = current
            .revision
            .checked_add(1)
            .ok_or(DeviceDirectoryError::RevisionExhausted)?;
        self.snapshots.send_replace(Arc::new(next));
        Ok(())
    }

    fn update_connection_info_for_generation(
        &self,
        peer: &PublicIdentity,
        generation: u64,
        info: ConnectionInfo,
    ) -> Result<(), DeviceDirectoryError> {
        let _mutation = self.lock_mutations()?;
        let current = self.snapshot();
        let mut next = (*current).clone();
        let device = next
            .devices
            .iter_mut()
            .find(|device| {
                device.identity == *peer
                    && device.connection_generation == generation
                    && device.connection == ConnectionState::Connected
            })
            .ok_or(DeviceDirectoryError::PeerNotTrusted)?;
        if device.connection_info == info {
            return Ok(());
        }
        device.connection_info = info;
        next.revision = current
            .revision
            .checked_add(1)
            .ok_or(DeviceDirectoryError::RevisionExhausted)?;
        self.snapshots.send_replace(Arc::new(next));
        Ok(())
    }

    fn lock_mutations(
        &self,
    ) -> Result<MutexGuard<'_, DirectoryMutationState>, DeviceDirectoryError> {
        self.mutations
            .lock()
            .map_err(|_| DeviceDirectoryError::MutationLockPoisoned)
    }

    fn allocate_generation(
        mutation: &mut DirectoryMutationState,
    ) -> Result<u64, DeviceDirectoryError> {
        mutation.next_generation = mutation
            .next_generation
            .checked_add(1)
            .ok_or(DeviceDirectoryError::RevisionExhausted)?;
        Ok(mutation.next_generation)
    }

    fn session_is_current(&self, peer: &PublicIdentity, generation: u64) -> bool {
        self.snapshot().get(&peer.device_id).is_some_and(|device| {
            device.trust == TrustState::Trusted
                && device.identity == *peer
                && device.connection_generation == generation
        })
    }

    fn session_termination_reason(
        &self,
        peer: &PublicIdentity,
        generation: u64,
    ) -> Option<SessionCloseReason> {
        match self.snapshot().get(&peer.device_id) {
            Some(device) if device.identity != *peer || device.trust != TrustState::Trusted => {
                Some(SessionCloseReason::TrustRevoked)
            }
            Some(device) if device.connection_generation != generation => {
                Some(SessionCloseReason::Superseded)
            }
            None => Some(SessionCloseReason::TrustRevoked),
            _ => None,
        }
    }
}

/// User or application reason for closing an authenticated session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SessionCloseReason {
    /// The user explicitly disconnected this device.
    UserRequested = 1,
    /// The application is shutting down or the owning handle was dropped.
    AppClosing = 2,
    /// A newer authenticated connection replaced this session.
    Superseded = 3,
    /// The peer was removed from the trusted-device directory.
    TrustRevoked = 4,
    /// The peer violated the active-session protocol.
    ProtocolError = 5,
}

impl SessionCloseReason {
    fn from_wire(reason: u32) -> Option<Self> {
        match reason {
            1 => Some(Self::UserRequested),
            2 => Some(Self::AppClosing),
            3 => Some(Self::Superseded),
            4 => Some(Self::TrustRevoked),
            5 => Some(Self::ProtocolError),
            _ => None,
        }
    }
}

/// Bounded, redacted event stream emitted by a session owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionEvent {
    /// The bilateral authenticated hello completed.
    Connected {
        /// Authenticated peer ID.
        peer_id: DeviceId,
        /// Session owner generation.
        generation: u64,
        /// Remote LAN resume port authenticated by its session hello; zero is absent.
        reconnect_port: u16,
    },
    /// The session ended normally or by an authenticated close message.
    Disconnected {
        /// Authenticated peer ID.
        peer_id: DeviceId,
        /// Session owner generation.
        generation: u64,
        /// Close reason reported by the local or remote endpoint.
        reason: SessionCloseReason,
    },
    /// The session ended because an authenticated protocol or transport check failed.
    Failed {
        /// Authenticated peer ID.
        peer_id: DeviceId,
        /// Session owner generation.
        generation: u64,
        /// Coarse failure category with no frame or payload data.
        failure: SessionFailure,
    },
}

/// Redacted failure categories exposed to observers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionFailure {
    /// Session hello did not match the expected identities or version.
    HelloRejected,
    /// The bilateral hello exceeded its monotonic setup deadline.
    HelloTimeout,
    /// Noise authentication or frame authentication failed.
    Authentication,
    /// A message violated the active-session protocol.
    Protocol,
    /// A transport send or receive failed.
    Transport,
    /// A keepalive did not receive its matching response in time.
    PingTimeout,
    /// The peer sent control traffic above the local bound.
    ControlRate,
    /// A ping or pong nonce was unexpected or exhausted.
    NonceSequence,
}

/// Errors returned when submitting a bounded disconnect request.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SessionCommandError {
    /// The one-slot command queue already contains a disconnect request.
    #[error("disconnect command queue is full")]
    QueueFull,
    /// The session owner has already exited.
    #[error("session owner has exited")]
    Closed,
}

enum SessionCommand {
    Disconnect(SessionCloseReason),
}

/// Handle retaining an active session task and its bounded observability APIs.
pub struct SessionHandle {
    peer_id: DeviceId,
    directory: DeviceDirectory,
    commands: mpsc::Sender<SessionCommand>,
    events: broadcast::Sender<SessionEvent>,
    lifecycle: watch::Receiver<Option<SessionEvent>>,
    closed: watch::Receiver<bool>,
    generation: u64,
}

impl fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionHandle")
            .field("peer_id", &"[redacted]")
            .field(
                "connection",
                &self.snapshot().get(&self.peer_id).map(|d| d.connection),
            )
            .finish()
    }
}

impl SessionHandle {
    /// Returns the authenticated peer's identifier.
    #[must_use]
    pub const fn peer_id(&self) -> DeviceId {
        self.peer_id
    }

    /// Returns this owner's connection generation, used to correlate snapshots.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Queues a disconnect command without allowing an unbounded backlog.
    pub fn disconnect(&self, reason: SessionCloseReason) -> Result<(), SessionCommandError> {
        match self.commands.try_send(SessionCommand::Disconnect(reason)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(SessionCommandError::QueueFull),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(SessionCommandError::Closed),
        }
    }

    /// Subscribes to bounded session events. Slow observers may receive Lagged.
    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<SessionEvent> {
        self.events.subscribe()
    }

    /// Subscribes to the latest lifecycle event, including the final event if
    /// the session already ended before this receiver was created.
    #[must_use]
    pub fn subscribe_lifecycle(&self) -> watch::Receiver<Option<SessionEvent>> {
        self.lifecycle.clone()
    }

    /// Returns the latest shared directory snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Arc<DeviceDirectorySnapshot> {
        self.directory.snapshot()
    }

    /// Waits until session cleanup, transport close, and final event publishing complete.
    pub async fn wait_closed(&mut self) {
        loop {
            if *self.closed.borrow() {
                break;
            }
            if self.closed.changed().await.is_err() {
                break;
            }
        }
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        let _ = self
            .commands
            .try_send(SessionCommand::Disconnect(SessionCloseReason::AppClosing));
    }
}

/// Starts one owner task for a pairing outcome's authenticated Noise session.
///
/// The `EstablishedSession` can only be obtained after `PairingEngine::run`
/// successfully persists trust. The task confirms that the exact peer remains
/// trusted, exchanges bilateral session hellos, and only then publishes
/// `Connected`. The session handle is the sole mutable owner of its crypto
/// state; no cipher state or sequence counter is recreated here.
pub fn start_session<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_with_options(
        directory,
        session,
        local_device_id,
        expected_peer,
        SessionOptions::default(),
    )
}

/// Starts a session while advertising the selected local LAN reconnect port.
///
/// Port zero keeps the endpoint unavailable for LAN resume. The remote port is
/// surfaced only after the authenticated bilateral session hello succeeds.
pub fn start_session_with_options<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    mut session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    options: SessionOptions,
) -> Result<SessionHandle, DeviceDirectoryError> {
    if directory.local_device_id() != local_device_id
        || session.local_device_id() != local_device_id
    {
        return Err(DeviceDirectoryError::LocalDeviceMismatch);
    }
    expected_peer
        .validate()
        .map_err(|_| DeviceDirectoryError::InvalidIdentity)?;
    if session.peer_identity() != expected_peer
        || expected_peer.device_id == local_device_id
        || !directory.contains_exact_peer(&expected_peer)
    {
        return Err(DeviceDirectoryError::PeerNotTrusted);
    }
    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| DeviceDirectoryError::RuntimeUnavailable)?;
    let generation = directory.begin_session(&expected_peer)?;

    let peer_id = expected_peer.device_id;
    let (command_tx, mut command_rx) = mpsc::channel(SESSION_COMMAND_CAPACITY);
    let (event_tx, _event_rx) = broadcast::channel(SESSION_EVENT_CAPACITY);
    let (lifecycle_tx, lifecycle_rx) = watch::channel(None);
    let (closed_tx, closed_rx) = watch::channel(false);
    let owner_directory = directory.clone();
    let owner_events = event_tx.clone();
    let mut directory_changes = directory.subscribe();
    runtime.spawn(async move {
        let session_started = Instant::now();
        let hello_context = SessionHelloContext {
            local_device_id,
            options,
            expected_peer,
            generation,
            directory: &owner_directory,
        };
        let initial = exchange_session_hello(
            &mut session,
            &hello_context,
            &mut directory_changes,
            &mut command_rx,
        )
        .await;
        let exit = match initial {
            Ok(remote_reconnect_port)
                if owner_directory.session_is_current(&expected_peer, generation) =>
            {
                if owner_directory
                    .update_connection_state_for_generation(
                        &expected_peer,
                        generation,
                        ConnectionState::Connected,
                    )
                    .is_err()
                {
                    SessionExit::Disconnected(
                        owner_directory
                            .session_termination_reason(&expected_peer, generation)
                            .unwrap_or(SessionCloseReason::TrustRevoked),
                        true,
                    )
                } else {
                    let event = SessionEvent::Connected {
                        peer_id,
                        generation,
                        reconnect_port: remote_reconnect_port,
                    };
                    let _ = owner_events.send(event);
                    lifecycle_tx.send_replace(Some(event));
                    tracing::info!(
                        elapsed_ms = session_started.elapsed().as_secs_f64() * 1000.0,
                        "authenticated session hello complete"
                    );
                    run_connected_session(
                        &mut session,
                        &owner_directory,
                        &mut directory_changes,
                        &mut command_rx,
                        expected_peer,
                        generation,
                    )
                    .await
                }
            }
            Ok(_) => SessionExit::Disconnected(
                owner_directory
                    .session_termination_reason(&expected_peer, generation)
                    .unwrap_or(SessionCloseReason::TrustRevoked),
                true,
            ),
            Err(exit) => exit,
        };
        tracing::info!(
            reason = ?exit,
            elapsed_ms = session_started.elapsed().as_secs_f64() * 1000.0,
            "authenticated session ending"
        );
        finish_session(
            &mut session,
            SessionFinishContext {
                directory: &owner_directory,
                expected_peer,
                peer_id,
                generation,
                events: &owner_events,
                lifecycle: &lifecycle_tx,
                closed: &closed_tx,
            },
            exit,
        )
        .await;
    });

    Ok(SessionHandle {
        peer_id,
        directory,
        commands: command_tx,
        events: event_tx,
        lifecycle: lifecycle_rx,
        closed: closed_rx,
        generation,
    })
}

#[derive(Debug)]
enum SessionExit {
    Disconnected(SessionCloseReason, bool),
    Failed(SessionFailure),
}

struct SessionHelloContext<'a> {
    local_device_id: DeviceId,
    options: SessionOptions,
    expected_peer: PublicIdentity,
    generation: u64,
    directory: &'a DeviceDirectory,
}

async fn exchange_session_hello<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    context: &SessionHelloContext<'_>,
    directory_changes: &mut watch::Receiver<Arc<DeviceDirectorySnapshot>>,
    commands: &mut mpsc::Receiver<SessionCommand>,
) -> Result<u16, SessionExit> {
    let hello_deadline = Instant::now() + SESSION_HELLO_TIMEOUT;
    if !context
        .directory
        .session_is_current(&context.expected_peer, context.generation)
    {
        return Err(SessionExit::Disconnected(
            context
                .directory
                .session_termination_reason(&context.expected_peer, context.generation)
                .unwrap_or(SessionCloseReason::TrustRevoked),
            true,
        ));
    }
    let hello = proto::SessionHello {
        session_version: ACTIVE_SESSION_VERSION,
        device_id: context.local_device_id.as_bytes().to_vec(),
        reconnect_port: u32::from(context.options.reconnect_port),
    };
    let send = session.send_message(proto::MessageType::SessionHello, &hello);
    tokio::select! {
        result = send => result.map_err(|error| SessionExit::Failed(classify_pairing_error(error)))?,
        command = commands.recv() => return Err(SessionExit::Disconnected(
            command.map_or(SessionCloseReason::AppClosing, |SessionCommand::Disconnect(reason)| reason),
            true,
        )),
        termination = wait_for_session_termination(context.directory, directory_changes, &context.expected_peer, context.generation) => {
            return Err(termination.map_or_else(
                SessionExit::Failed,
                |reason| SessionExit::Disconnected(reason, true),
            ));
        }
        () = sleep_until(hello_deadline) => return Err(SessionExit::Failed(SessionFailure::HelloTimeout)),
    }

    let receive = session.receive_message();
    let message = tokio::select! {
        result = receive => result.map_err(|error| SessionExit::Failed(classify_pairing_error(error)))?,
        command = commands.recv() => return Err(SessionExit::Disconnected(
            command.map_or(SessionCloseReason::AppClosing, |SessionCommand::Disconnect(reason)| reason),
            true,
        )),
        termination = wait_for_session_termination(context.directory, directory_changes, &context.expected_peer, context.generation) => {
            return Err(termination.map_or_else(
                SessionExit::Failed,
                |reason| SessionExit::Disconnected(reason, true),
            ));
        }
        () = sleep_until(hello_deadline) => return Err(SessionExit::Failed(SessionFailure::HelloTimeout)),
    };
    match (message.message_type, message.payload) {
        (proto::MessageType::SessionHello, MessagePayload::SessionHello(hello)) => {
            if hello.session_version != ACTIVE_SESSION_VERSION
                || hello.reconnect_port > u32::from(u16::MAX)
                || hello.device_id.as_slice() != context.expected_peer.device_id.as_bytes()
            {
                return Err(SessionExit::Failed(SessionFailure::HelloRejected));
            }
            Ok(hello.reconnect_port as u16)
        }
        (proto::MessageType::SessionClose, MessagePayload::SessionClose(close)) => {
            let Some(reason) = SessionCloseReason::from_wire(close.reason) else {
                return Err(SessionExit::Failed(SessionFailure::Protocol));
            };
            Err(SessionExit::Disconnected(reason, false))
        }
        _ => Err(SessionExit::Failed(SessionFailure::HelloRejected)),
    }
}

async fn wait_for_session_termination(
    directory: &DeviceDirectory,
    directory_changes: &mut watch::Receiver<Arc<DeviceDirectorySnapshot>>,
    peer: &PublicIdentity,
    generation: u64,
) -> Result<SessionCloseReason, SessionFailure> {
    loop {
        directory_changes
            .changed()
            .await
            .map_err(|_| SessionFailure::Protocol)?;
        if let Some(reason) = directory.session_termination_reason(peer, generation) {
            return Ok(reason);
        }
    }
}

async fn run_connected_session<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    directory: &DeviceDirectory,
    directory_changes: &mut watch::Receiver<Arc<DeviceDirectorySnapshot>>,
    commands: &mut mpsc::Receiver<SessionCommand>,
    expected_peer: PublicIdentity,
    generation: u64,
) -> SessionExit {
    let mut next_observation = Instant::now();
    let mut next_local_nonce = 1_u64;
    let mut last_remote_nonce = 0_u64;
    let mut outstanding: Option<(u64, Instant)> = None;
    let mut next_ping_at = Instant::now() + SESSION_PING_INTERVAL;
    let mut rate_limiter = ControlRateLimiter::default();

    loop {
        let ping_deadline = outstanding
            .map(|(_, deadline)| deadline)
            .unwrap_or(next_ping_at);
        let selected = tokio::select! {
            () = sleep_until(next_observation) => SelectedSessionEvent::ObservePath,
            command = commands.recv() => SelectedSessionEvent::Command(command),
            changed = directory_changes.changed() => SelectedSessionEvent::DirectoryChanged(changed),
            message = session.receive_message() => SelectedSessionEvent::Message(message),
            () = sleep_until(next_ping_at), if outstanding.is_none() => SelectedSessionEvent::PingDue,
            () = sleep_until(ping_deadline), if outstanding.is_some() => SelectedSessionEvent::PingTimeout,
        };

        match selected {
            SelectedSessionEvent::ObservePath => {
                let info =
                    tokio::time::timeout(Duration::from_millis(500), session.connection_info())
                        .await
                        .unwrap_or_default();
                let _ = directory.update_connection_info_for_generation(
                    &expected_peer,
                    generation,
                    info,
                );
                next_observation = Instant::now() + Duration::from_secs(2);
            }
            SelectedSessionEvent::Command(Some(SessionCommand::Disconnect(reason))) => {
                return SessionExit::Disconnected(reason, true);
            }
            SelectedSessionEvent::Command(None) => {
                return SessionExit::Disconnected(SessionCloseReason::AppClosing, true);
            }
            SelectedSessionEvent::DirectoryChanged(Ok(())) => {
                if let Some(reason) =
                    directory.session_termination_reason(&expected_peer, generation)
                {
                    return SessionExit::Disconnected(reason, true);
                }
            }
            SelectedSessionEvent::DirectoryChanged(Err(_)) => {
                return SessionExit::Failed(SessionFailure::Protocol);
            }
            SelectedSessionEvent::PingDue => {
                let nonce = next_local_nonce;
                let Some(next_nonce) = nonce.checked_add(1) else {
                    return SessionExit::Failed(SessionFailure::NonceSequence);
                };
                if let Err(failure) = send_control_bounded(
                    session,
                    proto::MessageType::SessionPing,
                    &proto::SessionPing { nonce },
                )
                .await
                {
                    return SessionExit::Failed(failure);
                }
                next_local_nonce = next_nonce;
                outstanding = Some((nonce, Instant::now() + SESSION_PING_TIMEOUT));
            }
            SelectedSessionEvent::PingTimeout => {
                return SessionExit::Failed(SessionFailure::PingTimeout);
            }
            SelectedSessionEvent::Message(Err(error)) => {
                return SessionExit::Failed(classify_pairing_error(error));
            }
            SelectedSessionEvent::Message(Ok(message)) => {
                if !rate_limiter.accept(Instant::now()) {
                    return SessionExit::Failed(SessionFailure::ControlRate);
                }
                match (message.message_type, message.payload) {
                    (proto::MessageType::SessionPing, MessagePayload::SessionPing(ping)) => {
                        let Some(expected_nonce) = last_remote_nonce.checked_add(1) else {
                            return SessionExit::Failed(SessionFailure::NonceSequence);
                        };
                        if ping.nonce != expected_nonce {
                            return SessionExit::Failed(SessionFailure::NonceSequence);
                        }
                        if let Err(failure) = send_control_bounded(
                            session,
                            proto::MessageType::SessionPong,
                            &proto::SessionPong { nonce: ping.nonce },
                        )
                        .await
                        {
                            return SessionExit::Failed(failure);
                        }
                        last_remote_nonce = ping.nonce;
                    }
                    (proto::MessageType::SessionPong, MessagePayload::SessionPong(pong)) => {
                        match outstanding {
                            Some((_, deadline)) if Instant::now() >= deadline => {
                                return SessionExit::Failed(SessionFailure::PingTimeout);
                            }
                            Some((nonce, _)) if nonce == pong.nonce => {}
                            _ => return SessionExit::Failed(SessionFailure::NonceSequence),
                        }
                        outstanding = None;
                        next_ping_at = Instant::now() + SESSION_PING_INTERVAL;
                    }
                    (proto::MessageType::SessionClose, MessagePayload::SessionClose(close)) => {
                        let Some(reason) = SessionCloseReason::from_wire(close.reason) else {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        };
                        return SessionExit::Disconnected(reason, false);
                    }
                    _ => return SessionExit::Failed(SessionFailure::Protocol),
                }
            }
        }
    }
}

enum SelectedSessionEvent {
    ObservePath,
    Command(Option<SessionCommand>),
    DirectoryChanged(Result<(), watch::error::RecvError>),
    Message(Result<kryptopass_protocol::DecodedMessage, PairingError>),
    PingDue,
    PingTimeout,
}

async fn finish_session<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    context: SessionFinishContext<'_>,
    exit: SessionExit,
) {
    let (event, connection) = match exit {
        SessionExit::Disconnected(reason, send_close) => {
            if send_close {
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    session.send_message(
                        proto::MessageType::SessionClose,
                        &proto::SessionClose {
                            reason: reason as u32,
                        },
                    ),
                )
                .await;
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
            (
                SessionEvent::Disconnected {
                    peer_id: context.peer_id,
                    generation: context.generation,
                    reason,
                },
                ConnectionState::Disconnected,
            )
        }
        SessionExit::Failed(failure) => {
            // A transient carrier failure must not tell the peer to stop recovery.
            if !matches!(
                failure,
                SessionFailure::Transport | SessionFailure::PingTimeout
            ) {
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    session.send_message(
                        proto::MessageType::SessionClose,
                        &proto::SessionClose {
                            reason: SessionCloseReason::ProtocolError as u32,
                        },
                    ),
                )
                .await;
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
            (
                SessionEvent::Failed {
                    peer_id: context.peer_id,
                    generation: context.generation,
                    failure,
                },
                ConnectionState::Failed,
            )
        }
    };
    let _ = context.directory.update_connection_state_for_generation(
        &context.expected_peer,
        context.generation,
        connection,
    );
    let _ = context.events.send(event);
    context.lifecycle.send_replace(Some(event));
    context.closed.send_replace(true);
}

struct SessionFinishContext<'a> {
    directory: &'a DeviceDirectory,
    expected_peer: PublicIdentity,
    peer_id: DeviceId,
    generation: u64,
    events: &'a broadcast::Sender<SessionEvent>,
    lifecycle: &'a watch::Sender<Option<SessionEvent>>,
    closed: &'a watch::Sender<bool>,
}

fn classify_pairing_error(error: PairingError) -> SessionFailure {
    match error {
        PairingError::Transport(_) => SessionFailure::Transport,
        PairingError::CryptoFailure => SessionFailure::Authentication,
        PairingError::Protocol(_) | PairingError::UnexpectedMessage => SessionFailure::Protocol,
        _ => SessionFailure::Protocol,
    }
}

async fn send_control_bounded<T: PeerTransport, M: prost::Message>(
    session: &mut EstablishedSession<T>,
    message_type: proto::MessageType,
    payload: &M,
) -> Result<(), SessionFailure> {
    match tokio::time::timeout(
        SESSION_CONTROL_SEND_TIMEOUT,
        session.send_message(message_type, payload),
    )
    .await
    {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(classify_pairing_error(error)),
        Err(_) => Err(SessionFailure::Transport),
    }
}

#[derive(Default)]
struct ControlRateLimiter {
    recent: VecDeque<Instant>,
}

impl ControlRateLimiter {
    fn accept(&mut self, now: Instant) -> bool {
        while self
            .recent
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(1))
        {
            let _ = self.recent.pop_front();
        }
        if self.recent.len() >= MAX_CONTROL_MESSAGES_PER_SECOND {
            return false;
        }
        self.recent.push_back(now);
        true
    }
}
