//! Explicit local pairing laboratory. No vault or OpenPGP operations.

use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use kryptopass_identity::{
    DeviceIdentity, PublicIdentity, SecureKeyStore, SqliteTrustStore, SystemSecretKeyStore,
};
use kryptopass_lan::LanHost;
use kryptopass_pairing::{
    LocalApproval, LocalDecision, PairingConfig, PairingEngine, PairingError, PairingInvitation,
    PairingRole, SasDisplay,
};
use kryptopass_transport::{LoopbackTransport, PeerTransport};
use thiserror::Error;
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot, watch},
    time::timeout,
};

/// UI events. No Debug implementation: bootstrap includes a one-time bearer credential.
pub enum HostEvent {
    /// Bootstrap intended solely for an explicit UI/privately controlled test pipe.
    Bootstrap(String),
    /// An authenticated context is ready for independent local comparison.
    Sas(Box<SasDisplay>),
}

/// Explicit choice of carrier; neither discovery nor signaling grants trust.
#[derive(Clone, Copy)]
pub enum LabNetwork {
    /// Original emulator-only adapter, bound to IPv4 loopback.
    EmulatorLoopback,
    /// Reliable ordered WebRTC DataChannel, with local WebSocket signaling.
    Lan { bind_ip: IpAddr },
}

/// Local addresses available for the explicitly enabled LAN listener.
pub fn lan_addresses() -> Result<Vec<IpAddr>, LabError> {
    kryptopass_lan::local_addresses().map_err(|_| LabError::Link)
}

enum LocalListener {
    Emulator(TcpListener),
    Lan(LanHost),
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
}

/// A host profile whose secret identity is owned here, never projected into QML.
pub struct LabHost {
    identity: DeviceIdentity,
    directory: PathBuf,
}

impl LabHost {
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
            return Self::open_existing_system_profile(directory);
        }

        let keys = SystemSecretKeyStore::new(directory, "emulator-pairing-lab-v1")
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
        let keys = SystemSecretKeyStore::new(directory, "emulator-pairing-lab-v1")
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
        mut cancelled: watch::Receiver<bool>,
        network: LabNetwork,
    ) -> Result<PublicIdentity, LabError> {
        if *cancelled.borrow() {
            return Err(LabError::Cancelled);
        }
        let operation = self.run_inner(expected_peer, events, decisions, network);
        tokio::select! {
            result = timeout(Duration::from_secs(300), operation) => result.map_err(|_| LabError::Expired)?,
            _ = cancelled.changed() => Err(LabError::Cancelled),
        }
    }

    async fn run_inner(
        &self,
        expected_peer: Option<PublicIdentity>,
        events: mpsc::Sender<HostEvent>,
        decisions: oneshot::Receiver<LocalDecision>,
        network: LabNetwork,
    ) -> Result<PublicIdentity, LabError> {
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
        let listener = match network {
            LabNetwork::EmulatorLoopback => LocalListener::Emulator(
                TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                    .await
                    .map_err(|_| LabError::Link)?,
            ),
            LabNetwork::Lan { bind_ip } => LocalListener::Lan(
                LanHost::bind(bind_ip, session_id)
                    .await
                    .map_err(|_| LabError::Link)?,
            ),
        };
        let port = match &listener {
            LocalListener::Emulator(listener) => {
                listener.local_addr().map_err(|_| LabError::Link)?.port()
            }
            LocalListener::Lan(listener) => listener.port(),
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
        let bootstrap = match network {
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
        match listener {
            LocalListener::Emulator(listener) => {
                let (stream, address) = listener.accept().await.map_err(|_| LabError::Link)?;
                drop(listener);
                if !address.ip().is_loopback() {
                    return Err(LabError::Link);
                }
                let transport = LoopbackTransport::new(stream).map_err(|_| LabError::Link)?;
                self.run_transport(transport, store, config, invitation.as_ref(), &mut approval)
                    .await
            }
            LocalListener::Lan(listener) => {
                let transport = listener.accept().await.map_err(|_| LabError::Link)?;
                self.run_transport(transport, store, config, invitation.as_ref(), &mut approval)
                    .await
            }
        }
    }

    async fn run_transport<T: PeerTransport>(
        &self,
        transport: T,
        store: SqliteTrustStore,
        config: PairingConfig,
        invitation: Option<&PairingInvitation>,
        approval: &mut UiApproval,
    ) -> Result<PublicIdentity, LabError> {
        let mut result = PairingEngine::new(transport, &self.identity, store, config)
            .map_err(classify)?
            .run(invitation, approval)
            .await
            .map_err(classify)?;
        let peer = result.peer;
        // Trust is already committed. Closing cannot turn a successful commit into a UI failure.
        let _ = result.session.close().await;
        Ok(peer)
    }
}

struct UiApproval {
    events: mpsc::Sender<HostEvent>,
    decisions: Option<oneshot::Receiver<LocalDecision>>,
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
