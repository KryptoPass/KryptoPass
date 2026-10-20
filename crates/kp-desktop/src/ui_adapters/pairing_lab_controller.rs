//! Explicit emulator pairing UI. Private identities remain in the worker/OS store.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use kryptopass_devices::{
    ConnectionState, DeviceDirectory, DeviceDirectorySnapshot, SessionCloseReason, SessionEvent,
    SessionFailure, SessionOptions,
};
use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_pairing::LocalDecision;
use kryptopass_pairing_lab::{
    HostEvent, LabHost, LabNetwork, ResumeOwnerSnapshot, ResumeProcessOwner,
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
        #[qsignal]
        #[cxx_name = "aliasSaved"]
        fn alias_saved(self: Pin<&mut Self>, device_id: &QString);
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
    aliases: BTreeMap<DeviceId, String>,
    pending: Option<PendingOperation>,
    active: Option<PendingOperation>,
    draining_worker: Option<Arc<AtomicBool>>,
    devices: Option<DeviceDirectory>,
    known_peers: Vec<PublicIdentity>,
    generation: u64,
    devices_revision: u64,
    resume_owner: Option<ResumeProcessOwner>,
    resume_owner_generation: u64,
    resume_owner_cancel: Option<watch::Sender<bool>>,
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
            internet_enabled: false,
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
            aliases: BTreeMap::new(),
            pending: None,
            active: None,
            draining_worker: None,
            devices: None,
            known_peers: Vec::new(),
            generation: 0,
            devices_revision: 0,
            resume_owner: None,
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
                let _ = owner.ensure_ready().await;
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
                "No se pudo abrir el perfil existente. No se creó ni reparó una identidad automáticamente.",
            ));
        }
    }

    pub fn load_network_addresses(mut self: Pin<&mut Self>) {
        if !cfg!(debug_assertions) || *self.busy() {
            return;
        }
        let previous = usize::try_from(self.rust().network_index)
            .ok()
            .and_then(|index| self.rust().network_ips.get(index))
            .copied();
        let interfaces = kryptopass_lan::local_interfaces().unwrap_or_default();
        let addresses: Vec<_> = interfaces
            .iter()
            .map(|interface| interface.address)
            .collect();
        let index = previous
            .and_then(|previous| addresses.iter().position(|ip| *ip == previous))
            .or_else(|| {
                addresses
                    .iter()
                    .position(|ip| !ip.is_loopback())
                    .or_else(|| (!addresses.is_empty()).then_some(0))
            });
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
        if *self.busy() {
            return;
        }
        if !enabled {
            self.as_mut().set_lan_enabled(false);
            return;
        }
        if usize::try_from(index)
            .ok()
            .is_some_and(|index| index < self.rust().network_ips.len())
        {
            self.as_mut().set_network_index(index);
            self.as_mut().set_lan_enabled(true);
        } else {
            self.as_mut().set_status(QString::from("No hay una dirección LAN seleccionada. Actualiza las interfaces o usa el modo emulador."));
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
                self.as_mut()
                    .set_status(QString::from("No se pudo localizar el perfil de pruebas."));
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
        self.as_mut().set_status(QString::from(
            "Cargando dispositivos autorizados del perfil…",
        ));
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let _completed = WorkerCompletion(worker_completed);
            let result = (|| {
                if *cancelled.borrow() {
                    return Err(kryptopass_pairing_lab::LabError::Cancelled);
                }
                if !directory
                    .try_exists()
                    .map_err(|_| kryptopass_pairing_lab::LabError::Profile)?
                {
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
        if *self.busy() {
            return;
        }
        let selected = usize::try_from(index)
            .ok()
            .and_then(|index| self.rust().known_peers.get(index))
            .copied();
        self.as_mut()
            .set_selected_index(if selected.is_some() { index } else { -1 });
        self.as_mut().set_can_reconnect(selected.is_some());
        self.as_mut()
            .set_peer(selected.map_or_else(QString::default, |peer| {
                QString::from(peer.device_id.to_string())
            }));
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
    }

    pub fn start_pairing(self: Pin<&mut Self>) {
        self.start(false);
    }
    pub fn start_reconnect(mut self: Pin<&mut Self>) {
        if *self.lan_enabled() && !*self.internet_enabled() {
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
        self.as_mut().start_resume_owner();
        let Some(owner) = self.rust().resume_owner.clone() else {
            self.as_mut()
                .set_status_code(QString::from("profile_error"));
            self.as_mut().set_status(QString::from(
                "No se pudo iniciar el listener de recuperación local.",
            ));
            return;
        };
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
        let network = if *self.internet_enabled() {
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
        let directory = match lab_directory() {
            Ok(path) => path,
            Err(()) => {
                self.as_mut()
                    .set_status(QString::from("No se pudo abrir el perfil de pruebas."));
                return;
            }
        };
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
                let host = if reconnect {
                    LabHost::open_existing_system_profile(&directory)?
                } else {
                    // This is the only path that may initialize a profile, and it
                    // is reached only after the user explicitly starts pairing.
                    LabHost::open_system_profile(&directory)?
                };
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| kryptopass_pairing_lab::LabError::Link)?;
                runtime.block_on(async {
                    let (events, mut received) = mpsc::channel(8);
                    let mut cancelled = cancelled;
                    let outcome = {
                        let operation = host.open_session_with_network(
                            expected,
                            events,
                            decisions,
                            cancelled.clone(),
                            network.clone(),
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
                    let local_id = host.public_identity().device_id;
                    let reconnect_port = if matches!(&network, LabNetwork::Lan { .. }) {
                        ready.reconnect_port
                    } else {
                        0
                    };
                    let options = SessionOptions { reconnect_port };
                    let devices = ready.devices.clone();
                    let mut handle = kryptopass_devices::start_session_with_options(
                        devices.clone(),
                        outcome.session,
                        local_id,
                        peer,
                        options,
                    )
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
        if let Some(owner) = self.rust().resume_owner.as_ref() {
            owner.suppress_peer(peer);
        }
        if device_id == self.rust().active_device_id {
            self.as_mut().disconnect_device();
        }
    }

    /// Detaches the view while keeping an established connection owned by the app.
    pub fn release_ui(mut self: Pin<&mut Self>) {
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
            return;
        }
        self.as_mut().maintain_profile(Some(id));
    }

    /// Resets the isolated pairing identity and trust, never the password vault.
    pub fn reset_pairing_profile(self: Pin<&mut Self>) {
        self.maintain_profile(None);
    }

    fn maintain_profile(mut self: Pin<&mut Self>, forget: Option<DeviceId>) {
        if !cfg!(debug_assertions) || self.rust().maintenance_busy || self.rust().alias_busy {
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
                    Err(_) => {
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

fn error_status(error: &kryptopass_pairing_lab::LabError) -> &'static str {
    use kryptopass_pairing_lab::LabError;
    match error {
        LabError::Cancelled => "Operación cancelada.",
        LabError::Expired => "La invitación o conexión caducó. Genera otro código.",
        LabError::Rejected => "Emparejamiento rechazado. No se concedió confianza.",
        LabError::KeyStore => {
            "El llavero del sistema no permitió abrir la identidad. No se creó un sustituto."
        }
        LabError::Profile => {
            "El perfil de pruebas falta o no es válido. No se reparó automáticamente."
        }
        LabError::Session => {
            "La confianza se guardó, pero no se pudo abrir la sesión activa. Puedes reconectar."
        }
        LabError::Link => {
            "El teléfono no completó la conexión de red. Comprueba su error y genera otra invitación; todavía no se verificaron identidades."
        }
        LabError::Authentication => {
            "La verificación criptográfica no se completó. No se autorizó el dispositivo. Genera una invitación nueva."
        }
    }
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
notifying_property!(set_lan_enabled, lan_enabled, lan_enabled_changed, bool);
notifying_property!(
    set_internet_enabled,
    internet_enabled,
    internet_enabled_changed,
    bool
);
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
