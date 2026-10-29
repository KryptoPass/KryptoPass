//! The pairing and connection host of this device: its pairing profile
//! (identity, trust, device names), pairing with QR and SAS, reconnection
//! without repeating them, Internet routes and network privacy. No vault or
//! OpenPGP operations; Sync is bound by the caller.

mod internet_routes;
mod network_privacy;
mod profile_trust;
mod resume_owner;
pub use kryptopass_store::aliases::AliasError;
pub use network_privacy::{
    NetworkPrivacySettings, load_network_privacy_settings, save_network_privacy_settings,
};
pub use profile_trust::ProfileTrust;
pub use resume_owner::{
    ResumeOwnerReady, ResumeOwnerSnapshot, ResumeProcessOwner, SyncBinder,
    spawn_resume_process_owner,
};

use std::{
    fs,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use kryptopass_identity::{
    DeviceId, DeviceIdentity, PublicIdentity, SecureKeyStore, SystemSecretKeyStore, TrustStore,
    TrustStoreError,
};
use kryptopass_lan::{
    InternetIceConfig, InternetIceTransportPolicy, LanHost, NetworkPolicyError, NetworkPolicyGuard,
    NetworkUse, RelayHost, RelayRoute, WebRtcTransport,
};
use kryptopass_pairing::{
    LocalApproval, LocalDecision, PairingConfig, PairingEngine, PairingError, PairingInvitation,
    PairingOutcome, PairingRole, SasDisplay,
};
use kryptopass_protocol::Channel;
use kryptopass_store::trust::SqliteTrustStore;
use kryptopass_transport::{LoopbackTransport, PeerTransport, TransportError, TransportFrame};
use thiserror::Error;
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot, watch},
    time::timeout,
};

const RECONNECT_PAIRING_TIMEOUT: Duration = Duration::from_secs(60);
/// Persisted name of the device identity in the OS keyring; renaming it would
/// lose the identity, so it keeps its original value.
const SYSTEM_PROFILE_KEYRING_ID: &str = "emulator-pairing-lab-v1";
const PROFILE_RESET_MARKER: &str = ".pairing-profile-reset-v1";
const PROFILE_RESET_MARKER_CONTENT: &[u8] = b"KryptoPass pairing profile reset v1\n";

/// UI events. No Debug implementation: bootstrap includes a one-time bearer credential.
pub enum HostEvent {
    /// Bootstrap intended solely for an explicit UI/privately controlled test pipe.
    Bootstrap(String),
    /// An authenticated context is ready for independent local comparison.
    Sas(Box<SasDisplay>),
}

/// Explicit choice of carrier; neither discovery nor signaling grants trust.
#[derive(Clone)]
pub enum PeerNetwork {
    /// Original emulator-only adapter, bound to IPv4 loopback.
    EmulatorLoopback,
    /// Reliable ordered WebRTC DataChannel, with local WebSocket signaling.
    Lan { bind_ip: IpAddr },
    /// WebRTC through a public untrusted rendezvous relay; identity stays in Noise.
    Internet { route: RelayRoute },
    /// Both at once: one invitation carries the LAN address and the relay room,
    /// and the first carrier the phone reaches wins. Neither grants trust.
    Auto { bind_ip: IpAddr, route: RelayRoute },
}

impl PeerNetwork {
    /// Creates a fresh rendezvous room; its random address never grants trust.
    pub fn internet(relay_origin: &str) -> Result<Self, PeerError> {
        let mut room = [0_u8; 32];
        getrandom::fill(&mut room).map_err(|_| PeerError::Link)?;
        let route = RelayRoute::new(relay_origin, room).map_err(|_| PeerError::Link)?;
        Ok(Self::Internet { route })
    }

    /// LAN plus a fresh rendezvous room, so pairing works off the local network too.
    pub fn auto(bind_ip: IpAddr, relay_origin: &str) -> Result<Self, PeerError> {
        let Self::Internet { route } = Self::internet(relay_origin)? else {
            return Err(PeerError::Link);
        };
        Ok(Self::Auto { bind_ip, route })
    }

    /// The relay room, when this carrier uses one.
    fn route(&self) -> Option<&RelayRoute> {
        match self {
            Self::Internet { route } | Self::Auto { route, .. } => Some(route),
            Self::EmulatorLoopback | Self::Lan { .. } => None,
        }
    }
}

/// Local addresses available for the explicitly enabled LAN listener.
pub fn lan_addresses() -> Result<Vec<IpAddr>, PeerError> {
    kryptopass_lan::local_addresses().map_err(|_| PeerError::Link)
}

enum LocalListener {
    Emulator(TcpListener),
    Lan(LanHost),
    Internet(Box<RelayHost>),
    Auto(LanHost, Box<RelayHost>),
}

/// Carrier retained after pairing, independent of the identity and session owner.
pub enum HostTransport {
    /// Bounded emulator-only TCP adapter.
    Emulator(LoopbackTransport),
    /// Local WebRTC DataChannel adapter.
    Lan(WebRtcTransport),
}

#[async_trait]
impl PeerTransport for HostTransport {
    async fn connection_info(&mut self) -> kryptopass_transport::ConnectionInfo {
        match self {
            Self::Emulator(transport) => transport.connection_info().await,
            Self::Lan(transport) => transport.connection_info().await,
        }
    }

    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        match self {
            Self::Emulator(transport) => transport.send(channel, data).await,
            Self::Lan(transport) => transport.send(channel, data).await,
        }
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        match self {
            Self::Emulator(transport) => transport.receive().await,
            Self::Lan(transport) => transport.receive().await,
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        match self {
            Self::Emulator(transport) => transport.close().await,
            Self::Lan(transport) => transport.close().await,
        }
    }
}

/// Redacted failures suitable for a development UI.
#[derive(Debug, Error)]
pub enum PeerError {
    #[error("the isolated pairing profile could not be opened")]
    Profile,
    #[error("the operating-system key store is unavailable or denied access")]
    KeyStore,
    #[error("the local pairing link failed")]
    Link,
    #[error("the pairing operation was cancelled")]
    Cancelled,
    #[error("the pairing invitation or operation expired")]
    Expired,
    #[error("the peer or local user rejected pairing")]
    Rejected,
    #[error("the peer authentication or trust operation failed")]
    Authentication,
    #[error("trust was persisted but the active session could not be started")]
    Session,
    #[error("network privacy policy blocked the operation: {0}")]
    NetworkPrivacyBlocked(#[from] NetworkPolicyError),
}

/// A host profile whose secret identity is owned here, never projected into QML.
pub struct PeerHost {
    identity: DeviceIdentity,
    directory: PathBuf,
}

impl PeerHost {
    async fn internet_ice_configuration(
        &self,
        guard: &NetworkPolicyGuard,
    ) -> Result<(InternetIceConfig, InternetIceTransportPolicy), kryptopass_lan::LanError> {
        let local_device_id = *self.identity.device_id().as_bytes();
        InternetIceConfig::from_environment_with_policy(
            local_device_id,
            guard.relay_only(),
            guard,
            |message| self.identity.sign(message),
        )
        .await
    }

    /// Return the persisted local network settings without exposing the
    /// profile identity or changing any network state.
    pub fn network_privacy_settings(&self) -> Result<NetworkPrivacySettings, PeerError> {
        load_network_privacy_settings(&self.directory)
    }

    /// Construct a route guard pinned to this profile's persisted policy.
    pub fn network_policy_guard(&self) -> Result<NetworkPolicyGuard, PeerError> {
        let settings = self.network_privacy_settings()?;
        Ok(NetworkPolicyGuard::new(settings.policy))
    }

    /// Load public relay routes only for peers still authorized in this profile.
    pub fn internet_routes(
        &self,
    ) -> Result<std::collections::BTreeMap<DeviceId, RelayRoute>, PeerError> {
        internet_routes::read(
            &self.directory,
            self.identity.device_id(),
            &self.trusted_peers()?,
        )
    }

    /// Save routing only after a successful identity-authenticated operation.
    pub fn set_internet_route(&self, peer: DeviceId, route: &RelayRoute) -> Result<(), PeerError> {
        self.lookup(peer)?;
        internet_routes::write(
            &self.directory,
            self.identity.device_id(),
            peer,
            Some(route),
        )
    }

    /// Returns local display aliases for currently trusted peers only.
    pub fn device_aliases(
        &self,
    ) -> Result<std::collections::BTreeMap<kryptopass_identity::DeviceId, String>, AliasError> {
        let peers = self.trusted_peers().map_err(|_| AliasError::Storage)?;
        kryptopass_store::aliases::read(&self.directory, self.identity.device_id(), &peers)
    }

    /// Persists a local label without changing identity or authorization.
    /// An empty label removes the alias. Unknown or revoked peers are rejected.
    pub fn set_device_alias(
        &self,
        peer: kryptopass_identity::DeviceId,
        alias: &str,
    ) -> Result<(), AliasError> {
        self.lookup(peer).map_err(|_| AliasError::UnknownDevice)?;
        kryptopass_store::aliases::write(&self.directory, self.identity.device_id(), peer, alias)
    }

    /// Starts the unique authenticated owner of a session, with Sync when the
    /// caller binds a store, a trust source and the keys this peer syncs.
    pub fn start_device_session(
        &self,
        devices: kryptopass_devices::DeviceDirectory,
        session: kryptopass_pairing::EstablishedSession<HostTransport>,
        peer: PublicIdentity,
        options: kryptopass_devices::SessionOptions,
        sync: Option<kryptopass_devices::SyncSessionBinding>,
    ) -> Result<kryptopass_devices::SessionHandle, PeerError> {
        kryptopass_devices::start_session_with_extensions(
            devices,
            session,
            self.identity.device_id(),
            peer,
            options,
            sync,
            cfg!(debug_assertions),
        )
        .map_err(|_| PeerError::Session)
    }

    /// Trust for Sync sessions: this device, the authorized peers, and the
    /// forgotten devices for verifying the history they signed.
    pub fn sync_trust(&self) -> ProfileTrust {
        ProfileTrust {
            local: self.identity.public_identity(),
            directory: self.directory.clone(),
        }
    }

    /// Forgets an authorized peer and its local alias so it can be paired again.
    ///
    /// Call only after closing the peer's session and serializing all profile
    /// operations. Alias removal happens first; if trust removal then fails,
    /// the peer remains trusted without its local label and this method returns
    /// an error. Revoked tombstones are never removed by this operation.
    pub fn forget_peer(&self, peer: DeviceId) -> Result<(), PeerError> {
        let mut store = SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| PeerError::Profile)?;
        let identity = store
            .lookup_trusted_peer(&peer)
            .map_err(map_forget_trust_error)?;
        profile_trust::keep_forgotten_author(&self.directory, &identity)?;
        internet_routes::write(&self.directory, self.identity.device_id(), peer, None)?;
        kryptopass_store::aliases::forget(&self.directory, self.identity.device_id(), peer)
            .map_err(|_| PeerError::Profile)?;
        store
            .forget_authorized_peer(&peer)
            .map_err(map_forget_trust_error)
    }

    /// Resets only this pairing profile's OS keyring identity, trust DB,
    /// and local device-label DB. It never touches the vault, OpenPGP keys, or
    /// GnuPG directory. Call only after dropping the `PeerHost`, stopping all
    /// sessions, and serializing profile operations.
    ///
    /// The keyring credential is removed first so an incomplete filesystem
    /// cleanup cannot authorize new sessions. The fixed DB paths are checked
    /// before deletion. If a later step fails, this returns an error and does
    /// not recreate any identity or database; a subsequent reset can retry.
    pub fn reset_system_profile(directory: &Path) -> Result<(), PeerError> {
        let keys = SystemSecretKeyStore::new(directory, SYSTEM_PROFILE_KEYRING_ID)
            .map_err(|_| PeerError::KeyStore)?;
        SqliteTrustStore::validate_reset_targets(directory).map_err(|_| PeerError::Profile)?;
        kryptopass_store::aliases::validate_reset_targets(directory)
            .map_err(|_| PeerError::Profile)?;
        internet_routes::validate_target(directory)?;
        validate_reset_profile_artifacts(directory, false, false)?;

        internet_routes::reset(directory)?;
        keys.delete().map_err(|_| PeerError::KeyStore)?;
        SqliteTrustStore::reset_profile_files(directory).map_err(|_| PeerError::Profile)?;
        kryptopass_store::aliases::reset_profile_files(directory)
            .map_err(|_| PeerError::Profile)?;
        write_reset_marker(directory)?;
        Ok(())
    }

    /// Opens an OS-backed profile for an explicit pairing operation.
    ///
    /// If the directory does not exist, this initializes a new identity and
    /// trust database. Existing directories are loaded through
    /// [`Self::open_existing_system_profile`]. A validated explicit reset marker
    /// permits fresh trust with an existing identity, as used by `just dev-reset`.
    /// Without that marker, missing or corrupt material is never repaired.
    pub fn open_system_profile(directory: &Path) -> Result<Self, PeerError> {
        #[cfg(unix)]
        let created = {
            use std::os::unix::fs::DirBuilderExt;
            match fs::DirBuilder::new().mode(0o700).create(directory) {
                Ok(()) => true,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
                Err(_) => return Err(PeerError::Profile),
            }
        };
        #[cfg(not(unix))]
        let created = match fs::create_dir(directory) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(_) => return Err(PeerError::Profile),
        };
        if !created {
            return if reset_marker_exists(directory)? {
                Self::initialize_reset_profile(directory)
            } else {
                Self::open_existing_system_profile(directory)
            };
        }
        Self::initialize_new_profile(directory)
    }

    fn initialize_new_profile(directory: &Path) -> Result<Self, PeerError> {
        let keys = SystemSecretKeyStore::new(directory, SYSTEM_PROFILE_KEYRING_ID)
            .map_err(|_| PeerError::KeyStore)?;
        if keys.load().map_err(|_| PeerError::KeyStore)?.is_some() {
            return Err(PeerError::Profile);
        }
        let identity = DeviceIdentity::create_new(&keys).map_err(|_| PeerError::KeyStore)?;
        SqliteTrustStore::open_new(directory, identity.device_id())
            .map_err(|_| PeerError::Profile)?;
        Ok(Self {
            identity,
            directory: directory.to_path_buf(),
        })
    }

    fn initialize_reset_profile(directory: &Path) -> Result<Self, PeerError> {
        let keys = SystemSecretKeyStore::new(directory, SYSTEM_PROFILE_KEYRING_ID)
            .map_err(|_| PeerError::KeyStore)?;
        validate_reset_profile_artifacts(directory, false, true)?;
        let existing = keys.load().map_err(|_| PeerError::KeyStore)?;
        if existing.is_some() {
            drop(existing);
            let identity = DeviceIdentity::load_existing(&keys).map_err(|_| PeerError::KeyStore)?;
            let database_exists = directory
                .join("device-trust.sqlite3")
                .try_exists()
                .map_err(|_| PeerError::Profile)?;
            let store = if database_exists {
                SqliteTrustStore::open_existing(directory, identity.device_id())
            } else {
                // dev-reset archives trust but preserves the system keyring.
                // Only its explicit reset marker permits a fresh trust DB for that identity.
                validate_reset_ready_directory(directory)?;
                SqliteTrustStore::open_new(directory, identity.device_id())
            }
            .map_err(|_| PeerError::Profile)?;
            let peers = store.list_trusted_peers().map_err(|_| PeerError::Profile)?;
            drop(store);
            kryptopass_store::aliases::read(directory, identity.device_id(), &peers)
                .map_err(|_| PeerError::Profile)?;
            remove_reset_marker(directory)?;
            return Ok(Self {
                identity,
                directory: directory.to_path_buf(),
            });
        }

        validate_reset_ready_directory(directory)?;
        let identity = DeviceIdentity::create_new(&keys).map_err(|_| PeerError::KeyStore)?;
        SqliteTrustStore::open_new(directory, identity.device_id())
            .map_err(|_| PeerError::Profile)?;
        remove_reset_marker(directory)?;
        Ok(Self {
            identity,
            directory: directory.to_path_buf(),
        })
    }

    /// Reports whether an explicit first pairing is needed without reading keys
    /// or changing the reset marker. Invalid files and directories remain errors.
    pub fn system_profile_needs_pairing(directory: &Path) -> Result<bool, PeerError> {
        match fs::symlink_metadata(directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(_) => return Err(PeerError::Profile),
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                return Err(PeerError::Profile);
            }
            Ok(_) => {}
        }
        // Construction validates the private directory without contacting the keyring.
        SystemSecretKeyStore::new(directory, SYSTEM_PROFILE_KEYRING_ID)
            .map_err(|_| PeerError::Profile)?;
        if reset_marker_exists(directory)? {
            validate_reset_profile_artifacts(directory, false, true)?;
            if directory
                .join("device-trust.sqlite3")
                .try_exists()
                .map_err(|_| PeerError::Profile)?
            {
                return Ok(false);
            }
            validate_reset_ready_directory(directory)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Opens an existing system profile without creating or repairing anything.
    ///
    /// The directory, keyring identity, and bound trust database must all
    /// already exist and validate. This is the entry point for loading peers
    /// and offering reconnect after an application restart.
    pub fn open_existing_system_profile(directory: &Path) -> Result<Self, PeerError> {
        let metadata = fs::symlink_metadata(directory).map_err(|_| PeerError::Profile)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(PeerError::Profile);
        }
        let keys = SystemSecretKeyStore::new(directory, SYSTEM_PROFILE_KEYRING_ID)
            .map_err(|_| PeerError::KeyStore)?;
        let identity = DeviceIdentity::load_existing(&keys).map_err(|_| PeerError::KeyStore)?;
        let store = SqliteTrustStore::open_existing(directory, identity.device_id())
            .map_err(|_| PeerError::Profile)?;
        drop(store);
        Ok(Self {
            identity,
            directory: directory.to_path_buf(),
        })
    }

    /// Test-process-only host. Its synthetic private identity is held in RAM, never written.
    /// The caller owns a fresh private directory and must remove it after the test.
    pub fn ephemeral_test_profile(directory: &Path) -> Result<Self, PeerError> {
        let identity = DeviceIdentity::generate().map_err(|_| PeerError::Authentication)?;
        SqliteTrustStore::open_new(directory, identity.device_id())
            .map_err(|_| PeerError::Profile)?;
        Ok(Self {
            identity,
            directory: directory.to_path_buf(),
        })
    }

    /// Public identity for display and reconnect correlation only.
    pub fn public_identity(&self) -> PublicIdentity {
        self.identity.public_identity()
    }

    /// Revalidates an already trusted peer for a subsequent fresh Noise handshake.
    pub fn lookup(&self, id: kryptopass_identity::DeviceId) -> Result<PublicIdentity, PeerError> {
        SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| PeerError::Profile)?
            .lookup_trusted_peer(&id)
            .map_err(|_| PeerError::Authentication)
    }

    /// Lists currently authorized peers from the existing profile database.
    /// No missing directory, identity, or trust database is created here.
    pub fn trusted_peers(&self) -> Result<Vec<PublicIdentity>, PeerError> {
        SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| PeerError::Profile)?
            .list_trusted_peers()
            .map_err(|_| PeerError::Profile)
    }

    /// Hosts exactly one bounded emulator pairing/reconnect, bound to loopback.
    pub async fn run_once(
        &self,
        expected_peer: Option<PublicIdentity>,
        events: mpsc::Sender<HostEvent>,
        decisions: oneshot::Receiver<LocalDecision>,
        cancelled: watch::Receiver<bool>,
    ) -> Result<PublicIdentity, PeerError> {
        self.run_once_with_network(
            expected_peer,
            events,
            decisions,
            cancelled,
            PeerNetwork::EmulatorLoopback,
        )
        .await
    }

    /// Hosts one local pairing/reconnect using the explicit carrier selection.
    pub async fn run_once_with_network(
        &self,
        expected_peer: Option<PublicIdentity>,
        events: mpsc::Sender<HostEvent>,
        decisions: oneshot::Receiver<LocalDecision>,
        cancelled: watch::Receiver<bool>,
        network: PeerNetwork,
    ) -> Result<PublicIdentity, PeerError> {
        let mut result = self
            .open_session_with_network(expected_peer, events, decisions, cancelled, network)
            .await?;
        // This compatibility helper deliberately ends after the persisted pairing.
        let _ = result.session.close().await;
        Ok(result.peer)
    }

    /// Authenticates and persists trust, returning the original live Noise session.
    ///
    /// The caller owns its subsequent lifetime. Pairing deadlines end here;
    /// keeping the carrier alive is the active-session owner's responsibility.
    pub async fn open_session_with_network(
        &self,
        expected_peer: Option<PublicIdentity>,
        events: mpsc::Sender<HostEvent>,
        decisions: oneshot::Receiver<LocalDecision>,
        cancelled: watch::Receiver<bool>,
        network: PeerNetwork,
    ) -> Result<PairingOutcome<HostTransport>, PeerError> {
        let guard = self.network_policy_guard()?;
        self.open_session_with_network_guard(
            expected_peer,
            events,
            decisions,
            cancelled,
            network,
            guard,
        )
        .await
    }

    /// Same as [`open_session_with_network`](Self::open_session_with_network),
    /// with a caller-owned guard so policy changes can invalidate pending and
    /// active carrier work through every clone of that guard.
    pub async fn open_session_with_network_guard(
        &self,
        expected_peer: Option<PublicIdentity>,
        events: mpsc::Sender<HostEvent>,
        decisions: oneshot::Receiver<LocalDecision>,
        mut cancelled: watch::Receiver<bool>,
        network: PeerNetwork,
        guard: NetworkPolicyGuard,
    ) -> Result<PairingOutcome<HostTransport>, PeerError> {
        if *cancelled.borrow() {
            return Err(PeerError::Cancelled);
        }
        let network_use = match &network {
            PeerNetwork::EmulatorLoopback => None,
            PeerNetwork::Lan { .. } => Some(NetworkUse::Lan),
            // LAN is an optional extra in auto mode; Internet must be allowed.
            PeerNetwork::Internet { .. } | PeerNetwork::Auto { .. } => Some(NetworkUse::Internet),
        };
        if let Some(network_use) = network_use {
            guard.preflight(network_use)?;
        }
        let operation = self.run_inner(expected_peer, events, decisions, network, guard.clone());
        tokio::select! {
            result = timeout(Duration::from_secs(300), operation) => result.map_err(|_| PeerError::Expired)?,
            _ = cancelled.changed() => Err(PeerError::Cancelled),
            error = guard.wait_for_change(Duration::from_millis(250)), if network_use.is_some() => {
                Err(PeerError::NetworkPrivacyBlocked(error))
            }
        }
    }

    /// Authenticates a resume carrier using a fresh Noise XX reconnect.
    ///
    /// The session ID is supplied by the LAN resume endpoint and is used only
    /// for this handshake. The exact trusted peer identity is reloaded from the
    /// existing profile before Noise starts. Reconnect cannot create or repair
    /// trust, and an unexpected request for SAS approval is rejected.
    pub async fn open_reconnect_session(
        &self,
        session_id: [u8; 16],
        transport: WebRtcTransport,
        expected_peer: PublicIdentity,
        mut cancelled: watch::Receiver<bool>,
    ) -> Result<PairingOutcome<HostTransport>, PeerError> {
        if *cancelled.borrow() {
            return Err(PeerError::Cancelled);
        }
        expected_peer
            .validate()
            .map_err(|_| PeerError::Authentication)?;
        if expected_peer.device_id == self.identity.device_id() {
            return Err(PeerError::Authentication);
        }
        let store = SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| PeerError::Profile)?;
        if store
            .lookup_trusted_peer(&expected_peer.device_id)
            .map_err(|_| PeerError::Authentication)?
            != expected_peer
        {
            return Err(PeerError::Authentication);
        }

        let mut config =
            PairingConfig::reconnect(session_id, PairingRole::Responder, now_ms()?, expected_peer);
        config.timeout = RECONNECT_PAIRING_TIMEOUT;
        let mut approval = RejectUnexpectedApproval;
        let operation = async {
            PairingEngine::new(
                HostTransport::Lan(transport),
                &self.identity,
                Box::new(store),
                config,
            )
            .map_err(classify_reconnect)?
            .run(None, &mut approval)
            .await
            .map_err(classify_reconnect)
        };
        tokio::select! {
            result = timeout(RECONNECT_PAIRING_TIMEOUT, operation) => {
                result.unwrap_or(Err(PeerError::Expired))
            }
            _ = cancelled.changed() => Err(PeerError::Cancelled),
        }
    }

    async fn run_inner(
        &self,
        expected_peer: Option<PublicIdentity>,
        events: mpsc::Sender<HostEvent>,
        decisions: oneshot::Receiver<LocalDecision>,
        network: PeerNetwork,
        guard: NetworkPolicyGuard,
    ) -> Result<PairingOutcome<HostTransport>, PeerError> {
        let mut store = SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| PeerError::Profile)?;
        if let Some(peer) = expected_peer {
            if store
                .lookup_trusted_peer(&peer.device_id)
                .map_err(|_| PeerError::Authentication)?
                != peer
            {
                return Err(PeerError::Authentication);
            }
        }
        let mut session_id = [0; 16];
        getrandom::fill(&mut session_id).map_err(|_| PeerError::Authentication)?;
        if session_id.iter().all(|byte| *byte == 0) {
            return Err(PeerError::Authentication);
        }
        let listener = match &network {
            PeerNetwork::EmulatorLoopback => LocalListener::Emulator(
                TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                    .await
                    .map_err(|_| PeerError::Link)?,
            ),
            PeerNetwork::Lan { bind_ip } => {
                guard.preflight(NetworkUse::Lan)?;
                LocalListener::Lan(
                    LanHost::bind(*bind_ip, session_id)
                        .await
                        .map_err(|_| PeerError::Link)?,
                )
            }
            PeerNetwork::Internet { route } => {
                LocalListener::Internet(Box::new(self.relay_host(route, &guard).await?))
            }
            PeerNetwork::Auto { bind_ip, route } => {
                let relay = Box::new(self.relay_host(route, &guard).await?);
                // A privacy policy that forbids LAN leaves the relay alone.
                let lan = match guard.preflight(NetworkUse::Lan) {
                    Ok(()) => LanHost::bind(*bind_ip, session_id).await.ok(),
                    Err(_) => None,
                };
                match lan {
                    Some(lan) => LocalListener::Auto(lan, relay),
                    None => {
                        tracing::warn!(
                            "Auto pairing: LAN unavailable, invitation uses the relay only"
                        );
                        LocalListener::Internet(relay)
                    }
                }
            }
        };
        let port = match &listener {
            LocalListener::Emulator(listener) => {
                listener.local_addr().map_err(|_| PeerError::Link)?.port()
            }
            LocalListener::Lan(listener) | LocalListener::Auto(listener, _) => listener.port(),
            LocalListener::Internet(_) => 0,
        };
        let invitation = if expected_peer.is_none() {
            Some(
                PairingInvitation::create(&mut store, now_ms()?, Duration::from_secs(300))
                    .map_err(|_| PeerError::Profile)?,
            )
        } else {
            None
        };
        let session_hex = hex(&session_id);
        let data = if let Some(invite) = invitation.as_ref() {
            let qr = invite
                .to_qr_bytes()
                .map_err(|_| PeerError::Authentication)?;
            URL_SAFE_NO_PAD.encode(qr.as_bytes())
        } else {
            self.identity.device_id().to_string()
        };
        let kind = if invitation.is_some() {
            "pair"
        } else {
            "reconnect"
        };
        let bootstrap = match (&network, &listener) {
            (PeerNetwork::Auto { bind_ip, route }, LocalListener::Auto(..)) => {
                let value = serde_json::json!({
                    "v": 1, "host": bind_ip.to_string(), "port": port,
                    "relay": route.relay_origin(), "room": route.room_hex(),
                    "session": session_hex, "kind": kind, "data": data,
                });
                let bytes = serde_json::to_vec(&value).map_err(|_| PeerError::Link)?;
                format!("kpauto1:{}", URL_SAFE_NO_PAD.encode(bytes))
            }
            (PeerNetwork::Auto { route, .. }, _) => {
                let value = serde_json::json!({
                    "v": 1, "relay": route.relay_origin(), "room": route.room_hex(),
                    "session": session_hex, "kind": kind, "data": data,
                });
                let bytes = serde_json::to_vec(&value).map_err(|_| PeerError::Link)?;
                format!("kpnet1:{}", URL_SAFE_NO_PAD.encode(bytes))
            }
            _ => match &network {
                PeerNetwork::EmulatorLoopback => format!(
                    "{}:{port}:{session_hex}:{data}",
                    if invitation.is_some() {
                        "kpdev1"
                    } else {
                        "kpdevr1"
                    }
                ),
                PeerNetwork::Lan { bind_ip } => {
                    let value = serde_json::json!({
                        "v": 1, "host": bind_ip.to_string(), "port": port,
                        "session": session_hex, "kind": if invitation.is_some() { "pair" } else { "reconnect" }, "data": data,
                    });
                    let bytes = serde_json::to_vec(&value).map_err(|_| PeerError::Link)?;
                    format!("kplan1:{}", URL_SAFE_NO_PAD.encode(bytes))
                }
                PeerNetwork::Internet { route } => {
                    let value = serde_json::json!({
                        "v": 1, "relay": route.relay_origin(), "room": route.room_hex(),
                        "session": session_hex, "kind": if invitation.is_some() { "pair" } else { "reconnect" }, "data": data,
                    });
                    let bytes = serde_json::to_vec(&value).map_err(|_| PeerError::Link)?;
                    format!("kpnet1:{}", URL_SAFE_NO_PAD.encode(bytes))
                }
                PeerNetwork::Auto { .. } => return Err(PeerError::Link),
            },
        };
        events
            .send(HostEvent::Bootstrap(bootstrap))
            .await
            .map_err(|_| PeerError::Cancelled)?;
        let mut approval = UiApproval {
            events,
            decisions: Some(decisions),
        };
        let mut config = if let Some(peer) = expected_peer {
            PairingConfig::reconnect(session_id, PairingRole::Responder, now_ms()?, peer)
        } else {
            PairingConfig::pairing(session_id, PairingRole::Responder, now_ms()?)
        };
        config.timeout = Duration::from_secs(180);
        let outcome = match listener {
            LocalListener::Emulator(listener) => {
                let (stream, address) = listener.accept().await.map_err(|_| PeerError::Link)?;
                drop(listener);
                if !address.ip().is_loopback() {
                    return Err(PeerError::Link);
                }
                let transport = LoopbackTransport::new(stream).map_err(|_| PeerError::Link)?;
                self.run_transport(
                    HostTransport::Emulator(transport),
                    store,
                    config,
                    invitation.as_ref(),
                    &mut approval,
                )
                .await
            }
            LocalListener::Lan(listener) => {
                let mut transport = tokio::select! {
                    result = listener.accept() => result.map_err(|_| PeerError::Link)?,
                    error = guard.wait_for_change(Duration::from_millis(250)) => {
                        return Err(PeerError::NetworkPrivacyBlocked(error));
                    }
                };
                transport
                    .enforce_network_policy(guard.clone())
                    .map_err(PeerError::NetworkPrivacyBlocked)?;
                self.run_transport(
                    HostTransport::Lan(transport),
                    store,
                    config,
                    invitation.as_ref(),
                    &mut approval,
                )
                .await
            }
            LocalListener::Auto(lan, relay) => {
                let expected_id = expected_peer.map(|peer| *peer.device_id.as_bytes());
                // The first carrier that reaches us wins; the other is dropped unused.
                tracing::info!(port, "Auto pairing: waiting on LAN and relay");
                let transport = tokio::select! {
                    result = lan.accept() => {
                        tracing::info!(ok = result.is_ok(), "Auto pairing: LAN carrier answered first");
                        let mut transport = result.map_err(|_| PeerError::Link)?;
                        transport
                            .enforce_network_policy(guard.clone())
                            .map_err(PeerError::NetworkPrivacyBlocked)?;
                        transport
                    }
                    result = relay.accept(session_id, move |id| {
                        expected_id.is_none_or(|expected| id == expected)
                    }) => {
                        tracing::info!(ok = result.is_ok(), "Auto pairing: relay carrier answered first");
                        result.map_err(|error| map_lan_network_error(error, &guard))?.transport
                    }
                    error = guard.wait_for_change(Duration::from_millis(250)) => {
                        return Err(PeerError::NetworkPrivacyBlocked(error));
                    }
                };
                self.run_transport(
                    HostTransport::Lan(transport),
                    store,
                    config,
                    invitation.as_ref(),
                    &mut approval,
                )
                .await
            }
            LocalListener::Internet(listener) => {
                let expected_id = expected_peer.map(|peer| *peer.device_id.as_bytes());
                let attempt = listener
                    .accept(session_id, move |id| {
                        expected_id.is_none_or(|expected| id == expected)
                    })
                    .await
                    .map_err(|error| map_lan_network_error(error, &guard))?;
                let outcome = self
                    .run_transport(
                        HostTransport::Lan(attempt.transport),
                        store,
                        config,
                        invitation.as_ref(),
                        &mut approval,
                    )
                    .await?;
                Ok(outcome)
            }
        }?;
        // Auto mode keeps the room too, so later recovery can use either carrier.
        if let Some(route) = network.route() {
            self.set_internet_route(outcome.peer.device_id, route)?;
        }
        Ok(outcome)
    }

    async fn relay_host(
        &self,
        route: &RelayRoute,
        guard: &NetworkPolicyGuard,
    ) -> Result<RelayHost, PeerError> {
        let (config, policy) = self
            .internet_ice_configuration(guard)
            .await
            .map_err(|error| map_lan_network_error(error, guard))?;
        RelayHost::connect_with_privacy(route.clone(), config, policy, guard.clone())
            .await
            .map_err(|error| map_lan_network_error(error, guard))
    }

    async fn run_transport(
        &self,
        transport: HostTransport,
        store: SqliteTrustStore,
        config: PairingConfig,
        invitation: Option<&PairingInvitation>,
        approval: &mut UiApproval,
    ) -> Result<PairingOutcome<HostTransport>, PeerError> {
        PairingEngine::new(transport, &self.identity, Box::new(store), config)
            .map_err(classify)?
            .run(invitation, approval)
            .await
            .map_err(classify)
    }
}

fn map_forget_trust_error(error: TrustStoreError) -> PeerError {
    match error {
        TrustStoreError::UnknownOrRevokedPeer | TrustStoreError::PeerRevoked => {
            PeerError::Authentication
        }
        _ => PeerError::Profile,
    }
}

fn map_lan_network_error(error: kryptopass_lan::LanError, guard: &NetworkPolicyGuard) -> PeerError {
    match error {
        kryptopass_lan::LanError::NetworkPolicy => PeerError::NetworkPrivacyBlocked(
            guard
                .ensure_current()
                .err()
                .unwrap_or(NetworkPolicyError::RouteChanged),
        ),
        _ => PeerError::Link,
    }
}

fn reset_marker_exists(directory: &Path) -> Result<bool, PeerError> {
    let marker = directory.join(PROFILE_RESET_MARKER);
    match fs::symlink_metadata(&marker) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(PeerError::Profile),
        Ok(_) => {
            validate_private_artifact(&marker, PROFILE_RESET_MARKER_CONTENT.len() as u64)?;
            let content = fs::read(&marker).map_err(|_| PeerError::Profile)?;
            if content.as_slice() != PROFILE_RESET_MARKER_CONTENT {
                return Err(PeerError::Profile);
            }
            Ok(true)
        }
    }
}

fn validate_reset_ready_directory(directory: &Path) -> Result<(), PeerError> {
    validate_reset_profile_artifacts(directory, true, true)
}

fn validate_reset_profile_artifacts(
    directory: &Path,
    reset_ready_only: bool,
    validate_marker_content: bool,
) -> Result<(), PeerError> {
    #[cfg(unix)]
    {
        let mut marker_found = false;
        for entry in fs::read_dir(directory).map_err(|_| PeerError::Profile)? {
            let entry = entry.map_err(|_| PeerError::Profile)?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(PeerError::Profile)?;
            let path = entry.path();
            match name {
                ".device-identity.lock" => validate_private_artifact(&path, 64)?,
                PROFILE_RESET_MARKER => {
                    validate_private_artifact(&path, PROFILE_RESET_MARKER_CONTENT.len() as u64)?;
                    if validate_marker_content {
                        let content = fs::read(&path).map_err(|_| PeerError::Profile)?;
                        if content.as_slice() != PROFILE_RESET_MARKER_CONTENT {
                            return Err(PeerError::Profile);
                        }
                    }
                    marker_found = true;
                }
                "device-trust.sqlite3"
                | "device-trust.sqlite3-wal"
                | "device-trust.sqlite3-shm"
                | "device-trust.sqlite3-journal" => {
                    if reset_ready_only {
                        return Err(PeerError::Profile);
                    }
                    validate_private_artifact(&path, 16 * 1024 * 1024)?;
                }
                "internet-routes-v1.json" => {
                    if reset_ready_only {
                        return Err(PeerError::Profile);
                    }
                    validate_private_artifact(&path, 1024 * 1024)?;
                }
                temporary
                    if temporary.starts_with(".internet-routes-")
                        && temporary.ends_with(".tmp") =>
                {
                    if reset_ready_only {
                        return Err(PeerError::Profile);
                    }
                    validate_private_artifact(&path, 1024 * 1024)?;
                }
                "device-labels-v1.sqlite3"
                | "device-labels-v1.sqlite3-wal"
                | "device-labels-v1.sqlite3-shm"
                | "device-labels-v1.sqlite3-journal" => {
                    if reset_ready_only {
                        return Err(PeerError::Profile);
                    }
                    validate_private_artifact(&path, 1024 * 1024)?;
                }
                // Forgotten authors verify vault history, which is not pairing
                // state: a reset keeps them.
                profile_trust::DIRECTORY => profile_trust::validate_directory(&path)?,
                _ => return Err(PeerError::Profile),
            }
        }
        if reset_ready_only && !marker_found {
            return Err(PeerError::Profile);
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (directory, reset_ready_only, validate_marker_content);
        Err(PeerError::Profile)
    }
}

#[cfg(unix)]
fn validate_private_artifact(path: &Path, maximum_size: u64) -> Result<(), PeerError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path).map_err(|_| PeerError::Profile)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.nlink() != 1
        || metadata.mode() & 0o777 != 0o600
        || metadata.len() > maximum_size
    {
        return Err(PeerError::Profile);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_artifact(_path: &Path, _maximum_size: u64) -> Result<(), PeerError> {
    Err(PeerError::Profile)
}

fn write_reset_marker(directory: &Path) -> Result<(), PeerError> {
    let marker = directory.join(PROFILE_RESET_MARKER);
    match fs::symlink_metadata(&marker) {
        Ok(_) => {
            validate_private_artifact(&marker, PROFILE_RESET_MARKER_CONTENT.len() as u64)?;
            let content = fs::read(&marker).map_err(|_| PeerError::Profile)?;
            if content.as_slice() == PROFILE_RESET_MARKER_CONTENT {
                return sync_profile_directory(directory);
            }
            fs::remove_file(&marker).map_err(|_| PeerError::Profile)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(PeerError::Profile),
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&marker) {
        Ok(mut file) => {
            file.write_all(PROFILE_RESET_MARKER_CONTENT)
                .map_err(|_| PeerError::Profile)?;
            file.sync_all().map_err(|_| PeerError::Profile)?;
        }
        Err(_) => return Err(PeerError::Profile),
    }
    sync_profile_directory(directory)
}

fn remove_reset_marker(directory: &Path) -> Result<(), PeerError> {
    if !reset_marker_exists(directory)? {
        return Err(PeerError::Profile);
    }
    fs::remove_file(directory.join(PROFILE_RESET_MARKER)).map_err(|_| PeerError::Profile)?;
    sync_profile_directory(directory)
}

fn sync_profile_directory(directory: &Path) -> Result<(), PeerError> {
    fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| PeerError::Profile)
}

struct UiApproval {
    events: mpsc::Sender<HostEvent>,
    decisions: Option<oneshot::Receiver<LocalDecision>>,
}

struct RejectUnexpectedApproval;

#[async_trait]
impl LocalApproval for RejectUnexpectedApproval {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        LocalDecision::Reject
    }
}

#[async_trait]
impl LocalApproval for UiApproval {
    async fn confirm_sas(&mut self, sas: &SasDisplay) -> LocalDecision {
        if self
            .events
            .send(HostEvent::Sas(Box::new(*sas)))
            .await
            .is_err()
        {
            return LocalDecision::Cancel;
        }
        match self.decisions.take() {
            Some(receiver) => receiver.await.unwrap_or(LocalDecision::Cancel),
            None => LocalDecision::Cancel,
        }
    }
}

/// Public identifiers only; never use this helper to log key seeds or invitations.
pub fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_ms() -> Result<i64, PeerError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| PeerError::Expired)?
            .as_millis(),
    )
    .map_err(|_| PeerError::Expired)
}

fn classify(error: PairingError) -> PeerError {
    match error {
        PairingError::Expired => PeerError::Expired,
        PairingError::Cancelled => PeerError::Cancelled,
        PairingError::LocalRejected | PairingError::RemoteRejected => PeerError::Rejected,
        _ => PeerError::Authentication,
    }
}

fn classify_reconnect(error: PairingError) -> PeerError {
    match error {
        PairingError::Transport(_) => PeerError::Link,
        PairingError::TrustStore(_) => PeerError::Profile,
        other => classify(other),
    }
}
