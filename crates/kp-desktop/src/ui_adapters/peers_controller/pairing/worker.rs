//! The worker thread of a pairing or explicit reconnect: it opens the profile,
//! runs the handshake, keeps the session until it closes, and mirrors each step
//! to the controller.

use super::*;
use cxx_qt::CxxQtThread;
use kryptopass_devices::SessionHandle;
use kryptopass_pairing::PairingOutcome;
use kryptopass_peers::{HostTransport, PeerError};

type Controller<'a> = Pin<&'a mut qobject::PeersController>;

/// Everything the worker needs, captured on the UI thread before it starts.
pub(super) struct PairingJob {
    pub(super) reconnect: bool,
    pub(super) directory: PathBuf,
    pub(super) expected: Option<PublicIdentity>,
    pub(super) network: PeerNetwork,
    pub(super) guard: NetworkPolicyGuard,
    pub(super) owner: ResumeProcessOwner,
    pub(super) cancelled: watch::Receiver<bool>,
    pub(super) generation: u64,
    pub(super) thread: CxxQtThread<qobject::PeersController>,
}

/// How a session that did open came to its end.
struct Ended {
    peer: DeviceId,
    devices: DeviceDirectory,
    last_exit: Option<SessionEvent>,
}

impl PairingJob {
    /// Runs to the end of the session and reports the result to the controller.
    pub(super) fn run(self, decisions: oneshot::Receiver<LocalDecision>) {
        let result = self.work(decisions);
        self.on_ui(move |object| show_end(object, result));
    }

    /// Queues `update` on the UI thread; it is dropped if the controller has
    /// moved on to another operation meanwhile.
    fn on_ui(&self, update: impl FnOnce(Controller<'_>) + Send + 'static) {
        let generation = self.generation;
        let _ = self.thread.queue(move |object| {
            if object.rust().generation == generation {
                update(object);
            }
        });
    }

    fn work(&self, decisions: oneshot::Receiver<LocalDecision>) -> Result<Ended, PeerError> {
        let host = (if self.reconnect {
            PeerHost::open_existing_system_profile(&self.directory)
        } else {
            // This is the only path that may initialize a profile, and it
            // is reached only after the user explicitly starts pairing.
            PeerHost::open_system_profile(&self.directory)
        })
        .inspect_err(|error| {
            tracing::warn!(
                phase = "open_pairing_profile",
                reason = error_code(error),
                reconnect = self.reconnect,
                "Device link setup failed"
            );
        })?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| PeerError::Link)?;
        runtime.block_on(self.session(&host, decisions))
    }

    async fn session(
        &self,
        host: &PeerHost,
        decisions: oneshot::Receiver<LocalDecision>,
    ) -> Result<Ended, PeerError> {
        let outcome = self.handshake(host, decisions).await?;
        let peer = outcome.peer;
        let ready = self.owner.refresh_trust().await?;
        if !ready.trusted_peers.contains(&peer) {
            return Err(PeerError::Authentication);
        }
        let reconnect_port = if matches!(
            &self.network,
            PeerNetwork::Lan { .. } | PeerNetwork::Auto { .. }
        ) {
            ready.reconnect_port
        } else {
            0
        };
        let devices = ready.devices.clone();
        let mut handle = host
            .start_device_session(
                devices.clone(),
                outcome.session,
                peer,
                SessionOptions { reconnect_port },
                vault_sync::sync_binding(&self.directory, peer.device_id),
            )
            .map_err(|_| PeerError::Session)?;
        let session_generation = handle.generation();
        self.owner
            .register_synthetic_transfer_control(handle.synthetic_key_transfer_control())
            .await
            .map_err(|_| PeerError::Session)?;
        let changes = devices.subscribe();
        let opened_devices = devices.clone();
        let peers = ready.trusted_peers;
        self.on_ui(move |object| show_session_opened(object, peer, opened_devices, peers));
        let lifecycle = handle.subscribe_lifecycle();
        self.follow(&mut handle, peer, changes).await;
        let last_exit = *lifecycle.borrow();
        self.owner
            .unregister_synthetic_transfer_control(peer.device_id, session_generation);
        Ok(Ended {
            peer: peer.device_id,
            devices,
            last_exit,
        })
    }

    /// Pairs or reconnects, showing the invitation and the SAS as they arrive.
    async fn handshake(
        &self,
        host: &PeerHost,
        decisions: oneshot::Receiver<LocalDecision>,
    ) -> Result<PairingOutcome<HostTransport>, PeerError> {
        let (events, mut received) = mpsc::channel(8);
        let operation = host.open_session_with_network_guard(
            self.expected,
            events,
            decisions,
            self.cancelled.clone(),
            self.network.clone(),
            self.guard.clone(),
        );
        tokio::pin!(operation);
        loop {
            tokio::select! {
                result = &mut operation => return result,
                Some(event) = received.recv() => {
                    self.on_ui(move |object| show_host_event(object, event));
                }
            }
        }
    }

    /// Mirrors the device directory until the session closes or the user cancels.
    async fn follow(
        &self,
        handle: &mut SessionHandle,
        peer: PublicIdentity,
        mut changes: watch::Receiver<Arc<DeviceDirectorySnapshot>>,
    ) {
        let mut cancelled = self.cancelled.clone();
        loop {
            if *cancelled.borrow() {
                break;
            }
            tokio::select! {
                () = handle.wait_closed() => return,
                _ = cancelled.changed() => break,
                changed = changes.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let snapshot = changes.borrow_and_update().clone();
                    self.on_ui(move |mut object| {
                        object.as_mut().apply_devices_snapshot(&snapshot);
                        if let Some(device) = snapshot.get(&peer.device_id) {
                            object.show(
                                connection_code(device.connection),
                                connection_status(device.connection),
                            );
                        }
                    });
                }
            }
        }
        let _ = handle.disconnect(SessionCloseReason::UserRequested);
        handle.wait_closed().await;
    }
}

fn show_host_event(mut object: Controller<'_>, event: HostEvent) {
    match event {
        HostEvent::Bootstrap(code) => {
            object.as_mut().set_bootstrap(QString::from(code));
            object.show(
                "waiting_peer",
                "Escanea el QR desde Android o copia el código de conexión.",
            );
        }
        HostEvent::Sas(sas) => {
            let emojis = sas
                .emojis
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    serde_json::json!({
                        "index": index + 1,
                        "emoji": entry.emoji,
                        "name": entry.name,
                    })
                })
                .collect::<Vec<_>>();
            object
                .as_mut()
                .set_sas_emojis(QString::from(serde_json::Value::from(emojis).to_string()));
            let display = sas
                .emojis
                .iter()
                .enumerate()
                .map(|(index, entry)| format!("{}. {}  {}", index + 1, entry.emoji, entry.name))
                .collect::<Vec<_>>()
                .join("\n");
            object.as_mut().set_sas(QString::from(display));
            object.as_mut().set_awaiting(true);
            object.as_mut().set_bootstrap(QString::default());
            object.show(
                "awaiting_sas",
                "Compara los siete emojis con Android y confirma en ambas pantallas.",
            );
        }
    }
}

/// The handshake is done and trusted: the pending operation becomes the
/// active session and can no longer be decided, only cancelled.
fn show_session_opened(
    mut object: Controller<'_>,
    peer: PublicIdentity,
    devices: DeviceDirectory,
    peers: Vec<PublicIdentity>,
) {
    let mut pending = object.as_mut().rust_mut().pending.take();
    if let Some(pending) = pending.as_mut() {
        pending.decision = None;
    }
    object.as_mut().rust_mut().active = pending;
    object.as_mut().set_session_active(true);
    object
        .as_mut()
        .set_active_device_id(QString::from(peer.device_id.to_string()));
    object.as_mut().rust_mut().devices = Some(devices.clone());
    object.as_mut().set_busy(false);
    object.as_mut().set_awaiting(false);
    object.as_mut().set_bootstrap(QString::default());
    object
        .as_mut()
        .replace_trusted_devices(peers, Some(peer.device_id));
    object.as_mut().apply_devices_snapshot(&devices.snapshot());
    object.show(
        "connecting",
        "Confianza guardada. Abriendo la sesión autenticada…",
    );
}

fn show_end(mut object: Controller<'_>, result: Result<Ended, PeerError>) {
    object.as_mut().set_busy(false);
    object.as_mut().set_awaiting(false);
    object.as_mut().set_bootstrap(QString::default());
    object.as_mut().rust_mut().pending = None;
    object.as_mut().rust_mut().active = None;
    object.as_mut().rust_mut().active_network_guard = None;
    object.as_mut().set_session_active(false);
    object.as_mut().set_active_device_id(QString::default());
    match result {
        Ok(ended) => show_closed_session(object, &ended),
        Err(error) => show_failed_setup(object, &error),
    }
}

fn show_closed_session(mut object: Controller<'_>, ended: &Ended) {
    let snapshot = ended.devices.snapshot();
    object.as_mut().apply_devices_snapshot(&snapshot);
    let state = snapshot
        .get(&ended.peer)
        .map_or(ConnectionState::Disconnected, |device| device.connection);
    object
        .as_mut()
        .show(connection_code(state), connection_status(state));
    if let Some(SessionEvent::Failed { failure, .. }) = ended.last_exit {
        if matches!(
            failure,
            SessionFailure::Authentication
                | SessionFailure::HelloRejected
                | SessionFailure::Protocol
                | SessionFailure::ControlRate
                | SessionFailure::NonceSequence
        ) {
            object.show(
                "authentication_error",
                "La sesión no superó la comprobación de seguridad. La confianza guardada se conserva, pero la conexión se detuvo.",
            );
        }
    }
}

/// Nothing opened, so whatever the directory still shows as connecting failed.
fn show_failed_setup(mut object: Controller<'_>, error: &PeerError) {
    object
        .as_mut()
        .set_status_code(QString::from(error_code(error)));
    if let Some(devices) = object.rust().devices.clone() {
        for device in &devices.snapshot().devices {
            if matches!(
                device.connection,
                ConnectionState::Connecting | ConnectionState::Reconnecting
            ) {
                let _ = devices
                    .update_connection_state(device.identity.device_id, ConnectionState::Failed);
            }
        }
        object.as_mut().apply_devices_snapshot(&devices.snapshot());
    }
    object
        .as_mut()
        .set_status(QString::from(error_status(error)));
}
