//! Explicit emulator pairing UI. Private identities remain in the worker/OS store.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_pairing::LocalDecision;
use kryptopass_pairing_lab::{HostEvent, LabHost, LabNetwork};
use std::{net::IpAddr, path::PathBuf, pin::Pin};
use tokio::sync::{mpsc, oneshot, watch};

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
        type PairingLabController = super::PairingLabControllerRust;

        #[qinvokable]
        #[cxx_name = "loadDevices"]
        fn load_devices(self: Pin<&mut Self>);
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
        #[cxx_name = "startPairing"]
        fn start_pairing(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "startReconnect"]
        fn start_reconnect(self: Pin<&mut Self>);
        #[qinvokable]
        fn decide(self: Pin<&mut Self>, accepted: bool);
        #[qinvokable]
        fn cancel(self: Pin<&mut Self>);
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
    network_addresses: QStringList,
    network_index: i32,
    network_ips: Vec<IpAddr>,
    status: QString,
    bootstrap: QString,
    sas: QString,
    peer: QString,
    pending: Option<PendingOperation>,
    known_peers: Vec<PublicIdentity>,
    generation: u64,
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
            network_addresses: QStringList::default(),
            network_index: -1,
            network_ips: Vec::new(),
            status: QString::from("Listo para una prueba local con el emulador."),
            bootstrap: QString::default(),
            sas: QString::default(),
            peer: QString::default(),
            pending: None,
            known_peers: Vec::new(),
            generation: 0,
        }
    }
}

struct PendingOperation {
    cancel: watch::Sender<bool>,
    decision: Option<oneshot::Sender<LocalDecision>>,
}

impl Drop for PendingOperation {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
    }
}

impl qobject::PairingLabController {
    pub fn load_network_addresses(mut self: Pin<&mut Self>) {
        if !cfg!(debug_assertions) || *self.busy() {
            return;
        }
        let addresses = kryptopass_pairing_lab::lan_addresses().unwrap_or_default();
        let index = addresses
            .iter()
            .position(|ip| !ip.is_loopback())
            .or_else(|| (!addresses.is_empty()).then_some(0));
        let display = addresses
            .iter()
            .map(|ip| QString::from(ip.to_string()))
            .collect();
        self.as_mut().rust_mut().network_ips = addresses;
        self.as_mut().set_network_addresses(display);
        self.as_mut().set_network_index(
            index
                .and_then(|index| i32::try_from(index).ok())
                .unwrap_or(-1),
        );
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
        if !cfg!(debug_assertions) || *self.busy() {
            return;
        }
        let preferred = self.rust().selected_peer().map(|peer| peer.device_id);
        self.as_mut().replace_trusted_devices(Vec::new(), None);
        let directory = match lab_directory() {
            Ok(path) => path,
            Err(()) => {
                self.as_mut()
                    .set_status(QString::from("No se pudo localizar el perfil de pruebas."));
                return;
            }
        };
        let (cancel, cancelled) = watch::channel(false);
        self.as_mut().rust_mut().generation = self.rust().generation.wrapping_add(1);
        let generation = self.rust().generation;
        self.as_mut().rust_mut().pending = Some(PendingOperation {
            cancel,
            decision: None,
        });
        self.as_mut().set_busy(true);
        self.as_mut().set_status(QString::from(
            "Cargando dispositivos autorizados del perfil…",
        ));
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = (|| {
                if *cancelled.borrow() {
                    return Err(kryptopass_pairing_lab::LabError::Cancelled);
                }
                if !directory
                    .try_exists()
                    .map_err(|_| kryptopass_pairing_lab::LabError::Profile)?
                {
                    return Ok(Vec::new());
                }
                let host = LabHost::open_existing_system_profile(&directory)?;
                let peers = host.trusted_peers()?;
                if *cancelled.borrow() {
                    return Err(kryptopass_pairing_lab::LabError::Cancelled);
                }
                Ok(peers)
            })();
            let _ = thread.queue(move |mut object| {
                if object.rust().generation != generation { return; }
                object.as_mut().rust_mut().pending = None;
                object.as_mut().set_busy(false);
                match result {
                    Ok(peers) => {
                        let empty = peers.is_empty();
                        tracing::info!(authorized_devices = peers.len(), "Pairing lab: loaded persisted trust");
                        object.as_mut().replace_trusted_devices(peers, preferred);
                        object.as_mut().set_status(QString::from(if empty {
                            "No hay dispositivos autorizados. Crea una invitación para vincular Android."
                        } else {
                            "Dispositivos recuperados. Selecciona uno y pulsa Reconectar; no necesitas repetir los emojis."
                        }));
                    }
                    Err(error) => object.as_mut().set_status(QString::from(error_status(&error))),
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
    pub fn start_reconnect(self: Pin<&mut Self>) {
        self.start(true);
    }

    fn start(mut self: Pin<&mut Self>, reconnect: bool) {
        if !cfg!(debug_assertions) || *self.busy() {
            return;
        }
        let expected = if reconnect {
            self.rust().selected_peer()
        } else {
            None
        };
        if reconnect && expected.is_none() {
            return;
        }
        let network = if *self.lan_enabled() {
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
        let (decision, decisions) = oneshot::channel();
        self.as_mut().rust_mut().generation = self.rust().generation.wrapping_add(1);
        let generation = self.rust().generation;
        self.as_mut().rust_mut().pending = Some(PendingOperation {
            cancel,
            decision: Some(decision),
        });
        self.as_mut().set_busy(true);
        self.as_mut().set_awaiting(false);
        self.as_mut().set_bootstrap(QString::default());
        self.as_mut().set_sas(QString::default());
        self.as_mut().set_status(QString::from(
            "Abriendo identidad protegida por el llavero del sistema…",
        ));
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = (|| {
                let host = if reconnect {
                    LabHost::open_existing_system_profile(&directory)?
                } else {
                    LabHost::open_system_profile(&directory)?
                };
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| kryptopass_pairing_lab::LabError::Link)?;
                runtime.block_on(async {
                    let (events, mut received) = mpsc::channel(8);
                    let operation = host.run_once_with_network(expected, events, decisions, cancelled, network);
                    tokio::pin!(operation);
                    loop {
                        tokio::select! {
                            result = &mut operation => break result.and_then(|peer| Ok((peer, host.trusted_peers()?))),
                            event = received.recv() => {
                                let Some(event) = event else { continue; };
                                let _ = thread.queue(move |mut object| {
                                    if object.rust().generation != generation { return; }
                                    match event {
                                        HostEvent::Bootstrap(code) => {
                                            object.as_mut().set_bootstrap(QString::from(code));
                                            object.as_mut().set_status(QString::from("Introduce este código en Android → Ajustes → Emparejar con Desktop."));
                                        }
                                        HostEvent::Sas(sas) => {
                                            let display = sas.emojis.iter().enumerate()
                                                .map(|(index, entry)| format!("{}. {}  {}", index + 1, entry.emoji, entry.name))
                                                .collect::<Vec<_>>().join("\n");
                                            object.as_mut().set_sas(QString::from(display));
                                            object.as_mut().set_awaiting(true);
                                            object.as_mut().set_bootstrap(QString::default());
                                            object.as_mut().set_status(QString::from("Compara los siete emojis con Android y confirma en ambas pantallas."));
                                        }
                                    }
                                });
                            }
                        }
                    }
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
                match result {
                    Ok((peer, peers)) => {
                        object
                            .as_mut()
                            .replace_trusted_devices(peers, Some(peer.device_id));
                        object.as_mut().set_status(QString::from(if reconnect {
                            "Reconexión autenticada con claves de sesión nuevas."
                        } else {
                            "Dispositivo verificado. Confianza guardada en este perfil de pruebas."
                        }));
                    }
                    Err(error) => {
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
    }

    pub fn cancel(mut self: Pin<&mut Self>) {
        if let Some(operation) = self.as_mut().rust_mut().pending.take() {
            let _ = operation.cancel.send(true);
            let generation = self.rust().generation.wrapping_add(1);
            self.as_mut().rust_mut().generation = generation;
            self.as_mut().set_busy(false);
            self.as_mut().set_status(QString::from(
                "Operación cancelada. La confianza ya guardada se conserva.",
            ));
        }
        self.as_mut().set_awaiting(false);
        self.as_mut().set_bootstrap(QString::default());
        self.as_mut().set_sas(QString::default());
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
        _ => {
            "No se pudo autenticar o guardar el dispositivo. Comprueba el perfil y genera otro código para reintentar."
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
notifying_property!(set_bootstrap, bootstrap, bootstrap_changed, QString);
notifying_property!(set_sas, sas, sas_changed, QString);
notifying_property!(set_peer, peer, peer_changed, QString);

fn lab_directory() -> Result<PathBuf, ()> {
    crate::storage::paths::browser_state_directory(crate::app_info::APP.desktop_file_id)
        .map(|parent| parent.join("emulator-pairing-lab-v1"))
        .map_err(|_| ())
}
