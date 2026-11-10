//! The trusted-device directory: who is trusted and how each one is connected.

use super::*;

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
    /// An explicitly supplied Sync binding names no keys, or too many.
    #[error("sync session binding keys are invalid")]
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

    pub(crate) fn contains_exact_peer(&self, peer: &PublicIdentity) -> bool {
        self.snapshot()
            .get(&peer.device_id)
            .is_some_and(|device| device.trust == TrustState::Trusted && device.identity == *peer)
    }

    pub(crate) fn begin_session(&self, peer: &PublicIdentity) -> Result<u64, DeviceDirectoryError> {
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

    pub(crate) fn update_connection_state_for_generation(
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

    pub(crate) fn update_connection_info_for_generation(
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
    pub(crate) fn set_peer_vault_keys(
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

    pub(crate) fn session_is_current(&self, peer: &PublicIdentity, generation: u64) -> bool {
        self.snapshot().get(&peer.device_id).is_some_and(|device| {
            device.trust == TrustState::Trusted
                && device.identity == *peer
                && device.connection_generation == generation
        })
    }

    pub(crate) fn session_termination_reason(
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
