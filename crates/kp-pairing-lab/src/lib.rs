//! Explicit local pairing laboratory. No vault or OpenPGP operations.

mod aliases;
mod internet_routes;
mod network_privacy;
mod resume_owner;
mod vault_binding;
pub use aliases::AliasError;
pub use network_privacy::{
    NetworkPrivacySettings, load_network_privacy_settings, save_network_privacy_settings,
};
pub use resume_owner::{
    ResumeOwnerReady, ResumeOwnerSnapshot, ResumeProcessOwner, spawn_resume_process_owner,
};
pub use vault_binding::VaultBindingInfo;

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
    DeviceId, DeviceIdentity, PublicIdentity, SecureKeyStore, SqliteTrustStore,
    SystemSecretKeyStore, TrustStoreError,
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
use kryptopass_transport::{LoopbackTransport, PeerTransport, TransportError, TransportFrame};
use thiserror::Error;
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot, watch},
    time::timeout,
};

const RECONNECT_PAIRING_TIMEOUT: Duration = Duration::from_secs(60);
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
pub enum LabNetwork {
    /// Original emulator-only adapter, bound to IPv4 loopback.
    EmulatorLoopback,
    /// Reliable ordered WebRTC DataChannel, with local WebSocket signaling.
    Lan { bind_ip: IpAddr },
    /// WebRTC through a public untrusted rendezvous relay; identity stays in Noise.
    Internet { route: RelayRoute },
}

impl LabNetwork {
    /// Creates a fresh rendezvous room; its random address never grants trust.
    pub fn internet(relay_origin: &str) -> Result<Self, LabError> {
        let mut room = [0_u8; 32];
        getrandom::fill(&mut room).map_err(|_| LabError::Link)?;
        let route = RelayRoute::new(relay_origin, room).map_err(|_| LabError::Link)?;
        Ok(Self::Internet { route })
    }
}

/// Local addresses available for the explicitly enabled LAN listener.
pub fn lan_addresses() -> Result<Vec<IpAddr>, LabError> {
    kryptopass_lan::local_addresses().map_err(|_| LabError::Link)
}

enum LocalListener {
    Emulator(TcpListener),
    Lan(LanHost),
    Internet(Box<RelayHost>),
}

/// Carrier retained after pairing, independent of the identity and session owner.
pub enum LabTransport {
    /// Bounded emulator-only TCP adapter.
    Emulator(LoopbackTransport),
    /// Local WebRTC DataChannel adapter.
    Lan(WebRtcTransport),
}

#[async_trait]
impl PeerTransport for LabTransport {
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
pub enum LabError {
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
pub struct LabHost {
    identity: DeviceIdentity,
    directory: PathBuf,
}

impl LabHost {
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
    pub fn network_privacy_settings(&self) -> Result<NetworkPrivacySettings, LabError> {
        load_network_privacy_settings(&self.directory)
    }

    /// Construct a route guard pinned to this profile's persisted policy.
    pub fn network_policy_guard(&self) -> Result<NetworkPolicyGuard, LabError> {
        let settings = self.network_privacy_settings()?;
        Ok(NetworkPolicyGuard::new(settings.policy))
    }

    /// Load public relay routes only for peers still authorized in this profile.
    pub fn internet_routes(
        &self,
    ) -> Result<std::collections::BTreeMap<DeviceId, RelayRoute>, LabError> {
        internet_routes::read(
            &self.directory,
            self.identity.device_id(),
            &self.trusted_peers()?,
        )
    }

    /// Save routing only after a successful identity-authenticated operation.
    pub fn set_internet_route(&self, peer: DeviceId, route: &RelayRoute) -> Result<(), LabError> {
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
        aliases::read(&self.directory, self.identity.device_id(), &peers)
    }

    /// Persists a local label without changing identity or authorization.
    /// An empty label removes the alias. Unknown or revoked peers are rejected.
    pub fn set_device_alias(
        &self,
        peer: kryptopass_identity::DeviceId,
        alias: &str,
    ) -> Result<(), AliasError> {
        self.lookup(peer).map_err(|_| AliasError::UnknownDevice)?;
        aliases::write(&self.directory, self.identity.device_id(), peer, alias)
    }

    /// Forgets an authorized peer and its local alias so it can be paired again.
    ///
    /// Call only after closing the peer's session and serializing all profile
    /// operations. Alias removal happens first; if trust removal then fails,
    /// the peer remains trusted without its local label and this method returns
    /// an error. Revoked tombstones are never removed by this operation.
    pub fn forget_peer(&self, peer: DeviceId) -> Result<(), LabError> {
        let mut store = SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| LabError::Profile)?;
        store
            .lookup_trusted_peer(&peer)
            .map_err(map_forget_trust_error)?;
        internet_routes::write(&self.directory, self.identity.device_id(), peer, None)?;
        aliases::forget(&self.directory, self.identity.device_id(), peer)
            .map_err(|_| LabError::Profile)?;
        store
            .forget_authorized_peer(&peer)
            .map_err(map_forget_trust_error)
    }

    /// Resets only this pairing-lab profile's OS keyring identity, trust DB,
    /// and local device-label DB. It never touches the vault, OpenPGP keys, or
    /// GnuPG directory. Call only after dropping the `LabHost`, stopping all
    /// sessions, and serializing profile operations.
    ///
    /// The keyring credential is removed first so an incomplete filesystem
    /// cleanup cannot authorize new sessions. The fixed DB paths are checked
    /// before deletion. If a later step fails, this returns an error and does
    /// not recreate any identity or database; a subsequent reset can retry.
    pub fn reset_system_profile(directory: &Path) -> Result<(), LabError> {
        let keys = SystemSecretKeyStore::new(directory, SYSTEM_PROFILE_KEYRING_ID)
            .map_err(|_| LabError::KeyStore)?;
        SqliteTrustStore::validate_reset_targets(directory).map_err(|_| LabError::Profile)?;
        aliases::validate_reset_targets(directory).map_err(|_| LabError::Profile)?;
        internet_routes::validate_target(directory)?;
        validate_reset_profile_artifacts(directory, false, false)?;

        internet_routes::reset(directory)?;
        keys.delete().map_err(|_| LabError::KeyStore)?;
        SqliteTrustStore::reset_profile_files(directory).map_err(|_| LabError::Profile)?;
        aliases::reset_profile_files(directory).map_err(|_| LabError::Profile)?;
        write_reset_marker(directory)?;
        Ok(())
    }

    /// Opens an OS-backed profile for an explicit pairing operation.
    ///
    /// If the directory does not exist, this initializes a new identity and
    /// trust database. Existing directories are loaded through
    /// [`Self::open_existing_system_profile`]; missing or corrupt material is
    /// never repaired by generating replacement keys or a new database.
    pub fn open_system_profile(directory: &Path) -> Result<Self, LabError> {
        #[cfg(unix)]
        let created = {
            use std::os::unix::fs::DirBuilderExt;
            match fs::DirBuilder::new().mode(0o700).create(directory) {
                Ok(()) => true,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
                Err(_) => return Err(LabError::Profile),
            }
        };
        #[cfg(not(unix))]
        let created = match fs::create_dir(directory) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(_) => return Err(LabError::Profile),
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

    fn initialize_new_profile(directory: &Path) -> Result<Self, LabError> {
        let keys = SystemSecretKeyStore::new(directory, SYSTEM_PROFILE_KEYRING_ID)
            .map_err(|_| LabError::KeyStore)?;
        if keys.load().map_err(|_| LabError::KeyStore)?.is_some() {
            return Err(LabError::Profile);
        }
        let identity = DeviceIdentity::create_new(&keys).map_err(|_| LabError::KeyStore)?;
        SqliteTrustStore::open_new(directory, identity.device_id())
            .map_err(|_| LabError::Profile)?;
        Ok(Self {
            identity,
            directory: directory.to_path_buf(),
        })
    }

    fn initialize_reset_profile(directory: &Path) -> Result<Self, LabError> {
        let keys = SystemSecretKeyStore::new(directory, SYSTEM_PROFILE_KEYRING_ID)
            .map_err(|_| LabError::KeyStore)?;
        validate_reset_profile_artifacts(directory, false, true)?;
        let existing = keys.load().map_err(|_| LabError::KeyStore)?;
        if existing.is_some() {
            drop(existing);
            let identity = DeviceIdentity::load_existing(&keys).map_err(|_| LabError::KeyStore)?;
            let store = SqliteTrustStore::open_existing(directory, identity.device_id())
                .map_err(|_| LabError::Profile)?;
            let peers = store.list_trusted_peers().map_err(|_| LabError::Profile)?;
            drop(store);
            aliases::read(directory, identity.device_id(), &peers)
                .map_err(|_| LabError::Profile)?;
            remove_reset_marker(directory)?;
            return Ok(Self {
                identity,
                directory: directory.to_path_buf(),
            });
        }

        validate_reset_ready_directory(directory)?;
        let identity = DeviceIdentity::create_new(&keys).map_err(|_| LabError::KeyStore)?;
        SqliteTrustStore::open_new(directory, identity.device_id())
            .map_err(|_| LabError::Profile)?;
        remove_reset_marker(directory)?;
        Ok(Self {
            identity,
            directory: directory.to_path_buf(),
        })
    }

    /// Opens an existing system profile without creating or repairing anything.
    ///
    /// The directory, keyring identity, and bound trust database must all
    /// already exist and validate. This is the entry point for loading peers
    /// and offering reconnect after an application restart.
    pub fn open_existing_system_profile(directory: &Path) -> Result<Self, LabError> {
        let metadata = fs::symlink_metadata(directory).map_err(|_| LabError::Profile)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(LabError::Profile);
        }
        let keys = SystemSecretKeyStore::new(directory, SYSTEM_PROFILE_KEYRING_ID)
            .map_err(|_| LabError::KeyStore)?;
        let identity = DeviceIdentity::load_existing(&keys).map_err(|_| LabError::KeyStore)?;
        let store = SqliteTrustStore::open_existing(directory, identity.device_id())
            .map_err(|_| LabError::Profile)?;
        drop(store);
        Ok(Self {
            identity,
            directory: directory.to_path_buf(),
        })
    }

    /// Test-process-only host. Its synthetic private identity is held in RAM, never written.
    /// The caller owns a fresh private directory and must remove it after the test.
    pub fn ephemeral_test_profile(directory: &Path) -> Result<Self, LabError> {
        let identity = DeviceIdentity::generate().map_err(|_| LabError::Authentication)?;
        SqliteTrustStore::open_new(directory, identity.device_id())
            .map_err(|_| LabError::Profile)?;
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
    pub fn lookup(&self, id: kryptopass_identity::DeviceId) -> Result<PublicIdentity, LabError> {
        SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| LabError::Profile)?
            .lookup_trusted_peer(&id)
            .map_err(|_| LabError::Authentication)
    }

    /// Lists currently authorized peers from the existing profile database.
    /// No missing directory, identity, or trust database is created here.
    pub fn trusted_peers(&self) -> Result<Vec<PublicIdentity>, LabError> {
        SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| LabError::Profile)?
            .list_trusted_peers()
            .map_err(|_| LabError::Profile)
    }

    /// Hosts exactly one bounded emulator pairing/reconnect, bound to loopback.
    pub async fn run_once(
        &self,
        expected_peer: Option<PublicIdentity>,
        events: mpsc::Sender<HostEvent>,
        decisions: oneshot::Receiver<LocalDecision>,
        cancelled: watch::Receiver<bool>,
    ) -> Result<PublicIdentity, LabError> {
        self.run_once_with_network(
            expected_peer,
            events,
            decisions,
            cancelled,
            LabNetwork::EmulatorLoopback,
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
        network: LabNetwork,
    ) -> Result<PublicIdentity, LabError> {
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
        network: LabNetwork,
    ) -> Result<PairingOutcome<LabTransport>, LabError> {
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
        network: LabNetwork,
        guard: NetworkPolicyGuard,
    ) -> Result<PairingOutcome<LabTransport>, LabError> {
        if *cancelled.borrow() {
            return Err(LabError::Cancelled);
        }
        let network_use = match &network {
            LabNetwork::EmulatorLoopback => None,
            LabNetwork::Lan { .. } => Some(NetworkUse::Lan),
            LabNetwork::Internet { .. } => Some(NetworkUse::Internet),
        };
        if let Some(network_use) = network_use {
            guard.preflight(network_use)?;
        }
        let operation = self.run_inner(expected_peer, events, decisions, network, guard.clone());
        tokio::select! {
            result = timeout(Duration::from_secs(300), operation) => result.map_err(|_| LabError::Expired)?,
            _ = cancelled.changed() => Err(LabError::Cancelled),
            error = guard.wait_for_change(Duration::from_millis(250)), if network_use.is_some() => {
                Err(LabError::NetworkPrivacyBlocked(error))
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
    ) -> Result<PairingOutcome<LabTransport>, LabError> {
        if *cancelled.borrow() {
            return Err(LabError::Cancelled);
        }
        expected_peer
            .validate()
            .map_err(|_| LabError::Authentication)?;
        if expected_peer.device_id == self.identity.device_id() {
            return Err(LabError::Authentication);
        }
        let store = SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| LabError::Profile)?;
        if store
            .lookup_trusted_peer(&expected_peer.device_id)
            .map_err(|_| LabError::Authentication)?
            != expected_peer
        {
            return Err(LabError::Authentication);
        }

        let mut config =
            PairingConfig::reconnect(session_id, PairingRole::Responder, now_ms()?, expected_peer);
        config.timeout = RECONNECT_PAIRING_TIMEOUT;
        let mut approval = RejectUnexpectedApproval;
        let operation = async {
            PairingEngine::new(LabTransport::Lan(transport), &self.identity, store, config)
                .map_err(classify_reconnect)?
                .run(None, &mut approval)
                .await
                .map_err(classify_reconnect)
        };
        tokio::select! {
            result = timeout(RECONNECT_PAIRING_TIMEOUT, operation) => {
                result.unwrap_or(Err(LabError::Expired))
            }
            _ = cancelled.changed() => Err(LabError::Cancelled),
        }
    }

    async fn run_inner(
        &self,
        expected_peer: Option<PublicIdentity>,
        events: mpsc::Sender<HostEvent>,
        decisions: oneshot::Receiver<LocalDecision>,
        network: LabNetwork,
        guard: NetworkPolicyGuard,
    ) -> Result<PairingOutcome<LabTransport>, LabError> {
        let mut store = SqliteTrustStore::open_existing(&self.directory, self.identity.device_id())
            .map_err(|_| LabError::Profile)?;
        if let Some(peer) = expected_peer {
            if store
                .lookup_trusted_peer(&peer.device_id)
                .map_err(|_| LabError::Authentication)?
                != peer
            {
                return Err(LabError::Authentication);
            }
        }
        let mut session_id = [0; 16];
        getrandom::fill(&mut session_id).map_err(|_| LabError::Authentication)?;
        if session_id.iter().all(|byte| *byte == 0) {
            return Err(LabError::Authentication);
        }
        let listener = match &network {
            LabNetwork::EmulatorLoopback => LocalListener::Emulator(
                TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                    .await
                    .map_err(|_| LabError::Link)?,
            ),
            LabNetwork::Lan { bind_ip } => {
                guard.preflight(NetworkUse::Lan)?;
                LocalListener::Lan(
                    LanHost::bind(*bind_ip, session_id)
                        .await
                        .map_err(|_| LabError::Link)?,
                )
            }
            LabNetwork::Internet { route } => {
                let (config, policy) = self
                    .internet_ice_configuration(&guard)
                    .await
                    .map_err(|error| map_lan_network_error(error, &guard))?;
                LocalListener::Internet(Box::new(
                    RelayHost::connect_with_privacy(route.clone(), config, policy, guard.clone())
                        .await
                        .map_err(|error| map_lan_network_error(error, &guard))?,
                ))
            }
        };
        let port = match &listener {
            LocalListener::Emulator(listener) => {
                listener.local_addr().map_err(|_| LabError::Link)?.port()
            }
            LocalListener::Lan(listener) => listener.port(),
            LocalListener::Internet(_) => 0,
        };
        let invitation = if expected_peer.is_none() {
            Some(
                PairingInvitation::create(&mut store, now_ms()?, Duration::from_secs(300))
                    .map_err(|_| LabError::Profile)?,
            )
        } else {
            None
        };
        let session_hex = hex(&session_id);
        let data = if let Some(invite) = invitation.as_ref() {
            let qr = invite.to_qr_bytes().map_err(|_| LabError::Authentication)?;
            URL_SAFE_NO_PAD.encode(qr.as_bytes())
        } else {
            self.identity.device_id().to_string()
        };
        let bootstrap = match &network {
            LabNetwork::EmulatorLoopback => format!(
                "{}:{port}:{session_hex}:{data}",
                if invitation.is_some() {
                    "kpdev1"
                } else {
                    "kpdevr1"
                }
            ),
            LabNetwork::Lan { bind_ip } => {
                let value = serde_json::json!({
                    "v": 1, "host": bind_ip.to_string(), "port": port,
                    "session": session_hex, "kind": if invitation.is_some() { "pair" } else { "reconnect" }, "data": data,
                });
                let bytes = serde_json::to_vec(&value).map_err(|_| LabError::Link)?;
                format!("kplan1:{}", URL_SAFE_NO_PAD.encode(bytes))
            }
            LabNetwork::Internet { route } => {
                let value = serde_json::json!({
                    "v": 1, "relay": route.relay_origin(), "room": route.room_hex(),
                    "session": session_hex, "kind": if invitation.is_some() { "pair" } else { "reconnect" }, "data": data,
                });
                let bytes = serde_json::to_vec(&value).map_err(|_| LabError::Link)?;
                format!("kpnet1:{}", URL_SAFE_NO_PAD.encode(bytes))
            }
        };
        events
            .send(HostEvent::Bootstrap(bootstrap))
            .await
            .map_err(|_| LabError::Cancelled)?;
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
                let (stream, address) = listener.accept().await.map_err(|_| LabError::Link)?;
                drop(listener);
                if !address.ip().is_loopback() {
                    return Err(LabError::Link);
                }
                let transport = LoopbackTransport::new(stream).map_err(|_| LabError::Link)?;
                self.run_transport(
                    LabTransport::Emulator(transport),
                    store,
                    config,
                    invitation.as_ref(),
                    &mut approval,
                )
                .await
            }
            LocalListener::Lan(listener) => {
                let mut transport = tokio::select! {
                    result = listener.accept() => result.map_err(|_| LabError::Link)?,
                    error = guard.wait_for_change(Duration::from_millis(250)) => {
                        return Err(LabError::NetworkPrivacyBlocked(error));
                    }
                };
                transport
                    .enforce_network_policy(guard.clone())
                    .map_err(LabError::NetworkPrivacyBlocked)?;
                self.run_transport(
                    LabTransport::Lan(transport),
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
                        LabTransport::Lan(attempt.transport),
                        store,
                        config,
                        invitation.as_ref(),
                        &mut approval,
                    )
                    .await?;
                Ok(outcome)
            }
        }?;
        if let LabNetwork::Internet { route } = network {
            self.set_internet_route(outcome.peer.device_id, &route)?;
        }
        Ok(outcome)
    }

    async fn run_transport(
        &self,
        transport: LabTransport,
        store: SqliteTrustStore,
        config: PairingConfig,
        invitation: Option<&PairingInvitation>,
        approval: &mut UiApproval,
    ) -> Result<PairingOutcome<LabTransport>, LabError> {
        PairingEngine::new(transport, &self.identity, store, config)
            .map_err(classify)?
            .run(invitation, approval)
            .await
            .map_err(classify)
    }
}

fn map_forget_trust_error(error: TrustStoreError) -> LabError {
    match error {
        TrustStoreError::UnknownOrRevokedPeer | TrustStoreError::PeerRevoked => {
            LabError::Authentication
        }
        _ => LabError::Profile,
    }
}

fn map_lan_network_error(error: kryptopass_lan::LanError, guard: &NetworkPolicyGuard) -> LabError {
    match error {
        kryptopass_lan::LanError::NetworkPolicy => LabError::NetworkPrivacyBlocked(
            guard
                .ensure_current()
                .err()
                .unwrap_or(NetworkPolicyError::RouteChanged),
        ),
        _ => LabError::Link,
    }
}

fn reset_marker_exists(directory: &Path) -> Result<bool, LabError> {
    let marker = directory.join(PROFILE_RESET_MARKER);
    match fs::symlink_metadata(&marker) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(LabError::Profile),
        Ok(_) => {
            validate_private_artifact(&marker, PROFILE_RESET_MARKER_CONTENT.len() as u64)?;
            let content = fs::read(&marker).map_err(|_| LabError::Profile)?;
            if content.as_slice() != PROFILE_RESET_MARKER_CONTENT {
                return Err(LabError::Profile);
            }
            Ok(true)
        }
    }
}

fn validate_reset_ready_directory(directory: &Path) -> Result<(), LabError> {
    validate_reset_profile_artifacts(directory, true, true)
}

fn validate_reset_profile_artifacts(
    directory: &Path,
    reset_ready_only: bool,
    validate_marker_content: bool,
) -> Result<(), LabError> {
    #[cfg(unix)]
    {
        let mut marker_found = false;
        for entry in fs::read_dir(directory).map_err(|_| LabError::Profile)? {
            let entry = entry.map_err(|_| LabError::Profile)?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(LabError::Profile)?;
            let path = entry.path();
            match name {
                ".device-identity.lock" => validate_private_artifact(&path, 64)?,
                PROFILE_RESET_MARKER => {
                    validate_private_artifact(&path, PROFILE_RESET_MARKER_CONTENT.len() as u64)?;
                    if validate_marker_content {
                        let content = fs::read(&path).map_err(|_| LabError::Profile)?;
                        if content.as_slice() != PROFILE_RESET_MARKER_CONTENT {
                            return Err(LabError::Profile);
                        }
                    }
                    marker_found = true;
                }
                "device-trust.sqlite3"
                | "device-trust.sqlite3-wal"
                | "device-trust.sqlite3-shm"
                | "device-trust.sqlite3-journal" => {
                    if reset_ready_only {
                        return Err(LabError::Profile);
                    }
                    validate_private_artifact(&path, 16 * 1024 * 1024)?;
                }
                "internet-routes-v1.json" => {
                    if reset_ready_only {
                        return Err(LabError::Profile);
                    }
                    validate_private_artifact(&path, 1024 * 1024)?;
                }
                temporary
                    if temporary.starts_with(".internet-routes-")
                        && temporary.ends_with(".tmp") =>
                {
                    if reset_ready_only {
                        return Err(LabError::Profile);
                    }
                    validate_private_artifact(&path, 1024 * 1024)?;
                }
                "device-labels-v1.sqlite3"
                | "device-labels-v1.sqlite3-wal"
                | "device-labels-v1.sqlite3-shm"
                | "device-labels-v1.sqlite3-journal" => {
                    if reset_ready_only {
                        return Err(LabError::Profile);
                    }
                    validate_private_artifact(&path, 1024 * 1024)?;
                }
                _ => return Err(LabError::Profile),
            }
        }
        if reset_ready_only && !marker_found {
            return Err(LabError::Profile);
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (directory, reset_ready_only, validate_marker_content);
        Err(LabError::Profile)
    }
}

#[cfg(unix)]
fn validate_private_artifact(path: &Path, maximum_size: u64) -> Result<(), LabError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path).map_err(|_| LabError::Profile)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.nlink() != 1
        || metadata.mode() & 0o777 != 0o600
        || metadata.len() > maximum_size
    {
        return Err(LabError::Profile);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_artifact(_path: &Path, _maximum_size: u64) -> Result<(), LabError> {
    Err(LabError::Profile)
}

fn write_reset_marker(directory: &Path) -> Result<(), LabError> {
    let marker = directory.join(PROFILE_RESET_MARKER);
    match fs::symlink_metadata(&marker) {
        Ok(_) => {
            validate_private_artifact(&marker, PROFILE_RESET_MARKER_CONTENT.len() as u64)?;
            let content = fs::read(&marker).map_err(|_| LabError::Profile)?;
            if content.as_slice() == PROFILE_RESET_MARKER_CONTENT {
                return sync_profile_directory(directory);
            }
            fs::remove_file(&marker).map_err(|_| LabError::Profile)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(LabError::Profile),
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
                .map_err(|_| LabError::Profile)?;
            file.sync_all().map_err(|_| LabError::Profile)?;
        }
        Err(_) => return Err(LabError::Profile),
    }
    sync_profile_directory(directory)
}

fn remove_reset_marker(directory: &Path) -> Result<(), LabError> {
    if !reset_marker_exists(directory)? {
        return Err(LabError::Profile);
    }
    fs::remove_file(directory.join(PROFILE_RESET_MARKER)).map_err(|_| LabError::Profile)?;
    sync_profile_directory(directory)
}

fn sync_profile_directory(directory: &Path) -> Result<(), LabError> {
    fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| LabError::Profile)
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

fn now_ms() -> Result<i64, LabError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| LabError::Expired)?
            .as_millis(),
    )
    .map_err(|_| LabError::Expired)
}

fn classify(error: PairingError) -> LabError {
    match error {
        PairingError::Expired => LabError::Expired,
        PairingError::Cancelled => LabError::Cancelled,
        PairingError::LocalRejected | PairingError::RemoteRejected => LabError::Rejected,
        _ => LabError::Authentication,
    }
}

fn classify_reconnect(error: PairingError) -> LabError {
    match error {
        PairingError::Transport(_) => LabError::Link,
        PairingError::TrustStore(_) => LabError::Profile,
        other => classify(other),
    }
}
