//! Explicit emulator pairing UI. Private identities remain in the worker/OS store.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use kryptopass_app::SyncSelection;
use kryptopass_core::KeyFingerprint;
use kryptopass_devices::{
    ConnectionState, DeviceDirectory, DeviceDirectorySnapshot, KeyTransferPurpose,
    SessionCloseReason, SessionEvent, SessionFailure, SessionOptions,
};
use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_lan::{
    NetworkPolicyError, NetworkPolicyGuard, NetworkPrivacyPolicy, VpnMode, VpnStatus,
};
use kryptopass_pairing::LocalDecision;
use kryptopass_peers::{
    HostEvent, NetworkPrivacySettings, PeerHost, PeerNetwork, ResumeOwnerSnapshot,
    ResumeProcessOwner, load_network_privacy_settings, save_network_privacy_settings,
    spawn_resume_process_owner,
};
use std::{
    collections::BTreeMap,
    net::IpAddr,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::Duration,
};

mod devices;
mod inbound_key_transfer;
mod key_transfer;
mod network;
mod pairing;
mod resume;
mod vault_sync;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        type QStringList = cxx_qt_lib::QStringList;
    }
    extern "RustQt" {
        /// Debug-only entry point for comparing an emulator with Desktop.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(bool, awaiting, READ, NOTIFY)]
        #[qproperty(bool, can_reconnect, cxx_name = "canReconnect", READ, NOTIFY)]
        #[qproperty(QStringList, device_ids, cxx_name = "deviceIds", READ, NOTIFY)]
        #[qproperty(i32, selected_index, cxx_name = "selectedIndex", READ, NOTIFY)]
        #[qproperty(bool, lan_enabled, cxx_name = "lanEnabled", READ, NOTIFY)]
        #[qproperty(bool, internet_enabled, cxx_name = "internetEnabled", READ, NOTIFY)]
        #[qproperty(bool, require_vpn, cxx_name = "requireVpn", READ, NOTIFY)]
        #[qproperty(
            bool,
            allow_lan_exception,
            cxx_name = "allowLanException",
            READ,
            NOTIFY
        )]
        #[qproperty(bool, relay_only, cxx_name = "relayOnly", READ, NOTIFY)]
        #[qproperty(i32, vpn_status, cxx_name = "vpnStatus", READ, NOTIFY)]
        #[qproperty(QString, relay_origin, cxx_name = "relayOrigin", READ, NOTIFY)]
        #[qproperty(
            QStringList,
            network_addresses,
            cxx_name = "networkAddresses",
            READ,
            NOTIFY
        )]
        #[qproperty(i32, network_index, cxx_name = "networkIndex", READ, NOTIFY)]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(QString, bootstrap, READ, NOTIFY)]
        #[qproperty(QString, sas, READ, NOTIFY)]
        #[qproperty(QString, peer, READ, NOTIFY)]
        #[qproperty(QString, devices_snapshot, cxx_name = "devicesSnapshot", READ, NOTIFY)]
        #[qproperty(QString, bootstrap_qr, cxx_name = "bootstrapQr", READ, NOTIFY)]
        #[qproperty(QString, sas_emojis, cxx_name = "sasEmojis", READ, NOTIFY)]
        #[qproperty(QString, status_code, cxx_name = "statusCode", READ, NOTIFY)]
        #[qproperty(bool, session_active, cxx_name = "sessionActive", READ, NOTIFY)]
        #[qproperty(QString, active_device_id, cxx_name = "activeDeviceId", READ, NOTIFY)]
        #[qproperty(bool, alias_busy, cxx_name = "aliasBusy", READ, NOTIFY)]
        #[qproperty(QString, alias_error, cxx_name = "aliasError", READ, NOTIFY)]
        #[qproperty(bool, maintenance_busy, cxx_name = "maintenanceBusy", READ, NOTIFY)]
        #[qproperty(bool, sync_busy, cxx_name = "syncBusy", READ, NOTIFY)]
        #[qproperty(QString, sync_peer_id, cxx_name = "syncPeerId", READ, NOTIFY)]
        #[qproperty(QString, sync_status, cxx_name = "syncStatus", READ, NOTIFY)]
        #[qproperty(QString, sync_error, cxx_name = "syncError", READ, NOTIFY)]
        #[qproperty(
            i32,
            sync_local_revision_count,
            cxx_name = "syncLocalRevisionCount",
            READ,
            NOTIFY
        )]
        #[qproperty(QStringList, sync_entries, cxx_name = "syncEntries", READ, NOTIFY)]
        #[qproperty(bool, sync_configured, cxx_name = "syncConfigured", READ, NOTIFY)]
        #[qproperty(
            QString,
            synthetic_transfer_fingerprint,
            cxx_name = "syntheticTransferFingerprint",
            READ,
            NOTIFY
        )]
        #[qproperty(
            QStringList,
            vault_sync_peers,
            cxx_name = "vaultSyncPeers",
            READ,
            NOTIFY
        )]
        #[qproperty(QString, vault_sync_keys, cxx_name = "vaultSyncKeys", READ, NOTIFY)]
        #[qproperty(
            bool,
            vault_mirror_active,
            cxx_name = "vaultMirrorActive",
            READ,
            NOTIFY
        )]
        #[qproperty(i32, vault_sync_activity, cxx_name = "vaultSyncActivity", READ, NOTIFY)]
        #[qproperty(
            QString,
            vault_sync_locked_key,
            cxx_name = "vaultSyncLockedKey",
            READ,
            NOTIFY
        )]
        #[qproperty(
            QString,
            vault_sync_message,
            cxx_name = "vaultSyncMessage",
            READ,
            NOTIFY
        )]
        #[qproperty(bool, vault_sync_failed, cxx_name = "vaultSyncFailed", READ, NOTIFY)]
        #[qproperty(
            QString,
            vault_key_fingerprint,
            cxx_name = "vaultKeyFingerprint",
            READ,
            NOTIFY
        )]
        #[qproperty(
            bool,
            synthetic_transfer_preparing,
            cxx_name = "syntheticTransferPreparing",
            READ,
            NOTIFY
        )]
        #[qproperty(
            QString,
            synthetic_transfer_error,
            cxx_name = "syntheticTransferError",
            READ,
            NOTIFY
        )]
        #[qproperty(
            QString,
            inbound_key_transfer,
            cxx_name = "inboundKeyTransfer",
            READ,
            NOTIFY
        )]
        type PeersController = super::PeersControllerRust;

        #[qinvokable]
        #[cxx_name = "loadDevices"]
        fn load_devices(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "startResumeOwner"]
        fn start_resume_owner(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "shutdownResumeOwner"]
        fn shutdown_resume_owner(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "selectDevice"]
        fn select_device(self: Pin<&mut Self>, index: i32);
        #[qinvokable]
        #[cxx_name = "loadNetworkAddresses"]
        fn load_network_addresses(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "configureNetwork"]
        fn configure_network(self: Pin<&mut Self>, enabled: bool, index: i32);
        #[qinvokable]
        #[cxx_name = "configurePrivacy"]
        fn configure_privacy(
            self: Pin<&mut Self>,
            require_vpn: bool,
            allow_lan_exception: bool,
            relay_only: bool,
        );
        #[qinvokable]
        #[cxx_name = "configureInternet"]
        fn configure_internet(self: Pin<&mut Self>, enabled: bool, relay_origin: QString);
        #[qinvokable]
        #[cxx_name = "startPairing"]
        fn start_pairing(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "startReconnect"]
        fn start_reconnect(self: Pin<&mut Self>);
        #[qinvokable]
        fn decide(self: Pin<&mut Self>, accepted: bool);
        #[qinvokable]
        fn cancel(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "disconnectDevice"]
        fn disconnect_device(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "disconnectPeer"]
        fn disconnect_peer(self: Pin<&mut Self>, device_id: QString);
        #[qinvokable]
        #[cxx_name = "releaseUi"]
        fn release_ui(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "saveDeviceAlias"]
        fn save_device_alias(self: Pin<&mut Self>, device_id: QString, alias: QString);
        #[qinvokable]
        #[cxx_name = "forgetDevice"]
        fn forget_device(self: Pin<&mut Self>, device_id: QString);
        #[qinvokable]
        #[cxx_name = "resetPairingProfile"]
        fn reset_pairing_profile(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "startSyntheticKeyTransfer"]
        fn start_synthetic_key_transfer(
            self: Pin<&mut Self>,
            device_id: QString,
            connection_generation: QString,
            approved_fingerprint: QString,
        );
        #[qinvokable]
        #[cxx_name = "startVaultKeyTransfer"]
        fn start_vault_key_transfer(
            self: Pin<&mut Self>,
            device_id: QString,
            connection_generation: QString,
            approved_fingerprint: QString,
        );
        #[qinvokable]
        #[cxx_name = "cancelSyntheticKeyTransfer"]
        fn cancel_synthetic_key_transfer(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "acceptInboundKeyTransfer"]
        fn accept_inbound_key_transfer(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "rejectInboundKeyTransfer"]
        fn reject_inbound_key_transfer(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "dismissInboundKeyTransfer"]
        fn dismiss_inbound_key_transfer(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "retryVaultSync"]
        fn retry_vault_sync(self: Pin<&mut Self>, device_id: QString);
        #[qinvokable]
        #[cxx_name = "unlockSyncKey"]
        fn unlock_sync_key(self: Pin<&mut Self>, fingerprint: QString);
        #[qinvokable]
        #[cxx_name = "enableVaultSync"]
        fn enable_vault_sync(self: Pin<&mut Self>, device_id: QString, fingerprints: QString);
        #[qinvokable]
        #[cxx_name = "disableVaultSync"]
        fn disable_vault_sync(self: Pin<&mut Self>, device_id: QString);
        #[qinvokable]
        #[cxx_name = "runVaultMirror"]
        fn run_vault_mirror(self: Pin<&mut Self>);
        #[qsignal]
        #[cxx_name = "aliasSaved"]
        fn alias_saved(self: Pin<&mut Self>, device_id: &QString);
        /// The mirror wrote phone changes into the vault; views should reload it.
        #[qsignal]
        #[cxx_name = "vaultMirrorPulled"]
        fn vault_mirror_pulled(self: Pin<&mut Self>);
    }
    impl cxx_qt::Threading for PeersController {}
}

pub struct PeersControllerRust {
    busy: bool,
    awaiting: bool,
    can_reconnect: bool,
    device_ids: QStringList,
    selected_index: i32,
    lan_enabled: bool,
    internet_enabled: bool,
    require_vpn: bool,
    allow_lan_exception: bool,
    relay_only: bool,
    vpn_status: i32,
    relay_origin: QString,
    network_addresses: QStringList,
    network_index: i32,
    network_ips: Vec<IpAddr>,
    status: QString,
    bootstrap: QString,
    sas: QString,
    peer: QString,
    devices_snapshot: QString,
    bootstrap_qr: QString,
    sas_emojis: QString,
    status_code: QString,
    session_active: bool,
    active_device_id: QString,
    alias_busy: bool,
    alias_error: QString,
    maintenance_busy: bool,
    sync_busy: bool,
    sync_peer_id: QString,
    sync_status: QString,
    sync_error: QString,
    sync_local_revision_count: i32,
    sync_entries: QStringList,
    sync_configured: bool,
    synthetic_transfer_fingerprint: QString,
    vault_key_fingerprint: QString,
    vault_sync_peers: QStringList,
    vault_sync_keys: QString,
    vault_mirror_active: bool,
    vault_sync_activity: i32,
    vault_sync_locked_key: QString,
    vault_sync_message: QString,
    vault_sync_failed: bool,
    vault_mirror_running: bool,
    synthetic_transfer_preparing: bool,
    synthetic_transfer_error: QString,
    synthetic_transfer_progress:
        BTreeMap<DeviceId, (u64, kryptopass_devices::SyntheticTransferProgress)>,
    synthetic_transfer_cancel: Option<watch::Sender<bool>>,
    synthetic_transfer_generation: u64,
    synthetic_transfer_preparing_peer: Option<(DeviceId, u64)>,
    synthetic_transfer_target: Option<(DeviceId, u64, [u8; 16])>,
    /// Vault key being handed to a phone, so its items follow it into Sync once it is received.
    vault_key_send: Option<(DeviceId, String)>,
    inbound_key_transfer: QString,
    inbound_transfer: Option<inbound_key_transfer::InboundTransferUi>,
    aliases: BTreeMap<DeviceId, String>,
    pending: Option<PendingOperation>,
    active: Option<PendingOperation>,
    draining_worker: Option<Arc<AtomicBool>>,
    devices: Option<DeviceDirectory>,
    known_peers: Vec<PublicIdentity>,
    generation: u64,
    devices_revision: u64,
    resume_owner: Option<ResumeProcessOwner>,
    active_network_guard: Option<NetworkPolicyGuard>,
    resume_owner_generation: u64,
    resume_owner_cancel: Option<watch::Sender<bool>>,
}

impl Default for PeersControllerRust {
    fn default() -> Self {
        Self {
            busy: false,
            awaiting: false,
            can_reconnect: false,
            device_ids: QStringList::default(),
            selected_index: -1,
            lan_enabled: true,
            internet_enabled: true,
            require_vpn: false,
            allow_lan_exception: false,
            relay_only: false,
            vpn_status: 0,
            relay_origin: QString::from("wss://signal.kryptopass.org"),
            network_addresses: QStringList::default(),
            network_index: -1,
            network_ips: Vec::new(),
            status: QString::from("Listo para una prueba local con el emulador."),
            bootstrap: QString::default(),
            sas: QString::default(),
            peer: QString::default(),
            devices_snapshot: QString::from("{\"revision\":0,\"devices\":[]}"),
            bootstrap_qr: QString::default(),
            sas_emojis: QString::from("[]"),
            status_code: QString::from("idle"),
            session_active: false,
            active_device_id: QString::default(),
            alias_busy: false,
            alias_error: QString::default(),
            maintenance_busy: false,
            sync_busy: false,
            sync_peer_id: QString::default(),
            sync_status: QString::default(),
            sync_error: QString::default(),
            sync_local_revision_count: 0,
            sync_entries: QStringList::default(),
            sync_configured: false,
            synthetic_transfer_fingerprint: QString::default(),
            vault_key_fingerprint: QString::default(),
            vault_sync_peers: QStringList::default(),
            vault_sync_keys: QString::default(),
            vault_mirror_active: false,
            vault_sync_activity: 0,
            vault_sync_locked_key: QString::default(),
            vault_sync_message: QString::default(),
            vault_sync_failed: false,
            vault_mirror_running: false,
            synthetic_transfer_preparing: false,
            synthetic_transfer_error: QString::default(),
            synthetic_transfer_progress: BTreeMap::new(),
            synthetic_transfer_cancel: None,
            synthetic_transfer_generation: 0,
            synthetic_transfer_preparing_peer: None,
            synthetic_transfer_target: None,
            vault_key_send: None,
            inbound_key_transfer: QString::from("{\"visible\":false}"),
            inbound_transfer: None,
            aliases: BTreeMap::new(),
            pending: None,
            active: None,
            draining_worker: None,
            devices: None,
            known_peers: Vec::new(),
            generation: 0,
            devices_revision: 0,
            resume_owner: None,
            active_network_guard: None,
            resume_owner_generation: 0,
            resume_owner_cancel: None,
        }
    }
}

struct PendingOperation {
    cancel: watch::Sender<bool>,
    decision: Option<oneshot::Sender<LocalDecision>>,
    completed: Arc<AtomicBool>,
}

struct WorkerCompletion(Arc<AtomicBool>);
impl Drop for WorkerCompletion {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl Drop for PendingOperation {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
    }
}

impl PeersControllerRust {
    fn selected_peer(&self) -> Option<PublicIdentity> {
        usize::try_from(self.selected_index)
            .ok()
            .and_then(|index| self.known_peers.get(index))
            .copied()
    }
}

fn error_status(error: &kryptopass_peers::PeerError) -> &'static str {
    use kryptopass_peers::PeerError;
    match error {
        PeerError::Cancelled => "Operación cancelada.",
        PeerError::Expired => "La invitación o conexión caducó. Genera otro código.",
        PeerError::Rejected => "Emparejamiento rechazado. No se concedió confianza.",
        PeerError::KeyStore => {
            "No se pudo acceder al llavero del sistema para abrir la identidad de este equipo."
        }
        PeerError::Profile => "No se pudieron cargar los datos de vinculación de este equipo.",
        PeerError::Session => {
            "La confianza se guardó, pero no se pudo abrir la sesión activa. Puedes reconectar."
        }
        PeerError::Link => {
            "El teléfono no completó la conexión de red. Comprueba su error y genera otra invitación; todavía no se verificaron identidades."
        }
        PeerError::Authentication => {
            "La verificación criptográfica no se completó. No se autorizó el dispositivo. Genera una invitación nueva."
        }
        PeerError::NetworkPrivacyBlocked(NetworkPolicyError::VpnUnverifiable) => {
            "No se puede comprobar si la conexión a Internet pasa por una VPN en este sistema, así que quedó en pausa."
        }
        PeerError::NetworkPrivacyBlocked(NetworkPolicyError::VpnInactive) => {
            "Elegiste usar Internet solo a través de VPN y ahora no hay una VPN activa. Conéctala para continuar."
        }
        PeerError::NetworkPrivacyBlocked(NetworkPolicyError::LanExceptionRequired) => {
            "La red local está desactivada mientras se exige VPN. Actívala en Privacidad si la necesitas."
        }
        PeerError::NetworkPrivacyBlocked(NetworkPolicyError::RelayRequired) => {
            "Ocultar tu dirección IP desactiva la red local: todo pasa por el servidor de retransmisión."
        }
        PeerError::NetworkPrivacyBlocked(NetworkPolicyError::PolicyChanged) => {
            "La política de privacidad cambió. La operación se cerró sin reutilizar su ruta anterior."
        }
        PeerError::NetworkPrivacyBlocked(NetworkPolicyError::RouteChanged) => {
            "La ruta de red cambió. La operación se cerró y no se reintentó por otra interfaz."
        }
        PeerError::NetworkPrivacyBlocked(NetworkPolicyError::RouteSnapshotUnavailable) => {
            "No se pudo verificar la ruta del sistema. La conexión quedó bloqueada."
        }
    }
}

fn error_code(error: &kryptopass_peers::PeerError) -> &'static str {
    use kryptopass_peers::PeerError;
    match error {
        PeerError::Cancelled => "cancelled",
        PeerError::Expired => "expired",
        PeerError::Rejected => "rejected",
        PeerError::Profile => "profile_error",
        PeerError::KeyStore => "keystore_error",
        PeerError::Authentication => "authentication_error",
        PeerError::Link | PeerError::Session => "transport_error",
        PeerError::NetworkPrivacyBlocked(_) => "network_policy_blocked",
    }
}

fn connection_code(state: ConnectionState) -> &'static str {
    match state {
        ConnectionState::Connecting => "connecting",
        ConnectionState::Reconnecting => "reconnecting",
        ConnectionState::Connected => "connected",
        ConnectionState::Disconnected => "disconnected",
        ConnectionState::Failed => "transport_error",
    }
}

fn parse_device_id(value: &str) -> Option<DeviceId> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    bytes
        .iter()
        .any(|byte| *byte != 0)
        .then_some(DeviceId::from_bytes(bytes))
}

fn connection_status(state: ConnectionState) -> &'static str {
    match state {
        ConnectionState::Connecting => "Confianza guardada. Abriendo la sesión autenticada…",
        ConnectionState::Reconnecting => {
            "Esperando recuperar la conexión con el dispositivo autorizado…"
        }
        ConnectionState::Connected => "Dispositivo conectado. Sesión autenticada activa.",
        ConnectionState::Disconnected => {
            "Dispositivo desconectado. La confianza guardada se conserva."
        }
        ConnectionState::Failed => {
            "La sesión se interrumpió. La confianza guardada se conserva; puedes reconectar."
        }
    }
}

macro_rules! notifying_property {
    ($setter:ident, $field:ident, $changed:ident, $ty:ty) => {
        impl qobject::PeersController {
            fn $setter(mut self: Pin<&mut Self>, value: $ty) {
                if self.rust().$field != value {
                    self.as_mut().rust_mut().$field = value;
                    self.$changed();
                }
            }
        }
    };
}
notifying_property!(set_busy, busy, busy_changed, bool);
notifying_property!(
    set_inbound_key_transfer,
    inbound_key_transfer,
    inbound_key_transfer_changed,
    QString
);
notifying_property!(set_lan_enabled, lan_enabled, lan_enabled_changed, bool);
notifying_property!(
    set_internet_enabled,
    internet_enabled,
    internet_enabled_changed,
    bool
);
notifying_property!(set_require_vpn, require_vpn, require_vpn_changed, bool);
notifying_property!(
    set_allow_lan_exception,
    allow_lan_exception,
    allow_lan_exception_changed,
    bool
);
notifying_property!(set_relay_only, relay_only, relay_only_changed, bool);
notifying_property!(set_vpn_status, vpn_status, vpn_status_changed, i32);
notifying_property!(
    set_relay_origin,
    relay_origin,
    relay_origin_changed,
    QString
);
notifying_property!(
    set_network_addresses,
    network_addresses,
    network_addresses_changed,
    QStringList
);
notifying_property!(set_network_index, network_index, network_index_changed, i32);
notifying_property!(set_device_ids, device_ids, device_ids_changed, QStringList);
notifying_property!(
    set_selected_index,
    selected_index,
    selected_index_changed,
    i32
);
notifying_property!(set_awaiting, awaiting, awaiting_changed, bool);
notifying_property!(
    set_can_reconnect,
    can_reconnect,
    can_reconnect_changed,
    bool
);
notifying_property!(set_status, status, status_changed, QString);
notifying_property!(
    set_bootstrap_qr,
    bootstrap_qr,
    bootstrap_qr_changed,
    QString
);
notifying_property!(set_sas_emojis, sas_emojis, sas_emojis_changed, QString);
notifying_property!(set_status_code, status_code, status_code_changed, QString);
notifying_property!(
    set_session_active,
    session_active,
    session_active_changed,
    bool
);
notifying_property!(
    set_active_device_id,
    active_device_id,
    active_device_id_changed,
    QString
);
notifying_property!(set_alias_busy, alias_busy, alias_busy_changed, bool);
notifying_property!(set_alias_error, alias_error, alias_error_changed, QString);
notifying_property!(set_sas, sas, sas_changed, QString);
notifying_property!(set_peer, peer, peer_changed, QString);
notifying_property!(
    set_devices_snapshot,
    devices_snapshot,
    devices_snapshot_changed,
    QString
);

fn hex_transfer_id(id: &[u8; 16]) -> String {
    let mut result = String::with_capacity(32);
    for byte in id {
        use std::fmt::Write;
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn peers_directory() -> Result<PathBuf, ()> {
    crate::vault_access::profile_directory().map_err(|_| ())
}

fn hex_bytes(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(text.get(index..index + 2)?, 16).ok())
        .collect()
}

notifying_property!(
    set_maintenance_busy,
    maintenance_busy,
    maintenance_busy_changed,
    bool
);
notifying_property!(
    set_sync_peer_id,
    sync_peer_id,
    sync_peer_id_changed,
    QString
);
notifying_property!(set_sync_status, sync_status, sync_status_changed, QString);
notifying_property!(set_sync_error, sync_error, sync_error_changed, QString);
notifying_property!(
    set_sync_local_revision_count,
    sync_local_revision_count,
    sync_local_revision_count_changed,
    i32
);
notifying_property!(
    set_sync_entries,
    sync_entries,
    sync_entries_changed,
    QStringList
);
notifying_property!(
    set_sync_configured,
    sync_configured,
    sync_configured_changed,
    bool
);
notifying_property!(
    set_synthetic_transfer_fingerprint,
    synthetic_transfer_fingerprint,
    synthetic_transfer_fingerprint_changed,
    QString
);
notifying_property!(
    set_vault_sync_activity,
    vault_sync_activity,
    vault_sync_activity_changed,
    i32
);
notifying_property!(
    set_vault_mirror_active,
    vault_mirror_active,
    vault_mirror_active_changed,
    bool
);
notifying_property!(
    set_vault_sync_locked_key,
    vault_sync_locked_key,
    vault_sync_locked_key_changed,
    QString
);
notifying_property!(
    set_vault_sync_keys,
    vault_sync_keys,
    vault_sync_keys_changed,
    QString
);
notifying_property!(
    set_vault_sync_peers,
    vault_sync_peers,
    vault_sync_peers_changed,
    QStringList
);
notifying_property!(
    set_vault_sync_message,
    vault_sync_message,
    vault_sync_message_changed,
    QString
);
notifying_property!(
    set_vault_key_fingerprint,
    vault_key_fingerprint,
    vault_key_fingerprint_changed,
    QString
);
notifying_property!(
    set_synthetic_transfer_preparing,
    synthetic_transfer_preparing,
    synthetic_transfer_preparing_changed,
    bool
);
notifying_property!(
    set_synthetic_transfer_error,
    synthetic_transfer_error,
    synthetic_transfer_error_changed,
    QString
);

notifying_property!(
    set_vault_sync_failed,
    vault_sync_failed,
    vault_sync_failed_changed,
    bool
);
