//! Trusted-device directory and active authenticated-session ownership.
//!
//! This crate contains no UI, persistence, GnuPG, or carrier implementation.
//! Applications feed it validated trusted identities and retain the returned
//! session handle for the lifetime of a connection.

/// Phone → Desktop vault-identity receiver (extension version 3).
pub mod synthetic_receive;
/// Explicit synthetic provisioning sender, independent of UI and OpenPGP engines.
pub mod synthetic_transfer;
pub use synthetic_receive::{
    INBOUND_KEY_TIMEOUT, INBOUND_KEY_TRANSFER_VERSION, InboundKeyMaterial, InboundTransferFailure,
    InboundTransferPhase, InboundTransferProgress,
};
use synthetic_receive::{InboundMaterialSlot, SyntheticTransferReceiver};
pub use synthetic_transfer::{
    KEY_TRANSFER_VERSION, KeyTransferPurpose, SYNTHETIC_KEY_CHUNK_BYTES, SYNTHETIC_KEY_MAX_BYTES,
    SYNTHETIC_KEY_TIMEOUT, SyntheticKeyTransferSend, SyntheticTransferError,
    SyntheticTransferOutbound, SyntheticTransferPhase, SyntheticTransferProgress,
    SyntheticTransferSender,
};

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError as StdTrySendError, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use zeroize::Zeroize;

use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_pairing::{EstablishedSession, PairingError};
use kryptopass_protocol::{Channel, MessagePayload, proto};
use kryptopass_sync::{
    Ledger, SyncOutbound, SyncProgress, SyncSession, SyncSessionError, TrustSource,
};
use kryptopass_transport::PeerTransport;
pub use kryptopass_transport::{ConnectionFamily, ConnectionInfo, ConnectionRoute};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Instant, sleep_until};

/// Maximum number of trusted peers accepted by the public read model.
pub const MAX_TRUSTED_DEVICES: usize = 1024;
/// Active-session extension version understood by this implementation.
pub const ACTIVE_SESSION_VERSION: u32 = 1;
/// Session capability: the peer may declare its OpenPGP keys in `VaultKeys`.
const VAULT_KEYS_VERSION: u32 = 1;
/// Interval between keepalive requests when the previous request was answered.
pub const SESSION_PING_INTERVAL: Duration = Duration::from_secs(3);
/// Maximum time allowed for the outstanding keepalive request.
pub const SESSION_PING_TIMEOUT: Duration = Duration::from_secs(6);
/// Maximum time allowed for the initial authenticated session hello.
pub const SESSION_HELLO_TIMEOUT: Duration = Duration::from_secs(30);

const SESSION_COMMAND_CAPACITY: usize = 1;
const SESSION_EVENT_CAPACITY: usize = 16;
const MAX_CONTROL_MESSAGES_PER_SECOND: usize = 32;
const MAX_DATA_MESSAGES_PER_SECOND: usize = 4096;
const MAX_SYNC_CONTROL_MESSAGES_PER_SECOND: usize = 2048;
const SESSION_CONTROL_SEND_TIMEOUT: Duration = Duration::from_secs(10);
const SYNC_WORKER_INPUT_CAPACITY: usize = 4;
const SYNC_WORKER_OUTPUT_CAPACITY: usize = 2;

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

/// Explicit opt-in binding for one existing ciphertext ledger and trust source.
///
/// Supplying this value is the only way to advertise Sync v1 from a session.
/// It does not select a vault, create trust, or inspect an OpenPGP key.
pub struct SyncSessionBinding {
    ledger: Ledger,
    trust: Box<dyn TrustSource + Send>,
    openpgp_fingerprint: Vec<u8>,
    automatic_rounds: bool,
    key_agnostic_hello: bool,
}

impl SyncSessionBinding {
    /// Binds an existing ledger and trust source to the caller-selected key fingerprint.
    #[must_use]
    pub fn new(
        ledger: Ledger,
        trust: Box<dyn TrustSource + Send>,
        openpgp_fingerprint: Vec<u8>,
    ) -> Self {
        Self {
            ledger,
            trust,
            openpgp_fingerprint,
            automatic_rounds: false,
            key_agnostic_hello: false,
        }
    }

    /// Accepts any valid OpenPGP fingerprint in the peer's hello (random-vault-ID bindings).
    #[must_use]
    pub fn with_key_agnostic_hello(mut self, enabled: bool) -> Self {
        self.key_agnostic_hello = enabled;
        self
    }

    /// Opts this binding into automatic follow-up rounds when both peers
    /// advertise continuous Sync support in their authenticated session hello.
    #[must_use]
    pub fn with_automatic_rounds(mut self) -> Self {
        self.automatic_rounds = true;
        self
    }
}

impl fmt::Debug for SyncSessionBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncSessionBinding")
            .field("vault_id", &"[redacted]")
            .field("fingerprint", &"[redacted]")
            .finish()
    }
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
    /// An explicitly supplied Sync binding has an invalid fingerprint shape.
    #[error("sync session binding fingerprint is invalid")]
    InvalidSyncBinding,
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
    /// Keys each peer declared in `VaultKeys`, with the session generation that sent them.
    peer_vault_keys: std::collections::HashMap<DeviceId, (u64, Vec<Vec<u8>>)>,
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

    /// Keeps the keys `peer` declared for this exact connection generation.
    fn set_peer_vault_keys(
        &self,
        peer: &PublicIdentity,
        generation: u64,
        keys: Vec<Vec<u8>>,
    ) -> Result<(), DeviceDirectoryError> {
        let mut mutation = self.lock_mutations()?;
        let current = self.snapshot();
        current
            .get(&peer.device_id)
            .filter(|device| {
                device.trust == TrustState::Trusted
                    && device.identity == *peer
                    && device.connection_generation == generation
            })
            .ok_or(DeviceDirectoryError::PeerNotTrusted)?;
        mutation
            .peer_vault_keys
            .insert(peer.device_id, (generation, keys));
        // Observers read the keys through the snapshot, so announce a new revision.
        let mut next = (*current).clone();
        next.revision = current
            .revision
            .checked_add(1)
            .ok_or(DeviceDirectoryError::RevisionExhausted)?;
        self.snapshots.send_replace(Arc::new(next));
        Ok(())
    }

    /// Public fingerprints that `peer` declared holding private keys for, if its
    /// current connection declared any. They are the peer's claim, used only to
    /// narrow what Desktop offers to sync, never to authorize anything.
    #[must_use]
    pub fn peer_vault_keys(&self, peer: &DeviceId) -> Option<Vec<Vec<u8>>> {
        let mutation = self.lock_mutations().ok()?;
        let (generation, keys) = mutation.peer_vault_keys.get(peer)?;
        let current = self.snapshot();
        let device = current.get(peer)?;
        (device.connection == ConnectionState::Connected
            && device.connection_generation == *generation)
            .then(|| keys.clone())
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
    /// The negotiated Sync round failed a bounded protocol or storage check.
    Sync,
    /// A local Sync storage or random-source operation failed.
    SyncStorage,
    /// A Sync request or transfer timed out.
    SyncTimeout,
    /// The peer sent opaque data frames above the separate local bound.
    DataRate,
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

enum SyntheticCommand {
    Start(Box<SyntheticKeyTransferSend>),
    /// Local decision on the inbound offer with this id.
    InboundDecision([u8; 16], bool),
    /// Application result of import + challenge decryption; `None` is a failure.
    InboundFinished([u8; 16], Option<[u8; 32]>),
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
    sync_progress: watch::Receiver<Option<SyncProgress>>,
    synthetic_commands: mpsc::Sender<SyntheticCommand>,
    synthetic_cancel: watch::Sender<Option<[u8; 16]>>,
    synthetic_progress: watch::Receiver<SyntheticTransferProgress>,
    inbound_material: InboundMaterialSlot,
    closed: watch::Receiver<bool>,
    generation: u64,
}

/// Cloneable command/observation capability scoped to one authenticated session.
/// Dropping this capability does not disconnect the session or duplicate its owner.
#[derive(Clone)]
pub struct SyntheticTransferControl {
    peer_id: DeviceId,
    generation: u64,
    directory: DeviceDirectory,
    synthetic_commands: mpsc::Sender<SyntheticCommand>,
    synthetic_cancel: watch::Sender<Option<[u8; 16]>>,
    synthetic_progress: watch::Receiver<SyntheticTransferProgress>,
    inbound_material: InboundMaterialSlot,
}

impl SyntheticTransferControl {
    /// The generation to bind a local approval to before starting preparation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Authenticated destination of this scoped capability.
    #[must_use]
    pub const fn peer_id(&self) -> DeviceId {
        self.peer_id
    }
    /// Queues an explicitly approved synthetic export for this exact session generation.
    pub fn start_synthetic_key_transfer(
        &self,
        expected_generation: u64,
        prepared: SyntheticKeyTransferSend,
    ) -> Result<(), SyntheticTransferError> {
        self.check_synthetic_session(expected_generation)?;
        let progress = *self.synthetic_progress.borrow();
        match progress.phase {
            SyntheticTransferPhase::Unsupported => return Err(SyntheticTransferError::Unsupported),
            SyntheticTransferPhase::Ready => {}
            _ => return Err(SyntheticTransferError::Busy),
        }
        // One operation per session counting both directions.
        if progress
            .inbound
            .is_some_and(|inbound| inbound.phase.is_active())
        {
            return Err(SyntheticTransferError::Busy);
        }
        self.synthetic_commands
            .try_send(SyntheticCommand::Start(Box::new(prepared)))
            .map_err(|_| SyntheticTransferError::Busy)
    }

    /// Cancels only an active operation from this exact authenticated generation.
    pub fn cancel_synthetic_key_transfer(
        &self,
        expected_generation: u64,
        transfer_id: [u8; 16],
    ) -> Result<(), SyntheticTransferError> {
        self.check_synthetic_session(expected_generation)?;
        let progress = *self.synthetic_progress.borrow();
        // A local cancellation may arrive before the queued start is consumed.
        // Both commands retain FIFO order and target this exact generation.
        if progress.phase != SyntheticTransferPhase::Ready
            && progress.transfer_id != Some(transfer_id)
        {
            return Err(SyntheticTransferError::WrongSession);
        }
        self.synthetic_cancel
            .send(Some(transfer_id))
            .map_err(|_| SyntheticTransferError::WrongSession)
    }

    /// Sends the local user's decision for the inbound offer `transfer_id`.
    pub fn decide_inbound_key_transfer(
        &self,
        expected_generation: u64,
        transfer_id: [u8; 16],
        accepted: bool,
    ) -> Result<(), SyntheticTransferError> {
        self.check_synthetic_session(expected_generation)?;
        let awaiting = self
            .synthetic_progress
            .borrow()
            .inbound
            .is_some_and(|inbound| {
                inbound.transfer_id == transfer_id
                    && inbound.phase == InboundTransferPhase::AwaitingDecision
            });
        if !awaiting {
            return Err(SyntheticTransferError::WrongSession);
        }
        self.synthetic_commands
            .try_send(SyntheticCommand::InboundDecision(transfer_id, accepted))
            .map_err(|_| SyntheticTransferError::Busy)
    }

    /// Takes the verified protected key block once the inbound operation
    /// reached the import stage. Only one caller can obtain it.
    pub fn take_inbound_key_material(
        &self,
        expected_generation: u64,
        transfer_id: [u8; 16],
    ) -> Result<InboundKeyMaterial, SyntheticTransferError> {
        self.check_synthetic_session(expected_generation)?;
        let mut slot = self
            .inbound_material
            .lock()
            .map_err(|_| SyntheticTransferError::Invalid)?;
        if slot
            .as_ref()
            .is_none_or(|material| material.transfer_id() != transfer_id)
        {
            return Err(SyntheticTransferError::WrongSession);
        }
        slot.take().ok_or(SyntheticTransferError::WrongSession)
    }

    /// Reports the application's import result: the SHA-256 of the decrypted
    /// challenge sends the receipt; `None` aborts with code 4.
    pub fn finish_inbound_key_transfer(
        &self,
        expected_generation: u64,
        transfer_id: [u8; 16],
        challenge_sha256: Option<[u8; 32]>,
    ) -> Result<(), SyntheticTransferError> {
        self.check_synthetic_session(expected_generation)?;
        self.synthetic_commands
            .try_send(SyntheticCommand::InboundFinished(
                transfer_id,
                challenge_sha256,
            ))
            .map_err(|_| SyntheticTransferError::Busy)
    }

    fn check_synthetic_session(
        &self,
        expected_generation: u64,
    ) -> Result<(), SyntheticTransferError> {
        let snapshot = self.directory.snapshot();
        let device = snapshot
            .get(&self.peer_id)
            .ok_or(SyntheticTransferError::WrongSession)?;
        if expected_generation != self.generation
            || device.connection != ConnectionState::Connected
            || !self
                .directory
                .session_is_current(&device.identity, self.generation)
        {
            return Err(SyntheticTransferError::WrongSession);
        }
        Ok(())
    }

    /// Returns current metadata, without private key material.
    #[must_use]
    pub fn synthetic_key_transfer_progress(&self) -> SyntheticTransferProgress {
        *self.synthetic_progress.borrow()
    }

    /// Subscribes to coalesced metadata for this session only.
    #[must_use]
    pub fn subscribe_synthetic_key_transfer(&self) -> watch::Receiver<SyntheticTransferProgress> {
        self.synthetic_progress.clone()
    }
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

    /// Retains a cloneable scoped transfer capability, without sharing cipher state.
    #[must_use]
    pub fn synthetic_key_transfer_control(&self) -> SyntheticTransferControl {
        SyntheticTransferControl {
            peer_id: self.peer_id,
            generation: self.generation,
            directory: self.directory.clone(),
            synthetic_commands: self.synthetic_commands.clone(),
            synthetic_cancel: self.synthetic_cancel.clone(),
            synthetic_progress: self.synthetic_progress.clone(),
            inbound_material: Arc::clone(&self.inbound_material),
        }
    }

    /// Queues an explicitly approved operation bound to the current session.
    pub fn start_synthetic_key_transfer(
        &self,
        expected_generation: u64,
        prepared: SyntheticKeyTransferSend,
    ) -> Result<(), SyntheticTransferError> {
        self.synthetic_key_transfer_control()
            .start_synthetic_key_transfer(expected_generation, prepared)
    }

    /// Cancels an operation in the current generation only.
    pub fn cancel_synthetic_key_transfer(
        &self,
        expected_generation: u64,
        transfer_id: [u8; 16],
    ) -> Result<(), SyntheticTransferError> {
        self.synthetic_key_transfer_control()
            .cancel_synthetic_key_transfer(expected_generation, transfer_id)
    }

    /// Returns secret-free transfer state for this exact session.
    #[must_use]
    pub fn synthetic_key_transfer_progress(&self) -> SyntheticTransferProgress {
        *self.synthetic_progress.borrow()
    }

    /// Observes coalesced transfer state for this exact session.
    #[must_use]
    pub fn subscribe_synthetic_key_transfer(&self) -> watch::Receiver<SyntheticTransferProgress> {
        self.synthetic_progress.clone()
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

    /// Returns the latest volatile progress for the explicitly bound Sync round.
    #[must_use]
    pub fn sync_progress(&self) -> Option<SyncProgress> {
        *self.sync_progress.borrow()
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

enum SyncWorkerInput {
    Message(proto::MessageType, MessagePayload),
    Cancel,
}

enum SyncWorkerOutput {
    Outbound(SyncOutbound),
    Progress(SyncProgress),
    Failed(SyncSessionError),
    RoundComplete,
    Complete,
}

struct SyncWorker {
    input: SyncSender<SyncWorkerInput>,
    outputs: mpsc::Receiver<SyncWorkerOutput>,
    cancelled: Arc<AtomicBool>,
}

struct SyncRuntime {
    worker: Option<SyncWorker>,
    progress: watch::Sender<Option<SyncProgress>>,
    synthetic: SyntheticRuntime,
    /// The peer advertised `VaultKeys` support, so it may send that message.
    accepts_vault_keys: bool,
}

struct SyntheticRuntime {
    actor: SyntheticTransferSender,
    receiver: SyntheticTransferReceiver,
    commands: mpsc::Receiver<SyntheticCommand>,
    cancel: watch::Receiver<Option<[u8; 16]>>,
    progress: watch::Sender<SyntheticTransferProgress>,
}

impl SyntheticRuntime {
    /// Combined secret-free state of both directions.
    fn snapshot(&self) -> SyntheticTransferProgress {
        let mut progress = self.actor.progress();
        progress.inbound = self.receiver.progress();
        progress
    }

    fn publish(&self) {
        let current = self.snapshot();
        self.progress.send_if_modified(|value| {
            if *value == current {
                false
            } else {
                *value = current;
                true
            }
        });
    }

    fn sender_active(&self) -> bool {
        matches!(
            self.actor.progress().phase,
            SyntheticTransferPhase::WaitingAcceptance
                | SyntheticTransferPhase::Sending
                | SyntheticTransferPhase::WaitingReceipt
        )
    }
}

impl Drop for SyntheticRuntime {
    fn drop(&mut self) {
        self.actor.disconnected();
        self.receiver.disconnected();
        self.progress.send_replace(self.snapshot());
    }
}

impl Drop for SyncWorker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.input.try_send(SyncWorkerInput::Cancel);
    }
}

fn spawn_sync_worker(
    binding: SyncSessionBinding,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    continuous_rounds: bool,
) -> SyncWorker {
    let (input, input_rx) = sync_channel(SYNC_WORKER_INPUT_CAPACITY);
    let (output_tx, outputs) = mpsc::channel(SYNC_WORKER_OUTPUT_CAPACITY);
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    tokio::task::spawn_blocking(move || {
        let continuous_rounds = continuous_rounds && binding.automatic_rounds;
        let key_agnostic_hello = binding.key_agnostic_hello;
        let mut engine = match SyncSession::new(
            binding.ledger,
            binding.trust,
            local_device_id,
            expected_peer,
            binding.openpgp_fingerprint,
            std::time::Instant::now(),
        ) {
            Ok(engine) => engine,
            Err(error) => {
                let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                return;
            }
        };
        engine = engine.with_ignored_peer_key(key_agnostic_hello);
        if continuous_rounds {
            if let Err(error) = engine.enable_continuous_rounds() {
                let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                return;
            }
        }
        let mut last_progress = engine.progress();
        if cfg!(debug_assertions) {
            tracing::info!(continuous_rounds, diagnostics = %engine.diagnostics(std::time::Instant::now()), "Sync started");
        }
        let mut round_complete_reported = false;
        let mut next_local_change_check = std::time::Instant::now();
        if output_tx
            .blocking_send(SyncWorkerOutput::Progress(last_progress))
            .is_err()
        {
            return;
        }

        loop {
            if worker_cancelled.load(Ordering::Acquire) {
                engine.cancel();
                return;
            }
            let now = std::time::Instant::now();
            if engine.is_complete() {
                if !continuous_rounds {
                    let _ = output_tx.blocking_send(SyncWorkerOutput::Progress(engine.progress()));
                    let _ = output_tx.blocking_send(SyncWorkerOutput::Complete);
                    return;
                }
                if !round_complete_reported {
                    let progress = engine.progress();
                    if progress != last_progress {
                        last_progress = progress;
                        if output_tx
                            .blocking_send(SyncWorkerOutput::Progress(progress))
                            .is_err()
                        {
                            return;
                        }
                    }
                    if output_tx
                        .blocking_send(SyncWorkerOutput::RoundComplete)
                        .is_err()
                    {
                        return;
                    }
                    round_complete_reported = true;
                }
            }
            if continuous_rounds && now >= next_local_change_check {
                match engine.has_uncovered_local_revisions() {
                    Ok(true) => {
                        if !engine.request_round() {
                            let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(
                                SyncSessionError::Protocol,
                            ));
                            return;
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                        return;
                    }
                }
                next_local_change_check = now + kryptopass_sync::SESSION_LOCAL_CHANGE_POLL;
            }
            if let Err(error) = engine.check_time(now) {
                if cfg!(debug_assertions) {
                    tracing::warn!(error = %error, diagnostics = %engine.diagnostics(now), "Sync deadline or trust check failed");
                }
                let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                return;
            }
            match engine.poll_output(now) {
                Ok(Some(outbound)) => {
                    if cfg!(debug_assertions) {
                        let counts = match &outbound {
                            SyncOutbound::Hello(message) => {
                                format!("hello revisions={}", message.known_revision_count)
                            }
                            SyncOutbound::Manifest(message) => format!(
                                "manifest page={} last={} heads={}",
                                message.page_index,
                                message.last_page,
                                message.heads.len()
                            ),
                            SyncOutbound::Request(message) => {
                                format!("request revisions={}", message.revisions.len())
                            }
                            SyncOutbound::Chunk(message) => format!(
                                "chunk index={} chunks={} bytes={}",
                                message.chunk_index,
                                message.chunk_count,
                                message.data.len()
                            ),
                            SyncOutbound::Commit(_) => "commit".to_owned(),
                            SyncOutbound::Ack(message) => format!("ack status={}", message.status),
                        };
                        if !matches!(&outbound, SyncOutbound::Chunk(message) if message.chunk_index != 0 && message.chunk_index + 1 != message.chunk_count)
                        {
                            tracing::debug!(message = %counts, diagnostics = %engine.diagnostics(now), "Sync tx_queued");
                        }
                    }
                    if output_tx
                        .blocking_send(SyncWorkerOutput::Outbound(outbound))
                        .is_err()
                    {
                        return;
                    }
                    let progress = engine.progress();
                    if progress != last_progress {
                        last_progress = progress;
                        if output_tx
                            .blocking_send(SyncWorkerOutput::Progress(progress))
                            .is_err()
                        {
                            return;
                        }
                    }
                    if !engine.is_complete() {
                        round_complete_reported = false;
                    }
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    if cfg!(debug_assertions) {
                        tracing::warn!(error = %error, diagnostics = %engine.diagnostics(now), "Sync output failed");
                    }
                    let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                    return;
                }
            }
            let now = std::time::Instant::now();
            let wait = if continuous_rounds && engine.is_complete() {
                next_local_change_check.saturating_duration_since(now)
            } else {
                let timeout = engine.timeout_after(now);
                if continuous_rounds {
                    timeout.min(kryptopass_sync::SESSION_LOCAL_CHANGE_POLL)
                } else {
                    timeout
                }
            };
            let received = input_rx.recv_timeout(wait.max(Duration::from_millis(1)));
            let input = match received {
                Ok(input) => input,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    engine.cancel();
                    return;
                }
            };
            if worker_cancelled.load(Ordering::Acquire) {
                engine.cancel();
                return;
            }
            let result = match input {
                SyncWorkerInput::Message(message_type, payload) => {
                    if cfg!(debug_assertions) {
                        let counts = match &payload {
                            MessagePayload::SyncHello(message) => {
                                format!("revisions={}", message.known_revision_count)
                            }
                            MessagePayload::SyncManifest(message) => format!(
                                "page={} last={} heads={}",
                                message.page_index,
                                message.last_page,
                                message.heads.len()
                            ),
                            MessagePayload::SyncRequest(message) => {
                                format!("revisions={}", message.revisions.len())
                            }
                            MessagePayload::SyncChunk(message) => format!(
                                "chunk={} chunks={} bytes={}",
                                message.chunk_index,
                                message.chunk_count,
                                message.data.len()
                            ),
                            MessagePayload::SyncAck(message) => {
                                format!("status={}", message.status)
                            }
                            _ => String::new(),
                        };
                        if !matches!(&payload, MessagePayload::SyncChunk(message) if message.chunk_index != 0 && message.chunk_index + 1 != message.chunk_count)
                        {
                            tracing::debug!(message_type = message_type.as_str_name(), message = %counts, diagnostics = %engine.diagnostics(std::time::Instant::now()), "Sync rx");
                        }
                    }
                    engine.handle_message(message_type, payload, std::time::Instant::now())
                }
                SyncWorkerInput::Cancel => {
                    engine.cancel();
                    return;
                }
            };
            if let Err(error) = result {
                if cfg!(debug_assertions) {
                    tracing::warn!(error = %error, diagnostics = %engine.diagnostics(std::time::Instant::now()), "Sync input rejected");
                }
                let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                return;
            }
            let progress = engine.progress();
            if progress != last_progress {
                if cfg!(debug_assertions) {
                    tracing::info!(diagnostics = %engine.diagnostics(std::time::Instant::now()), "Sync progress");
                }
                last_progress = progress;
                if output_tx
                    .blocking_send(SyncWorkerOutput::Progress(progress))
                    .is_err()
                {
                    return;
                }
            }
            if !engine.is_complete() {
                round_complete_reported = false;
            }
        }
    });
    SyncWorker {
        input,
        outputs,
        cancelled,
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
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    options: SessionOptions,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_internal(
        directory,
        session,
        local_device_id,
        expected_peer,
        options,
        None,
        false,
    )
}

/// Starts a session with an explicit ledger binding and default reconnect options.
pub fn start_session_with_sync<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    sync_binding: SyncSessionBinding,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_with_options_and_sync(
        directory,
        session,
        local_device_id,
        expected_peer,
        SessionOptions::default(),
        sync_binding,
    )
}

/// Starts a session with an explicit ledger binding and reconnect options.
pub fn start_session_with_options_and_sync<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    options: SessionOptions,
    sync_binding: SyncSessionBinding,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_internal(
        directory,
        session,
        local_device_id,
        expected_peer,
        options,
        Some(sync_binding),
        false,
    )
}

/// Starts the unique owner with explicit optional extensions. Production callers
/// keep synthetic provisioning disabled; laboratory callers opt in locally.
pub fn start_session_with_extensions<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    options: SessionOptions,
    sync_binding: Option<SyncSessionBinding>,
    synthetic_key_transfer: bool,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_internal(
        directory,
        session,
        local_device_id,
        expected_peer,
        options,
        sync_binding,
        synthetic_key_transfer,
    )
}

fn start_session_internal<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    mut session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    options: SessionOptions,
    sync_binding: Option<SyncSessionBinding>,
    synthetic_key_transfer: bool,
) -> Result<SessionHandle, DeviceDirectoryError> {
    let synthetic_key_transfer = synthetic_key_transfer && cfg!(debug_assertions);
    if sync_binding.as_ref().is_some_and(|binding| {
        !matches!(binding.openpgp_fingerprint.len(), 20 | 32)
            || binding.openpgp_fingerprint.iter().all(|byte| *byte == 0)
    }) {
        return Err(DeviceDirectoryError::InvalidSyncBinding);
    }
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
    let (sync_progress_tx, sync_progress_rx) = watch::channel(None);
    let (closed_tx, closed_rx) = watch::channel(false);
    let (synthetic_tx, synthetic_rx) = mpsc::channel(2);
    let (synthetic_cancel_tx, synthetic_cancel_rx) = watch::channel(None);
    let (synthetic_progress_tx, synthetic_progress_rx) =
        watch::channel(SyntheticTransferSender::new(false).progress());
    let inbound_material: InboundMaterialSlot = Arc::new(Mutex::new(None));
    let owner_inbound_material = Arc::clone(&inbound_material);
    let owner_directory = directory.clone();
    let owner_events = event_tx.clone();
    let mut directory_changes = directory.subscribe();
    runtime.spawn(async move {
        let session_started = Instant::now();
        let local_sync_rounds_version = u32::from(
            sync_binding
                .as_ref()
                .is_some_and(|binding| binding.automatic_rounds),
        );
        let hello_context = SessionHelloContext {
            local_device_id,
            options,
            sync_version: u32::from(sync_binding.is_some()),
            sync_rounds_version: local_sync_rounds_version,
            synthetic_key_transfer_version: if synthetic_key_transfer {
                KEY_TRANSFER_VERSION
            } else {
                0
            },
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
            Ok((
                remote_reconnect_port,
                remote_sync_version,
                remote_sync_rounds_version,
                remote_synthetic_version,
                remote_vault_keys_version,
            )) if owner_directory.session_is_current(&expected_peer, generation) => {
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
                    let sync_enabled = sync_binding.is_some() && remote_sync_version == 1;
                    let continuous_rounds = sync_enabled
                        && local_sync_rounds_version == 1
                        && remote_sync_rounds_version == 1;
                    tracing::info!(
                        local_binding = sync_binding.is_some(),
                        remote_sync_version,
                        local_sync_rounds_version,
                        remote_sync_rounds_version,
                        sync_enabled,
                        continuous_rounds,
                        "Sync negotiation"
                    );
                    let sync_worker = if sync_enabled {
                        sync_binding.map(|binding| {
                            spawn_sync_worker(
                                binding,
                                local_device_id,
                                expected_peer,
                                continuous_rounds,
                            )
                        })
                    } else {
                        None
                    };
                    let effective_synthetic_version = if synthetic_key_transfer {
                        remote_synthetic_version.min(KEY_TRANSFER_VERSION)
                    } else {
                        0
                    };
                    tracing::info!(
                        local_enabled = synthetic_key_transfer,
                        local_version = KEY_TRANSFER_VERSION,
                        remote_synthetic_version,
                        effective_synthetic_version,
                        "Key transfer negotiation"
                    );
                    let synthetic_actor =
                        SyntheticTransferSender::with_version(effective_synthetic_version);
                    let synthetic_receiver = SyntheticTransferReceiver::new(
                        effective_synthetic_version,
                        owner_inbound_material,
                    );
                    synthetic_progress_tx.send_replace(synthetic_actor.progress());
                    run_connected_session(
                        &mut session,
                        &owner_directory,
                        &mut directory_changes,
                        &mut command_rx,
                        expected_peer,
                        generation,
                        SyncRuntime {
                            worker: sync_worker,
                            progress: sync_progress_tx.clone(),
                            accepts_vault_keys: remote_vault_keys_version == VAULT_KEYS_VERSION,
                            synthetic: SyntheticRuntime {
                                actor: synthetic_actor,
                                receiver: synthetic_receiver,
                                commands: synthetic_rx,
                                cancel: synthetic_cancel_rx,
                                progress: synthetic_progress_tx.clone(),
                            },
                        },
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
        sync_progress: sync_progress_rx,
        synthetic_commands: synthetic_tx,
        synthetic_cancel: synthetic_cancel_tx,
        synthetic_progress: synthetic_progress_rx,
        inbound_material,
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
    sync_version: u32,
    sync_rounds_version: u32,
    synthetic_key_transfer_version: u32,
    expected_peer: PublicIdentity,
    generation: u64,
    directory: &'a DeviceDirectory,
}

async fn exchange_session_hello<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    context: &SessionHelloContext<'_>,
    directory_changes: &mut watch::Receiver<Arc<DeviceDirectorySnapshot>>,
    commands: &mut mpsc::Receiver<SessionCommand>,
) -> Result<(u16, u32, u32, u32, u32), SessionExit> {
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
        synthetic_key_transfer_version: context.synthetic_key_transfer_version,
        sync_version: context.sync_version,
        sync_rounds_version: context.sync_rounds_version,
        vault_keys_version: VAULT_KEYS_VERSION,
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
            Ok((
                hello.reconnect_port as u16,
                hello.sync_version,
                hello.sync_rounds_version,
                hello.synthetic_key_transfer_version,
                hello.vault_keys_version,
            ))
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
    sync_runtime: SyncRuntime,
) -> SessionExit {
    let SyncRuntime {
        worker: mut sync_worker,
        progress: sync_progress,
        mut synthetic,
        accepts_vault_keys,
    } = sync_runtime;
    let mut next_observation = Instant::now();
    let mut next_local_nonce = 1_u64;
    let mut last_remote_nonce = 0_u64;
    let mut outstanding: Option<(u64, Instant)> = None;
    let mut next_ping_at = Instant::now() + SESSION_PING_INTERVAL;
    let mut rate_limiter = ControlRateLimiter::default();
    let mut data_rate_limiter = DataRateLimiter::default();
    let mut sync_rate_limiter = DataRateLimiter::default();
    // Retain at most one decoded frame while a bounded storage queue is busy.
    // Keep draining worker output and observing cancellation instead of treating
    // an ordinary scheduler/SQLite delay as a malformed peer message.
    let mut pending_sync_input: Option<(SyncWorkerInput, Instant)> = None;
    // A Sync failure suspends Sync for the rest of this session instead of
    // closing it; only revoked trust still ends the authenticated connection.
    let mut sync_suspended = false;
    let suspend_sync = |worker: &mut Option<SyncWorker>,
                        pending: &mut Option<(SyncWorkerInput, Instant)>,
                        suspended: &mut bool,
                        reason: &'static str| {
        if !*suspended {
            tracing::warn!(
                reason,
                "Sync suspended for this session; the connection stays up"
            );
        }
        *worker = None;
        *pending = None;
        *suspended = true;
        sync_progress.send_modify(|progress| {
            if let Some(progress) = progress {
                progress.state = kryptopass_sync::SyncProgressState::Failed;
            }
        });
    };

    loop {
        if !directory.session_is_current(&expected_peer, generation) {
            return SessionExit::Disconnected(SessionCloseReason::TrustRevoked, true);
        }
        match commands.try_recv() {
            Ok(SessionCommand::Disconnect(reason)) => {
                return SessionExit::Disconnected(reason, true);
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                return SessionExit::Disconnected(SessionCloseReason::AppClosing, true);
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
        }
        // Give already queued human cancellation priority over the next key chunk.
        // The separate disconnect queue remains observable even under provisioning.
        for _ in 0..2 {
            let Ok(command) = synthetic.commands.try_recv() else {
                break;
            };
            if let Err(failure) = process_synthetic_command(session, &mut synthetic, command).await
            {
                return SessionExit::Failed(failure);
            }
        }
        let requested_cancel = *synthetic.cancel.borrow_and_update();
        if let Some(id) = requested_cancel {
            if synthetic.actor.progress().transfer_id == Some(id)
                && matches!(
                    synthetic.actor.progress().phase,
                    SyntheticTransferPhase::WaitingAcceptance
                        | SyntheticTransferPhase::Sending
                        | SyntheticTransferPhase::WaitingReceipt
                )
            {
                if let Ok(abort) = synthetic.actor.cancel(id, Instant::now()) {
                    if let Err(failure) = send_synthetic_outbound_bounded(session, abort).await {
                        return SessionExit::Failed(failure);
                    }
                }
            }
        }
        match synthetic.actor.next_outbound(Instant::now()) {
            Ok(Some(outbound)) => {
                if let Err(error) = send_synthetic_outbound_bounded(session, outbound).await {
                    return SessionExit::Failed(error);
                }
            }
            Ok(None) => {}
            Err(_) => return SessionExit::Failed(SessionFailure::Protocol),
        }
        if let Some(abort) = synthetic.receiver.expire(Instant::now()) {
            if let Err(error) = send_synthetic_outbound_bounded(session, abort).await {
                return SessionExit::Failed(error);
            }
        }
        synthetic.publish();
        if let Some((input, deadline)) = pending_sync_input.take() {
            if let Some(worker) = sync_worker.as_ref() {
                match worker.input.try_send(input) {
                    Ok(()) => {}
                    Err(StdTrySendError::Full(input)) if Instant::now() < deadline => {
                        pending_sync_input = Some((input, deadline));
                    }
                    Err(_) => suspend_sync(
                        &mut sync_worker,
                        &mut pending_sync_input,
                        &mut sync_suspended,
                        "worker input queue full past its deadline or closed",
                    ),
                }
            } else if !sync_suspended {
                tracing::warn!("Sync input arrived without a sync worker");
                return SessionExit::Failed(SessionFailure::Sync);
            }
        }
        let ping_deadline = outstanding
            .map(|(_, deadline)| deadline)
            .unwrap_or(next_ping_at);
        let selected = tokio::select! {
            () = sleep_until(next_observation) => SelectedSessionEvent::ObservePath,
            command = commands.recv() => SelectedSessionEvent::Command(command),
            command = synthetic.commands.recv() => SelectedSessionEvent::SyntheticCommand(command),
            _ = synthetic.cancel.changed() => SelectedSessionEvent::SyntheticTick,
            () = tokio::time::sleep(Duration::from_millis(if synthetic.actor.progress().phase == SyntheticTransferPhase::Sending { 1 } else { 100 })),
                if synthetic.sender_active() || synthetic.receiver.is_active() => SelectedSessionEvent::SyntheticTick,
            changed = directory_changes.changed() => SelectedSessionEvent::DirectoryChanged(changed),
            message = session.receive_message(), if pending_sync_input.is_none() => SelectedSessionEvent::Message(message),
            output = receive_sync_worker_output(&mut sync_worker), if sync_worker.is_some() => {
                SelectedSessionEvent::SyncOutput(output)
            },
            () = sleep_until(next_ping_at), if outstanding.is_none() => SelectedSessionEvent::PingDue,
            () = sleep_until(ping_deadline), if outstanding.is_some() => SelectedSessionEvent::PingTimeout,
            () = tokio::time::sleep(Duration::from_millis(1)), if pending_sync_input.is_some() => SelectedSessionEvent::RetrySyncInput,
        };

        if !directory.session_is_current(&expected_peer, generation) {
            return SessionExit::Disconnected(SessionCloseReason::TrustRevoked, true);
        }
        match selected {
            SelectedSessionEvent::RetrySyncInput | SelectedSessionEvent::SyntheticTick => {}
            SelectedSessionEvent::SyntheticCommand(Some(command)) => {
                if let Err(failure) =
                    process_synthetic_command(session, &mut synthetic, command).await
                {
                    return SessionExit::Failed(failure);
                }
            }
            SelectedSessionEvent::SyntheticCommand(None) => {
                return SessionExit::Disconnected(SessionCloseReason::AppClosing, true);
            }
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
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::Outbound(outbound))) => {
                let kind = match &outbound {
                    SyncOutbound::Hello(_) => Some("hello"),
                    SyncOutbound::Manifest(_) => Some("manifest"),
                    SyncOutbound::Request(_) => Some("request"),
                    SyncOutbound::Chunk(message)
                        if message.chunk_index == 0
                            || message.chunk_index + 1 == message.chunk_count =>
                    {
                        Some("chunk")
                    }
                    SyncOutbound::Chunk(_) => None,
                    SyncOutbound::Commit(_) => Some("commit"),
                    SyncOutbound::Ack(_) => Some("ack"),
                };
                if let Err(failure) = send_sync_outbound_bounded(session, outbound).await {
                    return SessionExit::Failed(failure);
                }
                if cfg!(debug_assertions) {
                    if let Some(kind) = kind {
                        tracing::debug!(message_type = kind, "Sync tx_sent");
                    }
                }
            }
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::Progress(progress))) => {
                sync_progress.send_replace(Some(progress));
            }
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::Failed(error))) => {
                tracing::warn!(error = %error, "Sync worker failed");
                if matches!(error, SyncSessionError::TrustRevoked) {
                    return SessionExit::Disconnected(SessionCloseReason::TrustRevoked, true);
                }
                suspend_sync(
                    &mut sync_worker,
                    &mut pending_sync_input,
                    &mut sync_suspended,
                    "worker failed",
                );
            }
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::Complete)) => {
                sync_worker = None;
            }
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::RoundComplete)) => {}
            SelectedSessionEvent::SyncOutput(None) => {
                suspend_sync(
                    &mut sync_worker,
                    &mut pending_sync_input,
                    &mut sync_suspended,
                    "worker stopped without a result",
                );
            }
            SelectedSessionEvent::Message(Err(error)) => {
                return SessionExit::Failed(classify_pairing_error(error));
            }
            SelectedSessionEvent::Message(Ok(message)) => {
                match message.channel {
                    Channel::Control
                        if !(if is_sync_message(message.message_type) {
                            sync_rate_limiter
                                .accept_limit(Instant::now(), MAX_SYNC_CONTROL_MESSAGES_PER_SECOND)
                        } else {
                            rate_limiter.accept(Instant::now())
                        }) =>
                    {
                        return SessionExit::Failed(SessionFailure::ControlRate);
                    }
                    Channel::Data if !data_rate_limiter.accept(Instant::now()) => {
                        return SessionExit::Failed(SessionFailure::DataRate);
                    }
                    Channel::Control | Channel::Data => {}
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
                    (proto::MessageType::VaultKeys, MessagePayload::VaultKeys(keys)) => {
                        // Only a peer that advertised the capability may send it.
                        if !accepts_vault_keys {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        }
                        let count = keys.openpgp_fingerprints.len();
                        if directory
                            .set_peer_vault_keys(
                                &expected_peer,
                                generation,
                                keys.openpgp_fingerprints,
                            )
                            .is_err()
                        {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        }
                        tracing::info!(keys = count, "Peer declared its vault keys");
                    }
                    (
                        proto::MessageType::SyntheticKeyOffer
                        | proto::MessageType::SyntheticKeyChunk
                        | proto::MessageType::SyntheticKeyComplete,
                        payload,
                    ) => {
                        // Offer, Chunk and Complete always target the local receiver.
                        // Failures are transfer errors answered with an abort; the
                        // authenticated session and Sync stay up.
                        let now = Instant::now();
                        let outbound = match &payload {
                            MessagePayload::SyntheticKeyOffer(offer) => {
                                let busy = synthetic.sender_active();
                                match synthetic.receiver.offer(offer, busy, now) {
                                    Ok(()) => None,
                                    Err(refusal) => {
                                        tracing::warn!(
                                            refusal = ?refusal,
                                            "Inbound key transfer: answering offer with abort code 4"
                                        );
                                        Some(SyntheticTransferOutbound::Abort(
                                            proto::SyntheticKeyAbort {
                                                transfer_id: offer.transfer_id.clone(),
                                                code: 4,
                                            },
                                        ))
                                    }
                                }
                            }
                            MessagePayload::SyntheticKeyChunk(chunk) => {
                                synthetic.receiver.chunk(chunk, now)
                            }
                            MessagePayload::SyntheticKeyComplete(complete) => {
                                synthetic.receiver.complete(complete, now)
                            }
                            _ => None,
                        };
                        if let MessagePayload::SyntheticKeyChunk(mut chunk) = payload {
                            chunk.data.zeroize();
                        }
                        if let Some(outbound) = outbound {
                            if let Err(failure) =
                                send_synthetic_outbound_bounded(session, outbound).await
                            {
                                return SessionExit::Failed(failure);
                            }
                        }
                        synthetic.publish();
                    }
                    (
                        proto::MessageType::SyntheticKeyAbort,
                        MessagePayload::SyntheticKeyAbort(abort),
                    ) if synthetic.receiver.knows(&abort.transfer_id) => {
                        synthetic.receiver.remote_abort(&abort);
                        synthetic.publish();
                    }
                    (
                        proto::MessageType::SyntheticKeyDecision
                        | proto::MessageType::SyntheticKeyReceipt
                        | proto::MessageType::SyntheticKeyAbort,
                        payload,
                    ) => {
                        let progress = synthetic.actor.progress();
                        let supplied_id = match &payload {
                            MessagePayload::SyntheticKeyDecision(message) => {
                                message.transfer_id.as_slice()
                            }
                            MessagePayload::SyntheticKeyReceipt(message) => {
                                message.transfer_id.as_slice()
                            }
                            MessagePayload::SyntheticKeyAbort(message) => {
                                message.transfer_id.as_slice()
                            }
                            _ => &[],
                        };
                        // Crossing cancellations and already-in-flight decisions for the
                        // consumed operation never reopen it or terminate unrelated Sync.
                        if matches!(
                            progress.phase,
                            SyntheticTransferPhase::Complete
                                | SyntheticTransferPhase::Rejected
                                | SyntheticTransferPhase::Cancelled
                                | SyntheticTransferPhase::Expired
                                | SyntheticTransferPhase::Failed
                                | SyntheticTransferPhase::Uncertain
                        ) && progress
                            .transfer_id
                            .as_ref()
                            .is_some_and(|id| supplied_id == id)
                        {
                            continue;
                        }
                        // A frame for no active outbound operation, or for another id,
                        // is a transfer error; it never tears down the session.
                        if !synthetic.sender_active()
                            || progress
                                .transfer_id
                                .as_ref()
                                .is_none_or(|id| supplied_id != id)
                        {
                            tracing::warn!(
                                message_type = message.message_type.as_str_name(),
                                phase = ?progress.phase,
                                "Key transfer: frame for an unknown outbound operation ignored"
                            );
                            continue;
                        }
                        if let Err(error) = synthetic.actor.receive(payload, Instant::now()) {
                            tracing::warn!(
                                error = ?error,
                                phase = ?synthetic.actor.progress().phase,
                                "Key transfer: outbound operation failed on a peer frame"
                            );
                        }
                        synthetic.publish();
                    }
                    (message_type, payload)
                        if is_sync_message(message_type) && message.channel == Channel::Control =>
                    {
                        // The peer may still send its pending round after a local failure.
                        if sync_suspended {
                            continue;
                        }
                        let Some(worker) = sync_worker.as_ref() else {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        };
                        match worker
                            .input
                            .try_send(SyncWorkerInput::Message(message_type, payload))
                        {
                            Ok(()) => {}
                            Err(StdTrySendError::Full(input)) => {
                                pending_sync_input =
                                    Some((input, Instant::now() + Duration::from_secs(2)));
                            }
                            Err(StdTrySendError::Disconnected(_)) => suspend_sync(
                                &mut sync_worker,
                                &mut pending_sync_input,
                                &mut sync_suspended,
                                "worker closed",
                            ),
                        }
                    }
                    (proto::MessageType::SyncChunk, payload)
                        if message.channel == Channel::Data =>
                    {
                        if sync_suspended {
                            continue;
                        }
                        let Some(worker) = sync_worker.as_ref() else {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        };
                        match worker.input.try_send(SyncWorkerInput::Message(
                            proto::MessageType::SyncChunk,
                            payload,
                        )) {
                            Ok(()) => {}
                            Err(StdTrySendError::Full(input)) => {
                                pending_sync_input =
                                    Some((input, Instant::now() + Duration::from_secs(2)));
                            }
                            Err(StdTrySendError::Disconnected(_)) => suspend_sync(
                                &mut sync_worker,
                                &mut pending_sync_input,
                                &mut sync_suspended,
                                "worker closed",
                            ),
                        }
                    }
                    _ => return SessionExit::Failed(SessionFailure::Protocol),
                }
            }
        }
    }
}

enum SelectedSessionEvent {
    SyntheticCommand(Option<SyntheticCommand>),
    SyntheticTick,
    RetrySyncInput,
    ObservePath,
    Command(Option<SessionCommand>),
    DirectoryChanged(Result<(), watch::error::RecvError>),
    Message(Result<kryptopass_protocol::DecodedMessage, PairingError>),
    SyncOutput(Option<SyncWorkerOutput>),
    PingDue,
    PingTimeout,
}

async fn receive_sync_worker_output(worker: &mut Option<SyncWorker>) -> Option<SyncWorkerOutput> {
    match worker {
        Some(worker) => worker.outputs.recv().await,
        None => std::future::pending().await,
    }
}

fn is_sync_message(message_type: proto::MessageType) -> bool {
    matches!(
        message_type,
        proto::MessageType::SyncHello
            | proto::MessageType::SyncManifest
            | proto::MessageType::SyncRequest
            | proto::MessageType::SyncChunk
            | proto::MessageType::SyncCommit
            | proto::MessageType::SyncAck
            | proto::MessageType::SyncConflict
    )
}

async fn process_synthetic_command<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    runtime: &mut SyntheticRuntime,
    command: SyntheticCommand,
) -> Result<(), SessionFailure> {
    let now = Instant::now();
    let outbound = match command {
        SyntheticCommand::Start(_) if runtime.receiver.is_active() => {
            tracing::warn!("Key transfer: start refused, an inbound operation is active");
            None
        }
        SyntheticCommand::Start(prepared) => runtime.actor.start(*prepared, now).ok(),
        SyntheticCommand::InboundDecision(transfer_id, accepted) => {
            runtime.receiver.decide(transfer_id, accepted, now)
        }
        SyntheticCommand::InboundFinished(transfer_id, challenge_sha256) => runtime
            .receiver
            .finish_import(transfer_id, challenge_sha256, now),
    };
    if let Some(outbound) = outbound {
        send_synthetic_outbound_bounded(session, outbound).await?;
    }
    runtime.progress.send_replace(runtime.snapshot());
    Ok(())
}

struct SensitiveSyntheticChunk(proto::SyntheticKeyChunk);

impl Drop for SensitiveSyntheticChunk {
    fn drop(&mut self) {
        self.0.data.zeroize();
    }
}

async fn send_synthetic_outbound_bounded<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    outbound: SyntheticTransferOutbound,
) -> Result<(), SessionFailure> {
    match outbound {
        SyntheticTransferOutbound::Offer(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyOffer, &payload).await
        }
        SyntheticTransferOutbound::Chunk(payload) => {
            let sensitive = SensitiveSyntheticChunk(payload);
            send_control_bounded(session, proto::MessageType::SyntheticKeyChunk, &sensitive.0).await
        }
        SyntheticTransferOutbound::Complete(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyComplete, &payload).await
        }
        SyntheticTransferOutbound::Abort(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyAbort, &payload).await
        }
        SyntheticTransferOutbound::Decision(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyDecision, &payload).await
        }
        SyntheticTransferOutbound::Receipt(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyReceipt, &payload).await
        }
    }
}

async fn send_sync_outbound_bounded<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    outbound: SyncOutbound,
) -> Result<(), SessionFailure> {
    match outbound {
        SyncOutbound::Hello(payload) => {
            send_control_bounded(session, proto::MessageType::SyncHello, &payload).await
        }
        SyncOutbound::Manifest(payload) => {
            send_control_bounded(session, proto::MessageType::SyncManifest, &payload).await
        }
        SyncOutbound::Request(payload) => {
            send_control_bounded(session, proto::MessageType::SyncRequest, &payload).await
        }
        SyncOutbound::Chunk(payload) => {
            send_control_bounded(session, proto::MessageType::SyncChunk, &payload).await
        }
        SyncOutbound::Commit(payload) => {
            send_control_bounded(session, proto::MessageType::SyncCommit, &payload).await
        }
        SyncOutbound::Ack(payload) => {
            send_control_bounded(session, proto::MessageType::SyncAck, &payload).await
        }
    }
}

async fn finish_session<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    context: SessionFinishContext<'_>,
    exit: SessionExit,
) {
    let (event, connection) = match exit {
        SessionExit::Disconnected(reason, send_close) => {
            if send_close {
                let close_sent = tokio::time::timeout(
                    Duration::from_secs(2),
                    session.send_message(
                        proto::MessageType::SessionClose,
                        &proto::SessionClose {
                            reason: reason as u32,
                        },
                    ),
                )
                .await
                .is_ok_and(|result| result.is_ok());
                if close_sent {
                    wait_for_peer_close(session).await;
                }
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
                SessionFailure::Transport
                    | SessionFailure::PingTimeout
                    | SessionFailure::SyncStorage
                    | SessionFailure::SyncTimeout
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

async fn wait_for_peer_close<T: PeerTransport>(session: &mut EstablishedSession<T>) {
    let deadline = Instant::now() + Duration::from_secs(2);
    for _ in 0..64 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, session.receive_message()).await {
            Ok(Ok(message)) if message.message_type == proto::MessageType::SessionClose => break,
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => break,
        }
    }
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

#[derive(Default)]
struct DataRateLimiter {
    recent: VecDeque<Instant>,
}

impl DataRateLimiter {
    fn accept(&mut self, now: Instant) -> bool {
        self.accept_limit(now, MAX_DATA_MESSAGES_PER_SECOND)
    }

    fn accept_limit(&mut self, now: Instant, maximum: usize) -> bool {
        while self
            .recent
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(1))
        {
            let _ = self.recent.pop_front();
        }
        if self.recent.len() >= maximum {
            return false;
        }
        self.recent.push_back(now);
        true
    }
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
