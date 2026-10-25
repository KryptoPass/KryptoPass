//! Explicit emulator pairing UI. Private identities remain in the worker/OS store.

use super::synthetic_sync::{SyntheticSyncAction, SyntheticSyncError, SyntheticSyncSnapshot};
use crate::vault_sync::mirror::{self, MirrorError};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use kryptopass_devices::{
    ConnectionState, DeviceDirectory, DeviceDirectorySnapshot, KeyTransferPurpose,
    SessionCloseReason, SessionEvent, SessionFailure, SessionOptions,
};
use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_lan::{
    NetworkPolicyError, NetworkPolicyGuard, NetworkPrivacyPolicy, VpnMode, VpnStatus,
};
use kryptopass_pairing::LocalDecision;
use kryptopass_pairing_lab::{
    HostEvent, LabHost, LabNetwork, NetworkPrivacySettings, ResumeOwnerSnapshot,
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
        #[qproperty(bool, available, READ, NOTIFY)]
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
        #[qproperty(
            QString,
            vault_sync_message,
            cxx_name = "vaultSyncMessage",
            READ,
            NOTIFY
        )]
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
        type PairingLabController = super::PairingLabControllerRust;

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
        #[cxx_name = "activateSyntheticSync"]
        fn activate_synthetic_sync(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "refreshSyntheticSync"]
        fn refresh_synthetic_sync(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "createSyntheticCredential"]
        fn create_synthetic_credential(self: Pin<&mut Self>);
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
        #[cxx_name = "enableVaultSync"]
        fn enable_vault_sync(self: Pin<&mut Self>, device_id: QString);
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
    impl cxx_qt::Threading for PairingLabController {}
}

pub struct PairingLabControllerRust {
    available: bool,
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
    vault_sync_message: QString,
    vault_mirror_running: bool,
    synthetic_transfer_preparing: bool,
    synthetic_transfer_error: QString,
    synthetic_transfer_progress:
        BTreeMap<DeviceId, (u64, kryptopass_devices::SyntheticTransferProgress)>,
    synthetic_transfer_cancel: Option<watch::Sender<bool>>,
    synthetic_transfer_generation: u64,
    synthetic_transfer_preparing_peer: Option<(DeviceId, u64)>,
    synthetic_transfer_target: Option<(DeviceId, u64, [u8; 16])>,
    inbound_key_transfer: QString,
    inbound_transfer: Option<InboundTransferUi>,
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
    sync_generation: u64,
}

impl Default for PairingLabControllerRust {
    fn default() -> Self {
        Self {
            available: cfg!(debug_assertions),
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
            vault_sync_message: QString::default(),
            vault_mirror_running: false,
            synthetic_transfer_preparing: false,
            synthetic_transfer_error: QString::default(),
            synthetic_transfer_progress: BTreeMap::new(),
            synthetic_transfer_cancel: None,
            synthetic_transfer_generation: 0,
            synthetic_transfer_preparing_peer: None,
            synthetic_transfer_target: None,
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
            sync_generation: 0,
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

/// Cancels a start that reached the session owner but could not be committed
/// into the Qt controller state. The guard remains armed while queued work is
/// waiting, so navigation or a dropped Qt callback cannot leave a hidden send.
struct SyntheticTransferCancelOnDrop {
    owner: ResumeProcessOwner,
    peer: DeviceId,
    generation: u64,
    transfer_id: [u8; 16],
    armed: bool,
}

impl SyntheticTransferCancelOnDrop {
    fn new(
        owner: ResumeProcessOwner,
        peer: DeviceId,
        generation: u64,
        transfer_id: [u8; 16],
    ) -> Self {
        Self {
            owner,
            peer,
            generation,
            transfer_id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SyntheticTransferCancelOnDrop {
    fn drop(&mut self) {
        if self.armed {
            spawn_synthetic_transfer_cancel(
                self.owner.clone(),
                self.peer,
                self.generation,
                self.transfer_id,
            );
        }
    }
}

fn spawn_synthetic_transfer_cancel(
    owner: ResumeProcessOwner,
    peer: DeviceId,
    generation: u64,
    transfer_id: [u8; 16],
) {
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        runtime.block_on(async move {
            let _ = owner
                .cancel_synthetic_key_transfer(peer, generation, transfer_id)
                .await;
        });
    });
}

impl Drop for PendingOperation {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
    }
}

impl qobject::PairingLabController {
    /// Starts the process-owned durable listener independently of the dialog.
    /// Opening a missing profile is intentionally left to explicit pairing.
    pub fn start_resume_owner(mut self: Pin<&mut Self>) {
        if !cfg!(debug_assertions) || self.rust().resume_owner.is_some() {
            return;
        }
        let directory = match lab_directory() {
            Ok(directory) => directory,
            Err(()) => return,
        };
        let owner = match spawn_resume_process_owner(directory.clone()) {
            Ok(owner) => owner,
            Err(_) => return,
        };
        let (cancel, mut cancelled) = watch::channel(false);
        self.as_mut().rust_mut().resume_owner_generation =
            self.rust().resume_owner_generation.wrapping_add(1);
        let owner_generation = self.rust().resume_owner_generation;
        self.as_mut().rust_mut().resume_owner_cancel = Some(cancel);
        self.as_mut().rust_mut().resume_owner = Some(owner.clone());
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async move {
                let mut updates = owner.subscribe();
                if matches!(LabHost::system_profile_needs_pairing(&directory), Ok(false)) {
                    let _ = owner.ensure_ready().await;
                }
                let initial = updates.borrow().clone();
                if initial.ready.is_some() || initial.profile_error {
                    let _ = thread.queue(move |mut object| {
                        object
                            .as_mut()
                            .apply_resume_owner_snapshot(owner_generation, initial);
                    });
                }
                loop {
                    tokio::select! {
                        changed = updates.changed() => {
                            if changed.is_err() { break; }
                            let snapshot = updates.borrow_and_update().clone();
                            let _ = thread.queue(move |mut object| {
                                object
                                    .as_mut()
                                    .apply_resume_owner_snapshot(owner_generation, snapshot);
                            });
                        }
                        changed = cancelled.changed() => {
                            if changed.is_err() || *cancelled.borrow() { break; }
                        }
                    }
                }
            });
        });
    }

    /// Stops the process listener. Profile maintenance uses the blocking drain
    /// API directly so trust cannot be changed while an inbound owner is live.
    pub fn shutdown_resume_owner(mut self: Pin<&mut Self>) {
        self.as_mut().stop_resume_owner();
    }

    fn stop_resume_owner(mut self: Pin<&mut Self>) -> Option<ResumeProcessOwner> {
        self.as_mut().cancel_synthetic_key_transfer_operation();
        self.as_mut().rust_mut().resume_owner_generation =
            self.rust().resume_owner_generation.wrapping_add(1);
        self.as_mut().rust_mut().devices_revision = 0;
        if let Some(cancel) = self.as_mut().rust_mut().resume_owner_cancel.take() {
            let _ = cancel.send(true);
        }
        let owner = self.as_mut().rust_mut().resume_owner.take();
        if let Some(owner) = owner.as_ref() {
            owner.shutdown();
        }
        owner
    }

    fn apply_resume_owner_snapshot(
        mut self: Pin<&mut Self>,
        owner_generation: u64,
        state: ResumeOwnerSnapshot,
    ) {
        if self.rust().resume_owner_generation != owner_generation {
            return;
        }
        // Log each phase change once; progress carries phases and versions, never key bytes.
        for (peer, (generation, progress)) in &state.synthetic_transfers {
            let previous = self
                .rust()
                .synthetic_transfer_progress
                .get(peer)
                .map(|(previous_generation, previous)| (*previous_generation, previous.phase));
            if previous != Some((*generation, progress.phase)) {
                tracing::info!(phase = ?progress.phase, version = progress.version, "Key transfer: phase changed");
            }
        }
        self.as_mut().rust_mut().synthetic_transfer_progress = state.synthetic_transfers.clone();
        self.as_mut()
            .track_inbound_key_transfer(&state.synthetic_transfers);
        if let Some(ready) = state.ready {
            let preferred = parse_device_id(&self.rust().active_device_id.to_string())
                .or_else(|| self.rust().selected_peer().map(|peer| peer.device_id));
            self.as_mut().rust_mut().devices = Some(ready.devices.clone());
            self.as_mut()
                .replace_trusted_devices(ready.trusted_peers, preferred);
            let snapshot = state
                .devices
                .as_deref()
                .cloned()
                .unwrap_or_else(|| ready.devices.snapshot().as_ref().clone());
            if !self.rust().busy && self.rust().active.is_none() {
                let connected = snapshot
                    .devices
                    .iter()
                    .find(|device| device.connection == ConnectionState::Connected)
                    .map(|device| device.identity.device_id);
                self.as_mut().set_session_active(connected.is_some());
                self.as_mut().set_active_device_id(
                    connected.map_or_else(QString::default, |peer| QString::from(peer.to_string())),
                );
                if connected.is_some() {
                    self.as_mut().set_status_code(QString::from("connected"));
                    self.as_mut()
                        .set_status(QString::from(connection_status(ConnectionState::Connected)));
                } else if !self.rust().maintenance_busy
                    && self.rust().status_code != QString::from("waiting_peer")
                    && self.rust().status_code != QString::from("resume_ready")
                {
                    self.as_mut().set_status_code(QString::from("idle"));
                }
            }
            self.as_mut().apply_devices_snapshot(&snapshot);
        } else if state.profile_error && !self.rust().busy && !self.rust().maintenance_busy {
            self.as_mut()
                .set_status_code(QString::from("profile_error"));
            self.as_mut().set_status(QString::from(
                "No se pudieron cargar los dispositivos guardados.",
            ));
        }
    }

    pub fn load_network_addresses(mut self: Pin<&mut Self>) {
        if !cfg!(debug_assertions) || *self.busy() {
            return;
        }
        let settings = match lab_directory()
            .map_err(|_| ())
            .and_then(|directory| load_network_privacy_settings(&directory).map_err(|_| ()))
        {
            Ok(settings) => settings,
            Err(()) => {
                self.as_mut()
                    .set_status_code(QString::from("network_policy_blocked"));
                self.as_mut().set_status(QString::from(
                    "No se pudo leer la política de privacidad de red. Las conexiones de recuperación quedan bloqueadas hasta reparar la configuración.",
                ));
                return;
            }
        };
        let loaded_policy_changed = settings.policy
            != (NetworkPrivacyPolicy {
                vpn_mode: if self.rust().require_vpn {
                    VpnMode::RequireVpn
                } else {
                    VpnMode::FollowSystem
                },
                allow_lan_exception: self.rust().allow_lan_exception,
                relay_only: self.rust().relay_only,
            });
        if loaded_policy_changed {
            if let Some(guard) = self.as_mut().rust_mut().active_network_guard.as_ref() {
                guard.invalidate();
            }
            if let Some(owner) = self.rust().resume_owner.as_ref() {
                owner.refresh_network_policy();
            }
        }
        self.as_mut()
            .set_require_vpn(settings.policy.vpn_mode == VpnMode::RequireVpn);
        self.as_mut()
            .set_allow_lan_exception(settings.policy.allow_lan_exception);
        self.as_mut().set_relay_only(settings.policy.relay_only);
        self.as_mut()
            .set_vpn_status(vpn_status_code(settings.policy));
        self.as_mut().align_pairing_method_with_policy();
        let previous = usize::try_from(self.rust().network_index)
            .ok()
            .and_then(|index| self.rust().network_ips.get(index))
            .copied();
        let interfaces = kryptopass_lan::local_interfaces().unwrap_or_default();
        let addresses: Vec<_> = interfaces
            .iter()
            .map(|interface| interface.address)
            .collect();
        let selected = kryptopass_lan::select_local_bind_address(
            settings.lan_bind_ip,
            &addresses,
            kryptopass_lan::os_selected_local_address(),
        )
        .or_else(|| previous.filter(|address| addresses.contains(address)));
        let index = selected.and_then(|selected| addresses.iter().position(|ip| *ip == selected));
        let display = interfaces
            .iter()
            .map(|interface| {
                use kryptopass_lan::InterfaceKind;
                let label = match interface.kind {
                    InterfaceKind::Wifi => "Wi-Fi",
                    InterfaceKind::Ethernet => "Ethernet",
                    InterfaceKind::Virtual => "VPN / virtual",
                    InterfaceKind::Loopback => "Local",
                    InterfaceKind::Other => "Red",
                };
                QString::from(format!(
                    "{label} · {} — {}",
                    interface.name, interface.address
                ))
            })
            .collect();
        self.as_mut().rust_mut().network_ips = addresses;
        self.as_mut().set_network_addresses(display);
        self.as_mut().set_network_index(
            index
                .and_then(|index| i32::try_from(index).ok())
                .unwrap_or(-1),
        );
    }

    /// Internet is an explicit carrier choice; it never changes device authorization.
    pub fn configure_internet(mut self: Pin<&mut Self>, enabled: bool, relay_origin: QString) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
        if *self.busy() || self.rust().maintenance_busy {
            return;
        }
        if enabled {
            let origin = relay_origin.to_string();
            if kryptopass_lan::RelayRoute::new(origin.trim(), [1; 32]).is_err() {
                self.as_mut()
                    .set_status_code(QString::from("network_error"));
                self.as_mut().set_status(QString::from(
                    "Introduce un servidor WSS válido, sin credenciales ni parámetros.",
                ));
                return;
            }
            self.as_mut().set_relay_origin(QString::from(origin.trim()));
        }
        self.as_mut().set_internet_enabled(enabled);
    }

    pub fn configure_network(mut self: Pin<&mut Self>, enabled: bool, index: i32) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
        if *self.busy() {
            return;
        }
        if !enabled {
            self.as_mut().set_lan_enabled(false);
            return;
        }
        // The UI disables this choice too; the backend still refuses it so a
        // stale binding can never enable LAN against the saved privacy policy.
        if !lan_permitted(
            self.rust().require_vpn,
            self.rust().allow_lan_exception,
            self.rust().relay_only,
        ) {
            self.as_mut()
                .set_status_code(QString::from("network_policy_blocked"));
            self.as_mut().set_status(QString::from(
                "La red local no está disponible con la privacidad elegida.",
            ));
            return;
        }
        if usize::try_from(index)
            .ok()
            .is_some_and(|index| index < self.rust().network_ips.len())
        {
            let Some(bind_ip) = usize::try_from(index)
                .ok()
                .and_then(|index| self.rust().network_ips.get(index))
                .copied()
            else {
                return;
            };
            if let Err(()) = persist_network_settings(Some(bind_ip)) {
                self.as_mut()
                    .set_status_code(QString::from("network_policy_blocked"));
                self.as_mut().set_status(QString::from(
                    "No se pudo guardar la dirección LAN seleccionada. No se iniciará una conexión con una política sin guardar.",
                ));
                return;
            }
            self.as_mut().set_network_index(index);
            self.as_mut().set_lan_enabled(true);
            if let Some(owner) = self.rust().resume_owner.as_ref() {
                owner.refresh_network_policy();
            }
        } else {
            self.as_mut().set_status(QString::from("No hay una dirección LAN seleccionada. Actualiza las interfaces o usa el modo emulador."));
        }
    }

    pub fn configure_privacy(
        mut self: Pin<&mut Self>,
        require_vpn: bool,
        allow_lan_exception: bool,
        relay_only: bool,
    ) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
        if !cfg!(debug_assertions) || self.rust().maintenance_busy {
            return;
        }
        // Changing privacy settings must not pin an automatically displayed
        // route address. Only the explicit LAN selection saves such a preference.
        let bind_ip = lab_directory()
            .ok()
            .and_then(|directory| load_network_privacy_settings(&directory).ok())
            .and_then(|settings| settings.lan_bind_ip);
        // Save only meaningful combinations: the LAN exception exists only
        // while VPN is required, and relay-only excludes every LAN path.
        let allow_lan_exception = require_vpn && allow_lan_exception && !relay_only;
        let policy = NetworkPrivacyPolicy {
            vpn_mode: if require_vpn {
                VpnMode::RequireVpn
            } else {
                VpnMode::FollowSystem
            },
            allow_lan_exception,
            relay_only,
        };
        let directory = match lab_directory() {
            Ok(directory) => directory,
            Err(()) => {
                self.as_mut()
                    .set_status_code(QString::from("network_policy_blocked"));
                self.as_mut().set_status(QString::from(
                    "No se pudo localizar el archivo de política de red. No se aplicaron cambios.",
                ));
                return;
            }
        };
        if save_network_privacy_settings(
            &directory,
            NetworkPrivacySettings {
                policy,
                lan_bind_ip: bind_ip,
            },
        )
        .is_err()
        {
            self.as_mut()
                .set_status_code(QString::from("network_policy_blocked"));
            self.as_mut().set_status(QString::from(
                "No se pudo guardar la política de red. No se aplicaron cambios.",
            ));
            return;
        }

        // Invalidate the guard shared by the pending pairing and its active
        // WebRTC transport. Its monitor closes locally without a route fallback.
        if let Some(guard) = self.as_mut().rust_mut().active_network_guard.take() {
            guard.invalidate();
        }
        self.as_mut().set_require_vpn(require_vpn);
        self.as_mut().set_allow_lan_exception(allow_lan_exception);
        self.as_mut().set_relay_only(relay_only);
        self.as_mut().set_vpn_status(vpn_status_code(policy));
        self.as_mut().align_pairing_method_with_policy();
        if let Some(owner) = self.rust().resume_owner.as_ref() {
            owner.refresh_network_policy();
        }
        if self.rust().busy || self.rust().session_active {
            self.as_mut()
                .set_status_code(QString::from("network_policy_changed"));
            self.as_mut().set_status(QString::from(
                "La política de red cambió. Se está cerrando la operación y no se reutilizará su ruta anterior.",
            ));
        }
    }

    pub fn load_devices(mut self: Pin<&mut Self>) {
        if self
            .rust()
            .draining_worker
            .as_ref()
            .is_some_and(|done| !done.load(Ordering::Acquire))
        {
            return;
        }
        if !cfg!(debug_assertions) || *self.busy() || self.rust().alias_busy {
            return;
        }
        self.as_mut().start_resume_owner();
        if self.rust().active.is_some() {
            if let Some(devices) = self.rust().devices.clone() {
                self.as_mut().apply_devices_snapshot(&devices.snapshot());
            }
            return;
        }
        let preferred = self.rust().selected_peer().map(|peer| peer.device_id);
        self.as_mut().replace_trusted_devices(Vec::new(), None);
        self.as_mut().rust_mut().devices = None;
        self.as_mut()
            .set_devices_snapshot(QString::from("{\"revision\":0,\"devices\":[]}"));
        let directory = match lab_directory() {
            Ok(path) => path,
            Err(()) => {
                self.as_mut().set_status(QString::from(
                    "No se pudieron localizar los datos de vinculación.",
                ));
                return;
            }
        };
        let Some(owner) = self.rust().resume_owner.clone() else {
            self.as_mut()
                .set_status_code(QString::from("profile_error"));
            self.as_mut().set_status(QString::from(
                "No se pudo iniciar el listener de recuperación local.",
            ));
            return;
        };
        let (cancel, cancelled) = watch::channel(false);
        let completed = Arc::new(AtomicBool::new(false));
        let worker_completed = completed.clone();
        self.as_mut().rust_mut().generation = self.rust().generation.wrapping_add(1);
        let generation = self.rust().generation;
        self.as_mut().rust_mut().pending = Some(PendingOperation {
            cancel,
            decision: None,
            completed,
        });
        self.as_mut().set_busy(true);
        self.as_mut().set_status_code(QString::from("loading"));
        self.as_mut()
            .set_status(QString::from("Cargando dispositivos vinculados…"));
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let _completed = WorkerCompletion(worker_completed);
            let transfer_fingerprint = super::synthetic_key_transfer::fingerprint();
            let vault_fingerprint = super::synthetic_key_transfer::vault_identity_fingerprint();
            let result = (|| {
                if *cancelled.borrow() {
                    return Err(kryptopass_pairing_lab::LabError::Cancelled);
                }
                if LabHost::system_profile_needs_pairing(&directory)? {
                    return Ok(None);
                }
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| kryptopass_pairing_lab::LabError::Link)?;
                let ready = runtime.block_on(owner.ensure_ready())?;
                let host = LabHost::open_existing_system_profile(&directory)?;
                let aliases = host.device_aliases();
                if *cancelled.borrow() {
                    return Err(kryptopass_pairing_lab::LabError::Cancelled);
                }
                Ok(Some((ready, aliases)))
            })();
            let _ = thread.queue(move |mut object| {
                if object.rust().generation != generation { return; }
                object.as_mut().rust_mut().pending = None;
                object.as_mut().set_busy(false);
                object.as_mut().set_synthetic_transfer_fingerprint(
                    QString::from(transfer_fingerprint),
                );
                object
                    .as_mut()
                    .set_vault_key_fingerprint(QString::from(vault_fingerprint));
                object.as_mut().refresh_vault_sync_peers();
                match result {
                    Ok(loaded) => {
                        let (ready, aliases) = loaded.map_or_else(|| (None, Ok(BTreeMap::new())), |(ready, aliases)| (Some(ready), aliases));
                        match aliases {
                            Ok(aliases) => { object.as_mut().rust_mut().aliases = aliases; object.as_mut().set_alias_error(QString::default()); },
                            Err(_) => { object.as_mut().rust_mut().aliases.clear(); object.as_mut().set_alias_error(QString::from("No se pudieron cargar los nombres locales. La confianza se conserva.")); },
                        }
                        let empty = ready.as_ref().is_none_or(|ready| ready.trusted_peers.is_empty());
                        if let Some(ready) = ready {
                            object.as_mut().replace_trusted_devices(ready.trusted_peers, preferred);
                            object.as_mut().rust_mut().devices = Some(ready.devices.clone());
                        } else {
                            object.as_mut().replace_trusted_devices(Vec::new(), preferred);
                            object.as_mut().rust_mut().devices = None;
                        }
                        object.as_mut().set_status_code(QString::from("idle"));
                        if let Some(devices) = object.rust().devices.clone() {
                            object.as_mut().apply_devices_snapshot(&devices.snapshot());
                        }
                        object.as_mut().set_status(QString::from(if empty {
                            "No hay dispositivos autorizados. Crea una invitación para vincular Android."
                        } else {
                            "Dispositivos recuperados. Selecciona uno y pulsa Reconectar; no necesitas repetir los emojis."
                        }));
                    }
                    Err(error) => {
                        object.as_mut().set_status_code(QString::from(error_code(&error)));
                        object.as_mut().set_status(QString::from(error_status(&error)));
                    },
                }
            });
        });
    }

    pub fn select_device(mut self: Pin<&mut Self>, index: i32) {
        if *self.busy() || self.rust().sync_busy {
            return;
        }
        let selected = usize::try_from(index)
            .ok()
            .and_then(|index| self.rust().known_peers.get(index))
            .copied();
        let selection_moved_during_transfer = self
            .rust()
            .synthetic_transfer_target
            .is_some_and(|(peer, _, _)| selected.is_none_or(|selected| selected.device_id != peer))
            || self
                .rust()
                .synthetic_transfer_preparing_peer
                .is_some_and(|(peer, _)| {
                    selected.is_none_or(|selected| selected.device_id != peer)
                });
        if selection_moved_during_transfer {
            self.as_mut().cancel_synthetic_key_transfer_operation();
        }
        self.as_mut()
            .set_selected_index(if selected.is_some() { index } else { -1 });
        self.as_mut().set_can_reconnect(selected.is_some());
        self.as_mut()
            .set_peer(selected.map_or_else(QString::default, |peer| {
                QString::from(peer.device_id.to_string())
            }));
        let sync_peer_id = selected.map_or_else(QString::default, |peer| {
            QString::from(peer.device_id.to_string())
        });
        if self.rust().sync_peer_id != sync_peer_id {
            self.as_mut().set_sync_peer_id(sync_peer_id);
            self.as_mut().set_sync_configured(false);
            self.as_mut().set_sync_status(QString::default());
            self.as_mut().set_sync_error(QString::default());
            self.as_mut().set_sync_local_revision_count(0);
            self.as_mut().set_sync_entries(QStringList::default());
        }
    }

    pub fn activate_synthetic_sync(mut self: Pin<&mut Self>) {
        self.as_mut()
            .run_synthetic_sync(SyntheticSyncAction::Activate);
    }

    pub fn refresh_synthetic_sync(mut self: Pin<&mut Self>) {
        self.as_mut()
            .run_synthetic_sync(SyntheticSyncAction::Refresh);
    }

    pub fn create_synthetic_credential(mut self: Pin<&mut Self>) {
        self.as_mut()
            .run_synthetic_sync(SyntheticSyncAction::CreateCredential);
    }

    /// Local confirmation callback for the explicitly synthetic fixture.
    /// GnuPG preparation starts only after this invocation and stays on a worker.
    pub fn start_synthetic_key_transfer(
        self: Pin<&mut Self>,
        device_id: QString,
        connection_generation: QString,
        approved_fingerprint: QString,
    ) {
        self.start_key_transfer(
            KeyTransferPurpose::SyntheticFixture,
            &device_id,
            &connection_generation,
            approved_fingerprint,
        );
    }

    /// Local confirmation callback for sending the vault identity to the phone.
    /// GnuPG asks for the key passphrase on the worker before anything is sent.
    pub fn start_vault_key_transfer(
        self: Pin<&mut Self>,
        device_id: QString,
        connection_generation: QString,
        approved_fingerprint: QString,
    ) {
        self.start_key_transfer(
            KeyTransferPurpose::VaultIdentity,
            &device_id,
            &connection_generation,
            approved_fingerprint,
        );
    }

    fn start_key_transfer(
        mut self: Pin<&mut Self>,
        purpose: KeyTransferPurpose,
        device_id: &QString,
        connection_generation: &QString,
        approved_fingerprint: QString,
    ) {
        if !cfg!(debug_assertions) {
            self.as_mut().set_synthetic_transfer_error(QString::from(
                "La transferencia sintética solo está disponible en Desktop debug.",
            ));
            return;
        }
        if self.rust().synthetic_transfer_preparing
            || self.rust().sync_busy
            || self.rust().busy
            || self.rust().maintenance_busy
        {
            return;
        }
        let Some(peer) = parse_device_id(&device_id.to_string()) else {
            self.as_mut().set_synthetic_transfer_error(QString::from(
                "Selecciona un teléfono autorizado y conectado.",
            ));
            return;
        };
        let Some(expected_generation) = connection_generation.to_string().parse::<u64>().ok()
        else {
            self.as_mut().set_synthetic_transfer_error(QString::from(
                "La sesión cambió. Vuelve a abrir la confirmación.",
            ));
            return;
        };
        let approved_fingerprint = approved_fingerprint.to_string();
        let (offered_fingerprint, minimum_version) = match purpose {
            KeyTransferPurpose::SyntheticFixture => {
                (self.rust().synthetic_transfer_fingerprint.to_string(), 1)
            }
            KeyTransferPurpose::VaultIdentity => (self.rust().vault_key_fingerprint.to_string(), 2),
        };
        if approved_fingerprint.is_empty() || approved_fingerprint != offered_fingerprint {
            self.as_mut().set_synthetic_transfer_error(QString::from(
                "La identidad cambió después de mostrar la confirmación. Vuelve a revisar el fingerprint.",
            ));
            return;
        }
        let connected = self.rust().devices.as_ref().is_some_and(|devices| {
            devices.snapshot().get(&peer).is_some_and(|device| {
                device.connection == ConnectionState::Connected
                    && device.connection_generation == expected_generation
            })
        });
        let ready = self
            .rust()
            .synthetic_transfer_progress
            .get(&peer)
            .is_some_and(|(generation, progress)| {
                *generation == expected_generation
                    && progress.phase == kryptopass_devices::SyntheticTransferPhase::Ready
                    && progress.version >= minimum_version
            });
        if !connected || !ready {
            self.as_mut().set_synthetic_transfer_error(QString::from(
                "El teléfono ya no está conectado o no admite recibir esta clave.",
            ));
            return;
        }
        let Some(owner) = self.rust().resume_owner.clone() else {
            self.as_mut().set_synthetic_transfer_error(QString::from(
                "No se pudo acceder al dueño de la sesión activa.",
            ));
            return;
        };

        if let Some(cancel) = self.as_mut().rust_mut().synthetic_transfer_cancel.take() {
            let _ = cancel.send(true);
        }
        let (cancel, cancelled) = watch::channel(false);
        self.as_mut().rust_mut().synthetic_transfer_cancel = Some(cancel);
        self.as_mut().rust_mut().synthetic_transfer_generation =
            self.rust().synthetic_transfer_generation.wrapping_add(1);
        let operation_generation = self.rust().synthetic_transfer_generation;
        self.as_mut().rust_mut().synthetic_transfer_preparing_peer =
            Some((peer, expected_generation));
        self.as_mut().set_synthetic_transfer_preparing(true);
        self.as_mut()
            .set_synthetic_transfer_error(QString::default());

        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = (|| {
                if *cancelled.borrow() {
                    return Ok(None);
                }
                tracing::info!(purpose = ?purpose, "Key transfer: preparing export");
                let (transfer_id, prepared) = match purpose {
                    KeyTransferPurpose::SyntheticFixture => {
                        super::synthetic_key_transfer::prepare(&approved_fingerprint)?
                    }
                    KeyTransferPurpose::VaultIdentity => {
                        super::synthetic_key_transfer::prepare_vault_identity(
                            &approved_fingerprint,
                        )?
                    }
                };
                tracing::info!(purpose = ?purpose, "Key transfer: export ready, sending offer");
                if *cancelled.borrow() {
                    return Ok(None);
                }
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| kryptopass_devices::SyntheticTransferError::WrongSession)?;
                runtime.block_on(async {
                    owner
                        .start_synthetic_key_transfer(peer, expected_generation, prepared)
                        .await?;
                    let cancel_guard = SyntheticTransferCancelOnDrop::new(
                        owner.clone(),
                        peer,
                        expected_generation,
                        transfer_id,
                    );
                    if *cancelled.borrow() {
                        drop(cancel_guard);
                        return Ok(None);
                    }
                    Ok(Some((transfer_id, cancel_guard)))
                })
            })();
            let _ = thread.queue(move |mut object| {
                if object.rust().synthetic_transfer_generation != operation_generation {
                    tracing::info!(purpose = ?purpose, "Key transfer: result superseded by a newer operation");
                    return;
                }
                let exact_session_current = object.rust().devices.as_ref().is_some_and(|devices| {
                    devices.snapshot().get(&peer).is_some_and(|device| {
                        device.connection == ConnectionState::Connected
                            && device.connection_generation == expected_generation
                    })
                });
                if !exact_session_current && matches!(&result, Ok(Some(_))) {
                    tracing::warn!(purpose = ?purpose, "Key transfer: session changed before the offer was activated");
                    object.as_mut().rust_mut().synthetic_transfer_cancel = None;
                    object.as_mut().rust_mut().synthetic_transfer_preparing_peer = None;
                    object.as_mut().set_synthetic_transfer_preparing(false);
                    object.as_mut().set_synthetic_transfer_error(QString::from(
                        "La sesión cambió antes de activar la transferencia. No se enviará material privado.",
                    ));
                    return;
                }
                object.as_mut().rust_mut().synthetic_transfer_cancel = None;
                object.as_mut().rust_mut().synthetic_transfer_preparing_peer = None;
                object.as_mut().set_synthetic_transfer_preparing(false);
                match result {
                    Ok(Some((transfer_id, mut cancel_guard))) => {
                        tracing::info!(purpose = ?purpose, "Key transfer: offer sent");
                        object.as_mut().rust_mut().synthetic_transfer_target =
                            Some((peer, expected_generation, transfer_id));
                        cancel_guard.disarm();
                        object.as_mut()
                            .set_synthetic_transfer_error(QString::default());
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(purpose = ?purpose, error = ?error, "Key transfer could not start");
                        object.as_mut().set_synthetic_transfer_error(QString::from(
                            synthetic_transfer_error_message(purpose, error),
                        ));
                    }
                }
            });
        });
    }

    /// Makes `device_id` sync the active vault. The phone renegotiates the
    /// session when the user enables it there too.
    pub fn enable_vault_sync(self: Pin<&mut Self>, device_id: QString) {
        self.change_vault_sync(device_id, true);
    }

    /// Stops syncing with `device_id`; the vault and the ledger history stay.
    pub fn disable_vault_sync(self: Pin<&mut Self>, device_id: QString) {
        self.change_vault_sync(device_id, false);
    }

    fn change_vault_sync(mut self: Pin<&mut Self>, device_id: QString, enabled: bool) {
        let Some(peer) = parse_device_id(&device_id.to_string()) else {
            return;
        };
        let Ok(directory) = lab_directory() else {
            self.as_mut()
                .set_vault_sync_message(QString::from(MirrorError::Profile.message()));
            return;
        };
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = if enabled {
                mirror::enable(&directory, peer)
            } else {
                mirror::disable(&directory, peer)
            };
            tracing::info!(enabled, error = ?result.err(), "Vault sync selection changed");
            let _ = thread.queue(move |mut object| {
                object.as_mut().set_vault_sync_message(match result {
                    Ok(()) if enabled => QString::from(
                        "Sincronización activada. Actívala también en el teléfono: Dispositivos → este Desktop → Sincronizar mi bóveda.",
                    ),
                    Ok(()) => QString::from("Sincronización desactivada. La bóveda no cambió."),
                    Err(error) => QString::from(error.message()),
                });
                object.as_mut().refresh_vault_sync_peers();
            });
        });
    }

    /// Exchanges vault changes with the ledger the phone syncs. QML calls it
    /// periodically; overlapping calls are ignored.
    pub fn run_vault_mirror(mut self: Pin<&mut Self>) {
        if self.rust().vault_mirror_running || self.rust().vault_sync_peers.is_empty() {
            return;
        }
        let Ok(directory) = lab_directory() else {
            return;
        };
        self.as_mut().rust_mut().vault_mirror_running = true;
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = mirror::run(&directory);
            match &result {
                Ok(outcome) if outcome.pulled + outcome.pushed + outcome.skipped > 0 => {
                    tracing::info!(
                        pulled = outcome.pulled,
                        pushed = outcome.pushed,
                        skipped = outcome.skipped,
                        "Vault mirror run"
                    )
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(error = ?error, "Vault mirror run failed"),
            }
            let _ = thread.queue(move |mut object| {
                object.as_mut().rust_mut().vault_mirror_running = false;
                match result {
                    Ok(outcome) => {
                        if outcome.pulled > 0 {
                            object.as_mut().vault_mirror_pulled();
                        }
                        if outcome.pulled > 0 || outcome.pushed > 0 {
                            object
                                .as_mut()
                                .set_vault_sync_message(QString::from(format!(
                                    "Bóveda sincronizada: {} recibidos, {} enviados.",
                                    outcome.pulled, outcome.pushed
                                )));
                        } else if object.rust().vault_sync_message.to_string()
                            == MirrorError::Locked.message()
                        {
                            object.as_mut().set_vault_sync_message(QString::default());
                        }
                    }
                    Err(error) => {
                        object
                            .as_mut()
                            .set_vault_sync_message(QString::from(error.message()));
                    }
                }
            });
        });
    }

    fn refresh_vault_sync_peers(self: Pin<&mut Self>) {
        let Ok(directory) = lab_directory() else {
            return;
        };
        let peers: Vec<DeviceId> = self
            .rust()
            .known_peers
            .iter()
            .map(|peer| peer.device_id)
            .collect();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let enabled: QStringList = peers
                .into_iter()
                .filter(|peer| mirror::is_enabled(&directory, *peer))
                .map(|peer| QString::from(peer.to_string()))
                .collect();
            let _ = thread.queue(move |mut object| {
                object.as_mut().set_vault_sync_peers(enabled);
            });
        });
    }

    /// Cancels preparation or the exact active transfer when the view closes,
    /// navigation changes, or the user disconnects the session.
    pub fn cancel_synthetic_key_transfer(mut self: Pin<&mut Self>) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
    }

    /// Keep the pairing method on a route the saved privacy choices allow, so
    /// the window never opens with LAN selected while LAN is disabled.
    fn align_pairing_method_with_policy(mut self: Pin<&mut Self>) {
        let allowed = lan_permitted(
            self.rust().require_vpn,
            self.rust().allow_lan_exception,
            self.rust().relay_only,
        );
        if allowed || self.rust().internet_enabled {
            return;
        }
        self.as_mut().set_lan_enabled(false);
        if !self.rust().relay_origin.is_empty() {
            self.as_mut().set_internet_enabled(true);
        }
    }

    #[track_caller]
    fn cancel_synthetic_key_transfer_operation(mut self: Pin<&mut Self>) {
        if self.rust().synthetic_transfer_preparing_peer.is_some()
            || self.rust().synthetic_transfer_target.is_some()
        {
            tracing::info!(
                caller_line = std::panic::Location::caller().line(),
                "Key transfer: cancelled"
            );
        }
        if let Some(cancel) = self.as_mut().rust_mut().synthetic_transfer_cancel.take() {
            let _ = cancel.send(true);
        }
        self.as_mut().rust_mut().synthetic_transfer_generation =
            self.rust().synthetic_transfer_generation.wrapping_add(1);
        self.as_mut().rust_mut().synthetic_transfer_preparing_peer = None;
        self.as_mut().set_synthetic_transfer_preparing(false);
        if let Some((peer, generation, transfer_id)) =
            self.as_mut().rust_mut().synthetic_transfer_target.take()
        {
            if let Some(owner) = self.rust().resume_owner.as_ref() {
                spawn_synthetic_transfer_cancel(owner.clone(), peer, generation, transfer_id);
            }
        }
    }

    fn run_synthetic_sync(mut self: Pin<&mut Self>, action: SyntheticSyncAction) {
        if !cfg!(debug_assertions) {
            self.as_mut().set_sync_error(QString::from(
                "La prueba sintética solo está disponible en Desktop debug.",
            ));
            return;
        }
        if self.rust().sync_busy || self.rust().busy || self.rust().maintenance_busy {
            return;
        }
        let Some(peer) = self.rust().selected_peer() else {
            self.as_mut()
                .set_sync_error(QString::from(SyntheticSyncError::NoPeerSelected.message()));
            return;
        };
        let Ok(directory) = lab_directory() else {
            self.as_mut().set_sync_error(QString::from(
                SyntheticSyncError::ProfileUnavailable.message(),
            ));
            return;
        };

        self.as_mut().set_sync_error(QString::default());
        self.as_mut()
            .set_sync_peer_id(QString::from(peer.device_id.to_string()));
        self.as_mut().set_sync_busy(true);
        self.as_mut().rust_mut().sync_generation = self.rust().sync_generation.wrapping_add(1);
        let generation = self.rust().sync_generation;
        let operation_peer_id = QString::from(peer.device_id.to_string());
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = super::synthetic_sync::run(&directory, peer.device_id, action);
            let _ = thread.queue(move |mut object| {
                if object.rust().sync_generation != generation {
                    return;
                }
                object.as_mut().set_sync_busy(false);
                if object.rust().sync_peer_id != operation_peer_id {
                    return;
                }
                match result {
                    Ok(snapshot) => {
                        object.as_mut().apply_synthetic_sync_snapshot(snapshot);
                        object.as_mut().set_sync_configured(true);
                        object.as_mut().set_sync_error(QString::default());
                    }
                    Err(error) => {
                        object
                            .as_mut()
                            .set_sync_error(QString::from(error.message()));
                        if matches!(error, SyntheticSyncError::NotConfigured) {
                            object.as_mut().set_sync_configured(false);
                            object.as_mut().set_sync_entries(QStringList::default());
                            object.as_mut().set_sync_local_revision_count(0);
                            object.as_mut().set_sync_status(QString::default());
                        }
                    }
                }
            });
        });
    }

    fn apply_synthetic_sync_snapshot(mut self: Pin<&mut Self>, snapshot: SyntheticSyncSnapshot) {
        let entries = snapshot.rows.into_iter().map(QString::from).collect();
        self.as_mut().set_sync_entries(entries);
        self.as_mut().set_sync_local_revision_count(
            i32::try_from(snapshot.local_revision_count).unwrap_or(i32::MAX),
        );
        self.as_mut()
            .set_sync_status(QString::from(snapshot.status));
    }

    fn replace_trusted_devices(
        mut self: Pin<&mut Self>,
        peers: Vec<PublicIdentity>,
        preferred: Option<DeviceId>,
    ) {
        let index = preferred
            .and_then(|id| peers.iter().position(|peer| peer.device_id == id))
            .or_else(|| (peers.len() == 1).then_some(0));
        let selected = index.and_then(|index| peers.get(index)).copied();
        let ids = peers
            .iter()
            .map(|peer| QString::from(peer.device_id.to_string()))
            .collect();
        self.as_mut().rust_mut().known_peers = peers;
        self.as_mut().set_device_ids(ids);
        self.as_mut().set_selected_index(
            index
                .and_then(|index| i32::try_from(index).ok())
                .unwrap_or(-1),
        );
        self.as_mut().set_can_reconnect(selected.is_some());
        self.as_mut()
            .set_peer(selected.map_or_else(QString::default, |peer| {
                QString::from(peer.device_id.to_string())
            }));
        let sync_peer_id = selected.map_or_else(QString::default, |peer| {
            QString::from(peer.device_id.to_string())
        });
        if self.rust().sync_peer_id != sync_peer_id {
            self.as_mut().set_sync_peer_id(sync_peer_id);
            self.as_mut().set_sync_configured(false);
            self.as_mut().set_sync_status(QString::default());
            self.as_mut().set_sync_error(QString::default());
            self.as_mut().set_sync_local_revision_count(0);
            self.as_mut().set_sync_entries(QStringList::default());
        }
    }

    pub fn start_pairing(self: Pin<&mut Self>) {
        self.start(false);
    }
    pub fn start_reconnect(mut self: Pin<&mut Self>) {
        if *self.lan_enabled() {
            self.as_mut().start_durable_reconnect();
        } else {
            // The emulator's TCP harness still uses the explicit legacy flow.
            self.start(true);
        }
    }

    fn start_durable_reconnect(mut self: Pin<&mut Self>) {
        if self
            .rust()
            .draining_worker
            .as_ref()
            .is_some_and(|done| !done.load(Ordering::Acquire))
            || !cfg!(debug_assertions)
            || *self.busy()
            || self.rust().active.is_some()
            || *self.session_active()
            || self.rust().maintenance_busy
        {
            return;
        }
        let Some(expected) = self.rust().selected_peer() else {
            return;
        };
        self.as_mut().start_resume_owner();
        let Some(owner) = self.rust().resume_owner.clone() else {
            self.as_mut()
                .set_status_code(QString::from("profile_error"));
            self.as_mut().set_status(QString::from(
                "No se pudo iniciar el listener de recuperación local.",
            ));
            return;
        };

        self.as_mut().rust_mut().generation = self.rust().generation.wrapping_add(1);
        let generation = self.rust().generation;
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status_code(QString::from("preparing_reconnect"));
        self.as_mut().set_bootstrap(QString::default());
        self.as_mut().set_sas(QString::default());
        self.as_mut().set_sas_emojis(QString::from("[]"));
        self.as_mut().set_status(QString::from(
            "Preparando la reconexión LAN con este dispositivo…",
        ));

        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = (|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| kryptopass_pairing_lab::LabError::Link)?;
                runtime.block_on(async {
                    let ready = owner.refresh_trust().await?;
                    if !ready.trusted_peers.contains(&expected) {
                        return Err(kryptopass_pairing_lab::LabError::Authentication);
                    }
                    if ready.reconnect_port == 0 {
                        return Err(kryptopass_pairing_lab::LabError::Link);
                    }
                    // This is an explicit local reconnect action. It only clears
                    // Desktop's disconnect suppression; Android still decides
                    // whether its own disabled intent permits reconnecting.
                    owner.allow_peer(expected.device_id);
                    Ok(())
                })
            })();
            let _ = thread.queue(move |mut object| {
                if object.rust().generation != generation {
                    return;
                }
                object.as_mut().set_busy(false);
                match result {
                    Ok(()) => {
                        object
                            .as_mut()
                            .set_status_code(QString::from("resume_ready"));
                        object.as_mut().set_status(QString::from(
                            "Listener LAN listo. Abre KryptoPass en Android y pulsa Reconectar para este Desktop.",
                        ));
                    }
                    Err(kryptopass_pairing_lab::LabError::Authentication) => {
                        object
                            .as_mut()
                            .set_status_code(QString::from("peer_not_authorized"));
                        object.as_mut().set_status(QString::from(
                            "Este dispositivo ya no está autorizado en este equipo.",
                        ));
                    }
                    Err(kryptopass_pairing_lab::LabError::Link) => {
                        object
                            .as_mut()
                            .set_status_code(QString::from("lan_error"));
                        object.as_mut().set_status(QString::from(
                            "No se pudo iniciar el listener LAN. Revisa la red e inténtalo de nuevo.",
                        ));
                    }
                    Err(_) => {
                        object
                            .as_mut()
                            .set_status_code(QString::from("profile_error"));
                        object.as_mut().set_status(QString::from(
                            "No se pudo abrir el perfil existente. No se creó una identidad nueva.",
                        ));
                    }
                }
            });
        });
    }

    fn start(mut self: Pin<&mut Self>, reconnect: bool) {
        if self
            .rust()
            .draining_worker
            .as_ref()
            .is_some_and(|done| !done.load(Ordering::Acquire))
        {
            return;
        }
        if !cfg!(debug_assertions) || *self.busy() || self.rust().active.is_some() {
            return;
        }
        let profile_directory = match lab_directory() {
            Ok(directory) => directory,
            Err(()) => {
                self.as_mut()
                    .set_status_code(QString::from("network_policy_blocked"));
                self.as_mut().set_status(QString::from(
                    "No se pudo localizar el archivo de política de red. No se inició una conexión.",
                ));
                return;
            }
        };
        let stored_settings = match load_network_privacy_settings(&profile_directory) {
            Ok(settings) => settings,
            Err(_) => {
                self.as_mut()
                    .set_status_code(QString::from("network_policy_blocked"));
                self.as_mut().set_status(QString::from(
                    "No se pudo leer la política de privacidad de red. No se inició una conexión.",
                ));
                return;
            }
        };
        let selected_bind_ip = stored_settings.lan_bind_ip;
        let policy = stored_settings.policy;
        if save_network_privacy_settings(
            &profile_directory,
            NetworkPrivacySettings {
                policy,
                lan_bind_ip: selected_bind_ip,
            },
        )
        .is_err()
        {
            self.as_mut()
                .set_status_code(QString::from("network_policy_blocked"));
            self.as_mut().set_status(QString::from(
                "No se pudo guardar la política de red. No se inició una conexión.",
            ));
            return;
        }
        let guard = NetworkPolicyGuard::new(policy);
        self.as_mut().rust_mut().active_network_guard = Some(guard.clone());
        self.as_mut().start_resume_owner();
        let Some(owner) = self.rust().resume_owner.clone() else {
            self.as_mut()
                .set_status_code(QString::from("profile_error"));
            self.as_mut().set_status(QString::from(
                "No se pudo iniciar el listener de recuperación local.",
            ));
            return;
        };
        owner.refresh_network_policy();
        let expected = if reconnect {
            self.rust().selected_peer()
        } else {
            None
        };
        if reconnect && expected.is_none() {
            return;
        }
        if let Some(peer) = expected {
            // A local reconnect action explicitly clears a prior local-disconnect block.
            owner.allow_peer(peer.device_id);
        }
        // Without an explicit choice (e.g. after a reset), use the address the
        // system would pick for LAN, the same rule the recovery listener uses.
        let lan_bind_ip = usize::try_from(self.rust().network_index)
            .ok()
            .and_then(|index| self.rust().network_ips.get(index))
            .copied()
            .or_else(|| {
                kryptopass_lan::select_local_bind_address(
                    None,
                    &kryptopass_lan::local_addresses().unwrap_or_default(),
                    kryptopass_lan::os_selected_local_address(),
                )
            })
            .filter(|_| {
                *self.lan_enabled()
                    && lan_permitted(
                        self.rust().require_vpn,
                        self.rust().allow_lan_exception,
                        self.rust().relay_only,
                    )
            });
        tracing::info!(
            lan = lan_bind_ip.is_some(),
            internet = *self.internet_enabled(),
            "Pairing carrier selected"
        );
        let network = if *self.internet_enabled() && lan_bind_ip.is_some() {
            // One invitation with both routes: the phone tries LAN, then the relay.
            match LabNetwork::auto(
                lan_bind_ip.unwrap_or(std::net::Ipv4Addr::UNSPECIFIED.into()),
                self.rust().relay_origin.to_string().trim(),
            ) {
                Ok(network) => network,
                Err(_) => {
                    self.as_mut()
                        .set_status_code(QString::from("network_error"));
                    self.as_mut().set_status(QString::from(
                        "No se pudo preparar el servidor de conexión por Internet.",
                    ));
                    return;
                }
            }
        } else if *self.internet_enabled() {
            match LabNetwork::internet(self.rust().relay_origin.to_string().trim()) {
                Ok(network) => network,
                Err(_) => {
                    self.as_mut()
                        .set_status_code(QString::from("network_error"));
                    self.as_mut().set_status(QString::from(
                        "No se pudo preparar el servidor de conexión por Internet.",
                    ));
                    return;
                }
            }
        } else if *self.lan_enabled() {
            let bind_ip = usize::try_from(self.rust().network_index)
                .ok()
                .and_then(|index| self.rust().network_ips.get(index))
                .copied();
            let Some(bind_ip) = bind_ip else {
                self.as_mut().set_status(QString::from(
                    "Selecciona la dirección de Desktop en tu red local.",
                ));
                return;
            };
            LabNetwork::Lan { bind_ip }
        } else {
            LabNetwork::EmulatorLoopback
        };
        let directory = profile_directory;
        let (cancel, cancelled) = watch::channel(false);
        let completed = Arc::new(AtomicBool::new(false));
        let worker_completed = completed.clone();
        let (decision, decisions) = oneshot::channel();
        self.as_mut().rust_mut().active = None;
        self.as_mut().rust_mut().generation = self.rust().generation.wrapping_add(1);
        let generation = self.rust().generation;
        self.as_mut().rust_mut().pending = Some(PendingOperation {
            cancel,
            decision: Some(decision),
            completed,
        });
        self.as_mut().set_busy(true);
        self.as_mut().set_status_code(QString::from("connecting"));
        self.as_mut().set_awaiting(false);
        self.as_mut().set_bootstrap(QString::default());
        self.as_mut().set_sas(QString::default());
        self.as_mut().set_sas_emojis(QString::from("[]"));
        self.as_mut().set_status(QString::from(
            "Abriendo identidad protegida por el llavero del sistema…",
        ));
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let _completed = WorkerCompletion(worker_completed);
            let result = (|| {
                let host = (if reconnect {
                    LabHost::open_existing_system_profile(&directory)
                } else {
                    // This is the only path that may initialize a profile, and it
                    // is reached only after the user explicitly starts pairing.
                    LabHost::open_system_profile(&directory)
                })
                .inspect_err(|error| {
                    tracing::warn!(
                        phase = "open_pairing_profile",
                        reason = error_code(error),
                        reconnect,
                        "Device link setup failed"
                    );
                })?;
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| kryptopass_pairing_lab::LabError::Link)?;
                runtime.block_on(async {
                    let (events, mut received) = mpsc::channel(8);
                    let mut cancelled = cancelled;
                    let outcome = {
                        let operation = host.open_session_with_network_guard(
                            expected,
                            events,
                            decisions,
                            cancelled.clone(),
                            network.clone(),
                            guard.clone(),
                        );
                        tokio::pin!(operation);
                        loop {
                            tokio::select! {
                                result = &mut operation => break result?,
                                Some(event) = received.recv() => {
                                    let _ = thread.queue(move |mut object| {
                                        if object.rust().generation != generation { return; }
                                        match event {
                                            HostEvent::Bootstrap(code) => {
                                                object.as_mut().set_bootstrap(QString::from(code));
                                                object.as_mut().set_status_code(QString::from("waiting_peer"));
                                                object.as_mut().set_status(QString::from("Escanea el QR desde Android o copia el código de conexión."));
                                            }
                                            HostEvent::Sas(sas) => {
                                                let emojis = sas.emojis.iter().enumerate().map(|(index, entry)| serde_json::json!({"index":index + 1,"emoji":entry.emoji,"name":entry.name})).collect::<Vec<_>>();
                                                object.as_mut().set_sas_emojis(QString::from(serde_json::Value::from(emojis).to_string()));
                                                let display = sas.emojis.iter().enumerate()
                                                    .map(|(index, entry)| format!("{}. {}  {}", index + 1, entry.emoji, entry.name))
                                                    .collect::<Vec<_>>().join("\n");
                                                object.as_mut().set_sas(QString::from(display));
                                                object.as_mut().set_awaiting(true);
                                                object.as_mut().set_status_code(QString::from("awaiting_sas"));
                                                object.as_mut().set_bootstrap(QString::default());
                                                object.as_mut().set_status(QString::from("Compara los siete emojis con Android y confirma en ambas pantallas."));
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    };
                    let peer = outcome.peer;
                    let ready = owner.refresh_trust().await?;
                    if !ready.trusted_peers.contains(&peer) {
                        return Err(kryptopass_pairing_lab::LabError::Authentication);
                    }
                    let reconnect_port = if matches!(&network, LabNetwork::Lan { .. } | LabNetwork::Auto { .. }) {
                        ready.reconnect_port
                    } else {
                        0
                    };
                    let options = SessionOptions { reconnect_port };
                    let devices = ready.devices.clone();
                    let mut handle = host.start_device_session(
                        devices.clone(),
                        outcome.session,
                        peer,
                        options,
                    )
                    .map_err(|_| kryptopass_pairing_lab::LabError::Session)?;
                    let session_generation = handle.generation();
                    owner
                        .register_synthetic_transfer_control(
                            handle.synthetic_key_transfer_control(),
                        )
                        .await
                        .map_err(|_| kryptopass_pairing_lab::LabError::Session)?;
                    let mut changes = devices.subscribe();
                    let ready_devices = devices.clone();
                    let peers = ready.trusted_peers;
                    let _ = thread.queue(move |mut object| {
                        if object.rust().generation != generation { return; }
                        let mut pending = object.as_mut().rust_mut().pending.take();
                        if let Some(pending) = pending.as_mut() { pending.decision = None; }
                        object.as_mut().rust_mut().active = pending;
                        object.as_mut().set_session_active(true);
                        object.as_mut().set_active_device_id(QString::from(peer.device_id.to_string()));
                        object.as_mut().rust_mut().devices = Some(ready_devices.clone());
                        object.as_mut().set_busy(false);
                        object.as_mut().set_awaiting(false);
                        object.as_mut().set_bootstrap(QString::default());
                        object.as_mut().replace_trusted_devices(peers, Some(peer.device_id));
                        object.as_mut().apply_devices_snapshot(&ready_devices.snapshot());
                        object.as_mut().set_status_code(QString::from("connecting"));
                        object.as_mut().set_status(QString::from("Confianza guardada. Abriendo la sesión autenticada…"));
                    });
                    let lifecycle = handle.subscribe_lifecycle();
                    loop {
                        if *cancelled.borrow() {
                            let _ = handle.disconnect(SessionCloseReason::UserRequested);
                            handle.wait_closed().await;
                            break;
                        }
                        tokio::select! {
                            () = handle.wait_closed() => break,
                            _ = cancelled.changed() => {
                                let _ = handle.disconnect(SessionCloseReason::UserRequested);
                                handle.wait_closed().await;
                                break;
                            }
                            changed = changes.changed() => {
                                if changed.is_err() { break; }
                                let snapshot = changes.borrow_and_update().clone();
                                let _ = thread.queue(move |mut object| {
                                    if object.rust().generation != generation { return; }
                                    object.as_mut().apply_devices_snapshot(&snapshot);
                                    if let Some(device) = snapshot.get(&peer.device_id) {
                                        object.as_mut().set_status_code(QString::from(connection_code(device.connection)));
                                        object.as_mut().set_status(QString::from(connection_status(device.connection)));
                                    }
                                });
                            }
                        }
                    }
                    let last_exit = *lifecycle.borrow();
                    owner.unregister_synthetic_transfer_control(peer.device_id, session_generation);
                    Ok((peer.device_id, devices, last_exit))
                })
            })();
            let _ = thread.queue(move |mut object| {
                if object.rust().generation != generation {
                    return;
                }
                object.as_mut().set_busy(false);
                object.as_mut().set_awaiting(false);
                object.as_mut().set_bootstrap(QString::default());
                object.as_mut().rust_mut().pending = None;
                object.as_mut().rust_mut().active = None;
                object.as_mut().rust_mut().active_network_guard = None;
                object.as_mut().set_session_active(false);
                object.as_mut().set_active_device_id(QString::default());
                match result {
                    Ok((peer_id, devices, last_exit)) => {
                        let snapshot = devices.snapshot();
                        object.as_mut().apply_devices_snapshot(&snapshot);
                        let state = snapshot
                            .get(&peer_id)
                            .map_or(ConnectionState::Disconnected, |device| device.connection);
                        object
                            .as_mut()
                            .set_status_code(QString::from(connection_code(state)));
                        object
                            .as_mut()
                            .set_status(QString::from(connection_status(state)));
                        if let Some(SessionEvent::Failed { failure, .. }) = last_exit {
                            if matches!(failure, SessionFailure::Authentication | SessionFailure::HelloRejected | SessionFailure::Protocol | SessionFailure::ControlRate | SessionFailure::NonceSequence) {
                                object.as_mut().set_status_code(QString::from("authentication_error"));
                                object.as_mut().set_status(QString::from("La sesión no superó la comprobación de seguridad. La confianza guardada se conserva, pero la conexión se detuvo."));
                            }
                        }
                    }
                    Err(error) => {
                        object
                            .as_mut()
                            .set_status_code(QString::from(error_code(&error)));
                        if let Some(devices) = object.rust().devices.clone() {
                            for device in &devices.snapshot().devices {
                                if matches!(
                                    device.connection,
                                    ConnectionState::Connecting | ConnectionState::Reconnecting
                                ) {
                                    let _ = devices.update_connection_state(
                                        device.identity.device_id,
                                        ConnectionState::Failed,
                                    );
                                }
                            }
                            object.as_mut().apply_devices_snapshot(&devices.snapshot());
                        }
                        object
                            .as_mut()
                            .set_status(QString::from(error_status(&error)));
                    }
                }
            });
        });
    }

    pub fn decide(mut self: Pin<&mut Self>, accepted: bool) {
        if !*self.awaiting() {
            return;
        }
        self.as_mut().set_awaiting(false);
        if let Some(operation) = self.as_mut().rust_mut().pending.as_mut() {
            if let Some(decision) = operation.decision.take() {
                let _ = decision.send(if accepted {
                    LocalDecision::Approve
                } else {
                    LocalDecision::Reject
                });
            }
        }
        self.as_mut().set_status(QString::from(if accepted {
            "Esperando la confirmación del teléfono…"
        } else {
            "Rechazando…"
        }));
        self.as_mut().set_status_code(QString::from(if accepted {
            "confirming"
        } else {
            "rejected"
        }));
    }

    pub fn cancel(mut self: Pin<&mut Self>) {
        if self.rust().maintenance_busy {
            return;
        }
        self.as_mut().cancel_synthetic_key_transfer_operation();
        self.as_mut().disconnect_device();
        if let Some(operation) = self.as_mut().rust_mut().pending.take() {
            let _ = operation.cancel.send(true);
            self.as_mut().rust_mut().draining_worker = Some(operation.completed.clone());
            let generation = self.rust().generation.wrapping_add(1);
            self.as_mut().rust_mut().generation = generation;
            self.as_mut().set_busy(false);
            self.as_mut().set_status_code(QString::from("cancelled"));
            self.as_mut().set_status(QString::from(
                "Operación cancelada. La confianza ya guardada se conserva.",
            ));
        }
        self.as_mut().set_awaiting(false);
        self.as_mut().set_bootstrap(QString::default());
        self.as_mut().set_sas(QString::default());
        self.as_mut().set_sas_emojis(QString::from("[]"));
    }

    /// Requests an authenticated session close without deleting persisted trust.
    pub fn disconnect_device(mut self: Pin<&mut Self>) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
        if let Some(peer) = parse_device_id(&self.rust().active_device_id.to_string()) {
            if let Some(owner) = self.rust().resume_owner.as_ref() {
                owner.suppress_peer(peer);
            }
        }
        if let Some(owner) = self.as_mut().rust_mut().active.take() {
            let _ = owner.cancel.send(true);
            self.as_mut().rust_mut().draining_worker = Some(owner.completed.clone());
            self.as_mut().set_status_code(QString::from("closing"));
            self.as_mut().set_status(QString::from(
                "Cerrando la sesión. La confianza guardada se conserva.",
            ));
        }
    }

    pub fn disconnect_peer(mut self: Pin<&mut Self>, device_id: QString) {
        let Some(peer) = parse_device_id(&device_id.to_string()) else {
            return;
        };
        if self
            .rust()
            .synthetic_transfer_target
            .is_some_and(|(target_peer, _, _)| target_peer == peer)
            || self
                .rust()
                .synthetic_transfer_preparing_peer
                .is_some_and(|(target_peer, _)| target_peer == peer)
        {
            self.as_mut().cancel_synthetic_key_transfer_operation();
        }
        if let Some(owner) = self.rust().resume_owner.as_ref() {
            owner.suppress_peer(peer);
        }
        if device_id == self.rust().active_device_id {
            self.as_mut().disconnect_device();
        }
    }

    /// Detaches the view while keeping an established connection owned by the app.
    pub fn release_ui(mut self: Pin<&mut Self>) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
        if self.rust().pending.is_some() {
            self.as_mut().cancel();
        }
        self.as_mut().set_bootstrap(QString::default());
        self.as_mut().set_sas(QString::default());
        self.as_mut().set_sas_emojis(QString::from("[]"));
    }

    pub fn save_device_alias(mut self: Pin<&mut Self>, device_id: QString, alias: QString) {
        if !cfg!(debug_assertions) || self.rust().alias_busy {
            return;
        }
        self.as_mut().set_alias_error(QString::default());
        if self.rust().busy {
            self.as_mut().set_alias_error(QString::from(
                "Espera a que termine la operación de conexión antes de cambiar el nombre.",
            ));
            return;
        }
        let Some(id) = parse_device_id(&device_id.to_string()) else {
            self.as_mut()
                .set_alias_error(QString::from("Selecciona un dispositivo emparejado."));
            return;
        };
        if !self
            .rust()
            .known_peers
            .iter()
            .any(|peer| peer.device_id == id)
        {
            self.as_mut()
                .set_alias_error(QString::from("El dispositivo ya no está autorizado."));
            return;
        }
        let Ok(directory) = lab_directory() else {
            self.as_mut().set_alias_error(QString::from(
                "No se pudo localizar el perfil de dispositivos.",
            ));
            return;
        };
        let value = alias.to_string();
        self.as_mut().set_alias_busy(true);
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = (|| {
                let host = LabHost::open_existing_system_profile(&directory)
                    .map_err(|_| kryptopass_pairing_lab::AliasError::Storage)?;
                host.set_device_alias(id, &value)?;
                host.device_aliases()
            })();
            let _ = thread.queue(move |mut object| {
                object.as_mut().set_alias_busy(false);
                match result {
                    Ok(aliases) => {
                        object.as_mut().rust_mut().aliases = aliases;
                        if let Some(devices) = object.rust().devices.clone() { object.as_mut().apply_devices_snapshot(&devices.snapshot()); }
                        object.as_mut().alias_saved(&QString::from(id.to_string()));
                    },
                    Err(error) => {
                        let message = match error {
                            kryptopass_pairing_lab::AliasError::InvalidName => "Usa hasta 64 caracteres y 256 bytes, sin controles ni saltos de línea.",
                            kryptopass_pairing_lab::AliasError::UnknownDevice => "El dispositivo ya no está autorizado.",
                            _ => "No se pudo guardar el nombre. La identidad y la confianza se conservan.",
                        };
                        object.as_mut().set_alias_error(QString::from(message));
                    }
                }
            });
        });
    }

    /// Explicit local authorization removal; the active peer session is drained first.
    pub fn forget_device(mut self: Pin<&mut Self>, device_id: QString) {
        let Some(id) = parse_device_id(&device_id.to_string()) else {
            return;
        };
        if !self
            .rust()
            .known_peers
            .iter()
            .any(|peer| peer.device_id == id)
        {
            tracing::warn!(reason = "unknown_peer", "Forget device ignored");
            return;
        }
        self.as_mut().maintain_profile(Some(id));
    }

    /// Resets the isolated pairing identity and trust, never the password vault.
    pub fn reset_pairing_profile(self: Pin<&mut Self>) {
        self.maintain_profile(None);
    }

    fn maintain_profile(mut self: Pin<&mut Self>, forget: Option<DeviceId>) {
        tracing::info!(
            forget = forget.is_some(),
            maintenance_busy = self.rust().maintenance_busy,
            alias_busy = self.rust().alias_busy,
            sync_busy = self.rust().sync_busy,
            busy = self.rust().busy,
            "Pairing profile maintenance requested"
        );
        if !cfg!(debug_assertions) || self.rust().maintenance_busy || self.rust().alias_busy {
            return;
        }
        if self.rust().sync_busy {
            self.as_mut().set_status(QString::from(
                "Espera a que termine la operación de la bóveda sintética antes de cambiar el perfil.",
            ));
            return;
        }
        if self.rust().busy {
            self.as_mut().set_status(QString::from(
                "Cancela la vinculación pendiente antes de borrar dispositivos.",
            ));
            return;
        }
        let Ok(directory) = lab_directory() else {
            return;
        };
        let completed = self
            .as_mut()
            .rust_mut()
            .active
            .take()
            .map(|owner| {
                let _ = owner.cancel.send(true);
                owner.completed.clone()
            })
            .or_else(|| self.rust().draining_worker.clone());
        self.as_mut().rust_mut().draining_worker = completed.clone();
        let resume_owner = self.as_mut().stop_resume_owner();
        self.as_mut().rust_mut().generation = self.rust().generation.wrapping_add(1);
        let generation = self.rust().generation;
        self.as_mut().set_maintenance_busy(true);
        self.as_mut().set_busy(true);
        self.as_mut().set_status_code(QString::from("closing"));
        self.as_mut().set_status(QString::from(
            "Cerrando la sesión y limpiando el perfil de vinculación…",
        ));
        self.as_mut().set_bootstrap(QString::default());
        self.as_mut().set_sas(QString::default());
        self.as_mut().set_sas_emojis(QString::from("[]"));
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = (|| {
                let deadline = std::time::Instant::now() + Duration::from_secs(8);
                if let Some(done) = completed {
                    while !done.load(Ordering::Acquire) {
                        if std::time::Instant::now() >= deadline {
                            return Err(kryptopass_pairing_lab::LabError::Session);
                        }
                        std::thread::sleep(Duration::from_millis(25));
                    }
                }
                if let Some(owner) = resume_owner {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if !owner.shutdown_and_wait(remaining) {
                        return Err(kryptopass_pairing_lab::LabError::Session);
                    }
                }
                let Some(peer) = forget else {
                    LabHost::reset_system_profile(&directory)?;
                    return Ok(None);
                };
                let host = LabHost::open_existing_system_profile(&directory)?;
                host.forget_peer(peer)?;
                let peers = host.trusted_peers()?;
                let aliases = host
                    .device_aliases()
                    .map_err(|_| kryptopass_pairing_lab::LabError::Profile)?;
                let devices = DeviceDirectory::new(host.public_identity().device_id);
                devices
                    .replace_trusted(peers.clone())
                    .map_err(|_| kryptopass_pairing_lab::LabError::Profile)?;
                Ok(Some((peers, aliases, devices)))
            })();
            let _ = thread.queue(move |mut object| {
                if object.rust().generation != generation { return; }
                object.as_mut().set_maintenance_busy(false);
                object.as_mut().set_busy(false);
                let drained = object.rust().draining_worker.as_ref().is_none_or(|done| done.load(Ordering::Acquire));
                if drained { object.as_mut().rust_mut().draining_worker = None; }
                object.as_mut().set_session_active(!drained);
                object.as_mut().set_active_device_id(QString::default());
                object.as_mut().rust_mut().devices = None;
                object.as_mut().rust_mut().aliases.clear();
                object.as_mut().replace_trusted_devices(Vec::new(), None);
                object.as_mut().set_devices_snapshot(QString::from("{\"revision\":0,\"devices\":[]}"));
                match result {
                    Ok(Some((peers, aliases, devices))) => {
                        object.as_mut().rust_mut().aliases = aliases;
                        object.as_mut().replace_trusted_devices(peers, None);
                        object.as_mut().apply_devices_snapshot(&devices.snapshot());
                        object.as_mut().rust_mut().devices = Some(devices);
                        object.as_mut().set_status_code(QString::from("disconnected"));
                        object.as_mut().set_status(QString::from("Dispositivo olvidado en este equipo. Para volver a vincularlo, compara un SAS nuevo."));
                    }
                    Ok(None) => {
                        object.as_mut().set_status_code(QString::from("idle"));
                        object.as_mut().set_status(QString::from("Perfil de vinculación eliminado. Crea una invitación para empezar de cero. La bóveda y OpenPGP se conservan."));
                    }
                    Err(error) => {
                        tracing::warn!(reason = error_code(&error), "Pairing profile maintenance failed");
                        object.as_mut().set_status_code(QString::from("profile_error"));
                        object.as_mut().set_status(QString::from("No se completó el borrado. No se creó otra identidad. Puedes reintentar el reinicio del perfil."));
                    }
                }
                object.as_mut().start_resume_owner();
            });
        });
    }

    fn set_bootstrap(mut self: Pin<&mut Self>, value: QString) {
        if self.rust().bootstrap == value {
            return;
        }
        let source = if value.is_empty() {
            QString::default()
        } else {
            super::secret_qr::source(&value.to_string())
                .map(|source| QString::from(source.as_str()))
                .unwrap_or_default()
        };
        self.as_mut().set_bootstrap_qr(source);
        self.as_mut().rust_mut().bootstrap = value;
        self.as_mut().bootstrap_changed();
    }

    fn apply_devices_snapshot(mut self: Pin<&mut Self>, snapshot: &DeviceDirectorySnapshot) {
        if snapshot.revision < self.rust().devices_revision {
            return;
        }
        self.as_mut().rust_mut().devices_revision = snapshot.revision;
        let devices = snapshot
            .devices
            .iter()
            .map(|device| {
                let alias = self.rust().aliases.get(&device.identity.device_id).cloned().unwrap_or_default();
                let id = device.identity.device_id.to_string();
                let name = if alias.is_empty() { format!("Dispositivo {}", &id[..12]) } else { alias.clone() };
                let info = device.connection_info;
                let route = match info.route {
                    kryptopass_devices::ConnectionRoute::Unknown => "unknown",
                    kryptopass_devices::ConnectionRoute::Local => "local",
                    kryptopass_devices::ConnectionRoute::Direct => "direct",
                    kryptopass_devices::ConnectionRoute::Turn => "turn",
                };
                let route_label = if device.connection != ConnectionState::Connected {
                    String::new()
                } else {
                    match info.route {
                        kryptopass_devices::ConnectionRoute::Unknown => "Comprobando…",
                        kryptopass_devices::ConnectionRoute::Local => "LAN",
                        kryptopass_devices::ConnectionRoute::Direct => "Directo",
                        kryptopass_devices::ConnectionRoute::Turn => "TURN",
                    }.to_owned()
                };
                let synthetic_progress = self
                    .rust()
                    .synthetic_transfer_progress
                    .get(&device.identity.device_id)
                    .filter(|(generation, _)| *generation == device.connection_generation)
                    .map(|(_, progress)| *progress);
                let synthetic_transfer_id = synthetic_progress
                    .and_then(|progress| progress.transfer_id)
                    .map(|id| hex_transfer_id(&id))
                    .unwrap_or_default();
                serde_json::json!({
                    "route": route,
                    "routeLabel": route_label,
                    "addressFamily": match info.family {
                        kryptopass_devices::ConnectionFamily::Unknown => "unknown",
                        kryptopass_devices::ConnectionFamily::Ipv4 => "ipv4",
                        kryptopass_devices::ConnectionFamily::Ipv6 => "ipv6",
                        kryptopass_devices::ConnectionFamily::Mixed => "mixed",
                    },
                    "deviceId": id,
                    "connectionGeneration": device.connection_generation.to_string(),
                    "syntheticTransferPhase": synthetic_progress.map_or("unsupported", |progress| synthetic_transfer_phase_code(progress.phase)),
                    "syntheticTransferId": synthetic_transfer_id,
                    "keyTransferVersion": synthetic_progress.map_or(0, |progress| progress.version),
                    "alias": alias,
                    "displayName": name,
                    "isActive": self.rust().session_active && id == self.rust().active_device_id.to_string(),
                    "trust": "trusted",
                    "connection": match device.connection {
                        ConnectionState::Disconnected => "disconnected",
                        ConnectionState::Connecting => "connecting",
                        ConnectionState::Reconnecting => "reconnecting",
                        ConnectionState::Connected => "connected",
                        ConnectionState::Failed => "failed",
                    }
                })
            })
            .collect::<Vec<_>>();
        let value = serde_json::json!({"revision": snapshot.revision, "devices": devices});
        self.as_mut()
            .set_devices_snapshot(QString::from(value.to_string()));
        let active_routes: Vec<_> = snapshot
            .devices
            .iter()
            .filter(|device| device.connection == ConnectionState::Connected)
            .map(|device| device.connection_info)
            .collect();
        tracing::info!(
            connected_devices = active_routes.len(),
            active_routes = ?active_routes,
            "Pairing lab: device session state updated"
        );
    }
}

impl PairingLabControllerRust {
    fn selected_peer(&self) -> Option<PublicIdentity> {
        usize::try_from(self.selected_index)
            .ok()
            .and_then(|index| self.known_peers.get(index))
            .copied()
    }
}

fn persist_network_settings(lan_bind_ip: Option<IpAddr>) -> Result<(), ()> {
    let directory = lab_directory()?;
    // Selecting a local address does not authorize changing the saved privacy
    // policy. An unreadable file must not be replaced with stale UI defaults.
    let policy = load_network_privacy_settings(&directory)
        .map_err(|_| ())?
        .policy;
    save_network_privacy_settings(
        &directory,
        NetworkPrivacySettings {
            policy,
            lan_bind_ip,
        },
    )
    .map_err(|_| ())
}

fn error_status(error: &kryptopass_pairing_lab::LabError) -> &'static str {
    use kryptopass_pairing_lab::LabError;
    match error {
        LabError::Cancelled => "Operación cancelada.",
        LabError::Expired => "La invitación o conexión caducó. Genera otro código.",
        LabError::Rejected => "Emparejamiento rechazado. No se concedió confianza.",
        LabError::KeyStore => {
            "No se pudo acceder al llavero del sistema para abrir la identidad de este equipo."
        }
        LabError::Profile => "No se pudieron cargar los datos de vinculación de este equipo.",
        LabError::Session => {
            "La confianza se guardó, pero no se pudo abrir la sesión activa. Puedes reconectar."
        }
        LabError::Link => {
            "El teléfono no completó la conexión de red. Comprueba su error y genera otra invitación; todavía no se verificaron identidades."
        }
        LabError::Authentication => {
            "La verificación criptográfica no se completó. No se autorizó el dispositivo. Genera una invitación nueva."
        }
        LabError::NetworkPrivacyBlocked(NetworkPolicyError::VpnUnverifiable) => {
            "No se puede comprobar si la conexión a Internet pasa por una VPN en este sistema, así que quedó en pausa."
        }
        LabError::NetworkPrivacyBlocked(NetworkPolicyError::VpnInactive) => {
            "Elegiste usar Internet solo a través de VPN y ahora no hay una VPN activa. Conéctala para continuar."
        }
        LabError::NetworkPrivacyBlocked(NetworkPolicyError::LanExceptionRequired) => {
            "La red local está desactivada mientras se exige VPN. Actívala en Privacidad si la necesitas."
        }
        LabError::NetworkPrivacyBlocked(NetworkPolicyError::RelayRequired) => {
            "Ocultar tu dirección IP desactiva la red local: todo pasa por el servidor de retransmisión."
        }
        LabError::NetworkPrivacyBlocked(NetworkPolicyError::PolicyChanged) => {
            "La política de privacidad cambió. La operación se cerró sin reutilizar su ruta anterior."
        }
        LabError::NetworkPrivacyBlocked(NetworkPolicyError::RouteChanged) => {
            "La ruta de red cambió. La operación se cerró y no se reintentó por otra interfaz."
        }
        LabError::NetworkPrivacyBlocked(NetworkPolicyError::RouteSnapshotUnavailable) => {
            "No se pudo verificar la ruta del sistema. La conexión quedó bloqueada."
        }
    }
}

/// VPN state for QML: 0 not required, 1 unverifiable, 2 active, 3 inactive.
fn vpn_status_code(policy: NetworkPrivacyPolicy) -> i32 {
    match NetworkPolicyGuard::new(policy).vpn_status() {
        VpnStatus::NotRequired => 0,
        VpnStatus::Unverifiable => 1,
        VpnStatus::Active => 2,
        VpnStatus::Inactive => 3,
    }
}

/// Whether any LAN path is allowed by the privacy choices.
const fn lan_permitted(require_vpn: bool, allow_lan_exception: bool, relay_only: bool) -> bool {
    !relay_only && (!require_vpn || allow_lan_exception)
}

fn error_code(error: &kryptopass_pairing_lab::LabError) -> &'static str {
    use kryptopass_pairing_lab::LabError;
    match error {
        LabError::Cancelled => "cancelled",
        LabError::Expired => "expired",
        LabError::Rejected => "rejected",
        LabError::Profile => "profile_error",
        LabError::KeyStore => "keystore_error",
        LabError::Authentication => "authentication_error",
        LabError::Link | LabError::Session => "transport_error",
        LabError::NetworkPrivacyBlocked(_) => "network_policy_blocked",
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

fn synthetic_transfer_phase_code(
    phase: kryptopass_devices::SyntheticTransferPhase,
) -> &'static str {
    use kryptopass_devices::SyntheticTransferPhase;
    match phase {
        SyntheticTransferPhase::Unsupported => "unsupported",
        SyntheticTransferPhase::Ready => "ready",
        SyntheticTransferPhase::WaitingAcceptance => "waiting_acceptance",
        SyntheticTransferPhase::Sending => "sending",
        SyntheticTransferPhase::WaitingReceipt => "waiting_receipt",
        SyntheticTransferPhase::Complete => "complete",
        SyntheticTransferPhase::Rejected => "rejected",
        SyntheticTransferPhase::Cancelled => "cancelled",
        SyntheticTransferPhase::Expired => "expired",
        SyntheticTransferPhase::Failed => "failed",
        SyntheticTransferPhase::Uncertain => "uncertain",
    }
}

fn hex_transfer_id(id: &[u8; 16]) -> String {
    let mut result = String::with_capacity(32);
    for byte in id {
        use std::fmt::Write;
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn synthetic_transfer_error_message(
    purpose: KeyTransferPurpose,
    error: kryptopass_devices::SyntheticTransferError,
) -> &'static str {
    use kryptopass_devices::SyntheticTransferError;
    match (purpose, error) {
        (KeyTransferPurpose::VaultIdentity, SyntheticTransferError::Unsupported) => {
            "El teléfono no admite recibir tu clave. Actualiza KryptoPass en Android."
        }
        (KeyTransferPurpose::VaultIdentity, SyntheticTransferError::Invalid) => {
            "No se pudo exportar tu clave: se canceló la contraseña o la clave no está disponible. No se envió material privado."
        }
        (_, error) => synthetic_transfer_fallback_message(error),
    }
}

fn synthetic_transfer_fallback_message(
    error: kryptopass_devices::SyntheticTransferError,
) -> &'static str {
    use kryptopass_devices::SyntheticTransferError;
    match error {
        SyntheticTransferError::Unsupported => {
            "El teléfono o la sesión no admite la transferencia sintética."
        }
        SyntheticTransferError::Busy => "Ya se inició una transferencia en esta sesión.",
        SyntheticTransferError::Invalid => {
            "No se pudo preparar la clave sintética local. No se envió material privado."
        }
        SyntheticTransferError::Expired => "La transferencia caducó. Vuelve a iniciar la prueba.",
        SyntheticTransferError::WrongSession => {
            "La sesión o la generación cambió. Vuelve a conectar antes de intentarlo."
        }
    }
}

macro_rules! notifying_property {
    ($setter:ident, $field:ident, $changed:ident, $ty:ty) => {
        impl qobject::PairingLabController {
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

fn lab_directory() -> Result<PathBuf, ()> {
    crate::storage::paths::browser_state_directory(crate::app_info::APP.desktop_file_id)
        .map(|parent| parent.join("emulator-pairing-lab-v1"))
        .map_err(|_| ())
}

notifying_property!(
    set_maintenance_busy,
    maintenance_busy,
    maintenance_busy_changed,
    bool
);
notifying_property!(set_sync_busy, sync_busy, sync_busy_changed, bool);
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

/// Secret-free UI state of the phone → Desktop vault-identity transfer.
struct InboundTransferUi {
    peer: DeviceId,
    generation: u64,
    transfer_id: [u8; 16],
    fingerprint: [u8; 20],
    device_name: String,
    assessment: Option<super::synthetic_key_transfer::InboundAssessment>,
    phase: &'static str,
    received: u64,
    total: u64,
    message: String,
    error: bool,
    decision_sent: bool,
    import_started: bool,
    dismissed: bool,
}

/// Resolves the scoped capability of the exact session generation and runs
/// `action` with it on the calling worker thread.
fn with_inbound_control<R>(
    owner: &ResumeProcessOwner,
    peer: DeviceId,
    generation: u64,
    action: impl FnOnce(kryptopass_devices::SyntheticTransferControl) -> R,
) -> Option<R> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let control = runtime.block_on(owner.synthetic_transfer_control(peer, generation))?;
    Some(action(control))
}

fn grouped_fingerprint(hex: &str) -> String {
    hex.as_bytes()
        .chunks(4)
        .map(|group| std::str::from_utf8(group).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

impl qobject::PairingLabController {
    /// Accepts the inbound vault identity after reviewing sender and fingerprint.
    pub fn accept_inbound_key_transfer(self: Pin<&mut Self>) {
        self.decide_inbound_key_transfer(true);
    }

    /// Declines the inbound offer; nothing is received or imported.
    pub fn reject_inbound_key_transfer(self: Pin<&mut Self>) {
        self.decide_inbound_key_transfer(false);
    }

    /// Hides a finished operation; an undecided offer is rejected instead.
    pub fn dismiss_inbound_key_transfer(mut self: Pin<&mut Self>) {
        let waiting = self.rust().inbound_transfer.as_ref().is_some_and(|ui| {
            matches!(ui.phase, "assessing" | "waiting_decision") && !ui.decision_sent
        });
        if waiting {
            self.as_mut().decide_inbound_key_transfer(false);
            return;
        }
        let active = self
            .rust()
            .inbound_transfer
            .as_ref()
            .is_some_and(|ui| matches!(ui.phase, "receiving" | "importing" | "verifying"));
        if active {
            return;
        }
        if let Some(ui) = self.as_mut().rust_mut().inbound_transfer.as_mut() {
            ui.dismissed = true;
        }
        self.as_mut().publish_inbound_key_transfer();
    }

    fn track_inbound_key_transfer(
        mut self: Pin<&mut Self>,
        transfers: &BTreeMap<DeviceId, (u64, kryptopass_devices::SyntheticTransferProgress)>,
    ) {
        use kryptopass_devices::{InboundTransferFailure, InboundTransferPhase};

        let mut changed = false;
        for (peer, (generation, progress)) in transfers {
            let Some(inbound) = progress.inbound else {
                continue;
            };
            let known = self.rust().inbound_transfer.as_ref().is_some_and(|ui| {
                ui.peer == *peer
                    && ui.generation == *generation
                    && ui.transfer_id == inbound.transfer_id
            });
            if !known {
                if inbound.phase != InboundTransferPhase::AwaitingDecision {
                    continue;
                }
                let id = peer.to_string();
                let device_name = self
                    .rust()
                    .aliases
                    .get(peer)
                    .filter(|alias| !alias.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("Dispositivo {}", &id[..12.min(id.len())]));
                tracing::info!(
                    total_size = inbound.total_size,
                    "Inbound key transfer: offer shown to the user, assessing"
                );
                self.as_mut().rust_mut().inbound_transfer = Some(InboundTransferUi {
                    peer: *peer,
                    generation: *generation,
                    transfer_id: inbound.transfer_id,
                    fingerprint: inbound.fingerprint,
                    device_name,
                    assessment: None,
                    phase: "assessing",
                    received: 0,
                    total: inbound.total_size,
                    message: String::from("Revisando la clave ofrecida…"),
                    error: false,
                    decision_sent: false,
                    import_started: false,
                    dismissed: false,
                });
                self.as_mut().spawn_inbound_assessment();
                changed = true;
            }
            let mut start_import = false;
            if let Some(ui) = self.as_mut().rust_mut().inbound_transfer.as_mut() {
                if ui.peer != *peer || ui.generation != *generation {
                    continue;
                }
                let previous = ui.phase;
                ui.received = inbound.received;
                ui.total = inbound.total_size;
                match inbound.phase {
                    InboundTransferPhase::AwaitingDecision => {}
                    InboundTransferPhase::Receiving => {
                        ui.phase = "receiving";
                        ui.message = String::from("Recibiendo la clave del teléfono…");
                    }
                    InboundTransferPhase::Importing => {
                        if !ui.import_started {
                            ui.import_started = true;
                            ui.phase = "importing";
                            ui.message = String::from(
                                "Clave recibida y verificada (SHA-256). Importando en GnuPG…",
                            );
                            start_import = true;
                        }
                    }
                    InboundTransferPhase::Complete => {
                        ui.phase = "complete";
                        ui.error = false;
                        ui.message = String::from(
                            "Clave importada en GnuPG y comprobada con el desafío. El teléfono recibió la confirmación.",
                        );
                    }
                    InboundTransferPhase::Rejected => {
                        ui.phase = "rejected";
                        if ui.message.is_empty() || !ui.error {
                            ui.message =
                                String::from("Transferencia rechazada. No se recibió nada.");
                        }
                    }
                    InboundTransferPhase::Cancelled => {
                        ui.phase = "cancelled";
                        ui.error = true;
                        ui.message = String::from(
                            "El teléfono canceló la transferencia. Se descartó lo recibido.",
                        );
                    }
                    InboundTransferPhase::Expired => {
                        ui.phase = "expired";
                        ui.error = true;
                        ui.message = String::from(
                            "La transferencia caducó (5 minutos desde la oferta). Se descartó lo recibido.",
                        );
                    }
                    InboundTransferPhase::Failed => {
                        if previous != "failed" {
                            ui.phase = "failed";
                            // A local import failure already set a precise message.
                            if !ui.error {
                                ui.error = true;
                                ui.message = String::from(match inbound.failure {
                                    Some(InboundTransferFailure::Protocol) => {
                                        "La transferencia no respetó el orden o el tamaño anunciado. Se descartó lo recibido."
                                    }
                                    Some(InboundTransferFailure::Digest) => {
                                        "La clave recibida no coincide con la oferta (SHA-256). Se descartó."
                                    }
                                    Some(InboundTransferFailure::Remote(_)) => {
                                        "El teléfono interrumpió la transferencia. Se descartó lo recibido."
                                    }
                                    Some(InboundTransferFailure::Disconnected) => {
                                        "Se perdió la conexión antes de terminar. Se descartó lo recibido."
                                    }
                                    Some(InboundTransferFailure::Local) | None => {
                                        "No se pudo importar o comprobar la clave. Se descartó lo recibido."
                                    }
                                });
                            }
                        }
                    }
                }
                if ui.phase != previous {
                    tracing::info!(
                        from = previous,
                        to = ui.phase,
                        received = ui.received,
                        total = ui.total,
                        failure = ?inbound.failure,
                        "Inbound key transfer: UI phase changed"
                    );
                }
                changed = true;
            }
            if start_import {
                self.as_mut().spawn_inbound_import();
            }
        }
        if changed {
            self.as_mut().publish_inbound_key_transfer();
        }
    }

    fn spawn_inbound_assessment(self: Pin<&mut Self>) {
        let Some(ui) = self.rust().inbound_transfer.as_ref() else {
            return;
        };
        let (peer, generation, transfer_id, fingerprint) =
            (ui.peer, ui.generation, ui.transfer_id, ui.fingerprint);
        let owner = self.rust().resume_owner.clone();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let assessment = super::synthetic_key_transfer::assess_inbound(&fingerprint);
            tracing::info!(
                assessment = assessment.code(),
                "Inbound key transfer: offer assessed"
            );
            if !assessment.allows_import() {
                let sent = owner.as_ref().and_then(|owner| {
                    with_inbound_control(owner, peer, generation, |control| {
                        control.decide_inbound_key_transfer(generation, transfer_id, false)
                    })
                });
                match sent {
                    Some(Ok(())) => tracing::warn!(
                        assessment = assessment.code(),
                        "Inbound key transfer: rejected automatically"
                    ),
                    other => tracing::warn!(
                        assessment = assessment.code(),
                        result = ?other.map(|result| result.err()),
                        "Inbound key transfer: automatic rejection could not be sent"
                    ),
                }
            }
            let _ = thread.queue(move |mut object| {
                object.as_mut().update_inbound(transfer_id, |ui| {
                    if ui.generation != generation {
                        return;
                    }
                    ui.assessment = Some(assessment);
                    if assessment.allows_import() {
                        if ui.phase == "assessing" {
                            ui.phase = "waiting_decision";
                            ui.message = String::new();
                        }
                    } else {
                        ui.decision_sent = true;
                        ui.error = true;
                        ui.message = assessment.message().to_owned();
                    }
                });
            });
        });
    }

    fn decide_inbound_key_transfer(mut self: Pin<&mut Self>, accepted: bool) {
        let Some(ui) = self.rust().inbound_transfer.as_ref() else {
            return;
        };
        let allowed = ui.assessment.is_some_and(|value| value.allows_import());
        if ui.decision_sent
            || !matches!(ui.phase, "assessing" | "waiting_decision")
            || (accepted && (ui.phase != "waiting_decision" || !allowed))
        {
            tracing::info!(
                accepted,
                phase = ui.phase,
                "Inbound key transfer: decision ignored in this state"
            );
            return;
        }
        let (peer, generation, transfer_id) = (ui.peer, ui.generation, ui.transfer_id);
        let Some(owner) = self.rust().resume_owner.clone() else {
            return;
        };
        if let Some(ui) = self.as_mut().rust_mut().inbound_transfer.as_mut() {
            ui.decision_sent = true;
            ui.error = false;
            ui.message = String::from(if accepted {
                "Aceptada. Esperando la clave del teléfono…"
            } else {
                "Rechazaste la transferencia. No se recibirá nada."
            });
        }
        tracing::info!(accepted, "Inbound key transfer: local decision");
        self.as_mut().publish_inbound_key_transfer();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = with_inbound_control(&owner, peer, generation, |control| {
                control.decide_inbound_key_transfer(generation, transfer_id, accepted)
            });
            if matches!(result, Some(Ok(()))) {
                return;
            }
            tracing::warn!(
                accepted,
                result = ?result.map(|result| result.err()),
                "Inbound key transfer: decision could not be sent"
            );
            let _ = thread.queue(move |mut object| {
                object.as_mut().update_inbound(transfer_id, |ui| {
                    ui.error = true;
                    ui.message = String::from(
                        "No se pudo enviar tu decisión: la sesión cambió o la oferta caducó.",
                    );
                });
            });
        });
    }

    fn spawn_inbound_import(self: Pin<&mut Self>) {
        let Some(ui) = self.rust().inbound_transfer.as_ref() else {
            return;
        };
        let (peer, generation, transfer_id) = (ui.peer, ui.generation, ui.transfer_id);
        let Some(owner) = self.rust().resume_owner.clone() else {
            tracing::warn!("Inbound key transfer: no session owner to take the received key from");
            return;
        };
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let stage_thread = thread.clone();
            let outcome = with_inbound_control(&owner, peer, generation, |control| {
                let material = match control.take_inbound_key_material(generation, transfer_id) {
                    Ok(material) => material,
                    Err(error) => {
                        tracing::warn!(error = ?error, "Inbound key transfer: received key not available for import");
                        return Err(None);
                    }
                };
                tracing::info!(
                    size = material.protected_key().len(),
                    challenge_bytes = material.encrypted_challenge().len(),
                    "Inbound key transfer: validating received block"
                );
                let result = super::synthetic_key_transfer::import_inbound(&material, |stage| {
                    let _ = stage_thread.queue(move |mut object| {
                        object.as_mut().update_inbound(transfer_id, |ui| {
                            if ui.error || ui.phase == "complete" {
                                return;
                            }
                            ui.phase = stage;
                            ui.message = String::from(if stage == "verifying" {
                                "Clave importada. Comprobando que descifra el desafío; GnuPG puede pedir su contraseña…"
                            } else {
                                "Importando la clave en GnuPG…"
                            });
                        });
                    });
                });
                // Wipes the received block before reporting the result.
                drop(material);
                let challenge_sha256 = result.as_ref().ok().copied();
                if let Err(error) =
                    control.finish_inbound_key_transfer(generation, transfer_id, challenge_sha256)
                {
                    tracing::warn!(error = ?error, "Inbound key transfer: result could not reach the session");
                }
                result.map(|_| ()).map_err(Some)
            });
            match &outcome {
                Some(Ok(())) => {
                    tracing::info!("Inbound key transfer: import and challenge verified")
                }
                Some(Err(Some(failure))) => tracing::warn!(
                    stage = failure.stage,
                    reason = failure.reason,
                    "Inbound key transfer: import failed"
                ),
                Some(Err(None)) | None => {
                    tracing::warn!("Inbound key transfer: import did not start, session changed")
                }
            }
            let _ = thread.queue(move |mut object| {
                object.as_mut().update_inbound(transfer_id, |ui| match outcome {
                    Some(Ok(())) => {
                        if ui.phase != "complete" {
                            ui.message = String::from(
                                "Clave importada y comprobada. Enviando la confirmación al teléfono…",
                            );
                        }
                    }
                    Some(Err(Some(failure))) => {
                        ui.phase = "failed";
                        ui.error = true;
                        ui.message = failure.message().to_owned();
                    }
                    Some(Err(None)) | None => {
                        ui.phase = "failed";
                        ui.error = true;
                        ui.message = String::from(
                            "La sesión cambió antes de importar. No se importó nada.",
                        );
                    }
                });
            });
        });
    }

    /// Applies `update` to the UI state of `transfer_id`, if it is still shown.
    fn update_inbound(
        mut self: Pin<&mut Self>,
        transfer_id: [u8; 16],
        update: impl FnOnce(&mut InboundTransferUi),
    ) {
        let applied = if let Some(ui) = self.as_mut().rust_mut().inbound_transfer.as_mut() {
            if ui.transfer_id == transfer_id {
                update(ui);
                true
            } else {
                false
            }
        } else {
            false
        };
        if applied {
            self.publish_inbound_key_transfer();
        }
    }

    fn publish_inbound_key_transfer(mut self: Pin<&mut Self>) {
        let value = match self.rust().inbound_transfer.as_ref() {
            Some(ui) => {
                let fingerprint = super::synthetic_key_transfer::fingerprint_hex(&ui.fingerprint);
                let assessment = ui.assessment;
                serde_json::json!({
                    "visible": !ui.dismissed,
                    "deviceId": ui.peer.to_string(),
                    "deviceName": ui.device_name,
                    "transferId": hex_transfer_id(&ui.transfer_id),
                    "fingerprint": grouped_fingerprint(&fingerprint),
                    "assessment": assessment.map_or("pending", |value| value.code()),
                    "assessmentMessage": assessment.map_or("", |value| value.message()),
                    "phase": ui.phase,
                    "received": ui.received,
                    "total": ui.total,
                    "message": ui.message,
                    "error": ui.error,
                    "canDecide": ui.phase == "waiting_decision"
                        && !ui.decision_sent
                        && assessment.is_some_and(|value| value.allows_import()),
                    "finished": matches!(ui.phase, "complete" | "rejected" | "cancelled" | "expired" | "failed"),
                })
            }
            None => serde_json::json!({ "visible": false }),
        };
        self.as_mut()
            .set_inbound_key_transfer(QString::from(value.to_string()));
    }
}
