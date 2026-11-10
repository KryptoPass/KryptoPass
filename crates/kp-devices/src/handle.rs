//! What the application holds of a live session: its handle, events and failures.

use super::*;

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
    pub(crate) fn from_wire(reason: u32) -> Option<Self> {
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

pub(crate) enum SyntheticCommand {
    Start(Box<SyntheticKeyTransferSend>),
    /// Local decision on the inbound offer with this id.
    InboundDecision([u8; 16], bool),
    /// Application result of import + challenge decryption; `None` is a failure.
    InboundFinished([u8; 16], Option<[u8; 32]>),
}

pub(crate) enum SessionCommand {
    Disconnect(SessionCloseReason),
}

/// Handle retaining an active session task and its bounded observability APIs.
pub struct SessionHandle {
    pub(crate) peer_id: DeviceId,
    pub(crate) directory: DeviceDirectory,
    pub(crate) commands: mpsc::Sender<SessionCommand>,
    pub(crate) events: broadcast::Sender<SessionEvent>,
    pub(crate) lifecycle: watch::Receiver<Option<SessionEvent>>,
    pub(crate) sync_progress: watch::Receiver<Option<SyncProgress>>,
    pub(crate) synthetic_commands: mpsc::Sender<SyntheticCommand>,
    pub(crate) synthetic_cancel: watch::Sender<Option<[u8; 16]>>,
    pub(crate) synthetic_progress: watch::Receiver<SyntheticTransferProgress>,
    pub(crate) inbound_material: InboundMaterialSlot,
    pub(crate) closed: watch::Receiver<bool>,
    /// Wakes the live session to ask its Sync for one more round; see
    /// [`SessionHandle::request_sync_round`].
    pub(crate) sync_round: Arc<tokio::sync::Notify>,
    pub(crate) generation: u64,
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

    /// Asks this live session's Sync for one more round now. Resyncing is not reconnecting:
    /// the session and its transport stay as they are. A round already running finishes
    /// first and the next one starts after it; repeated requests coalesce.
    pub fn request_sync_round(&self) {
        self.sync_round.notify_one();
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
