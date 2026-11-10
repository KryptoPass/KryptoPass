//! Pairing and reconnecting: the worker that runs a session and the local decisions on it.

use super::*;

impl qobject::PeersController {
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
                    .map_err(|_| kryptopass_peers::PeerError::Link)?;
                runtime.block_on(async {
                    let ready = owner.refresh_trust().await?;
                    if !ready.trusted_peers.contains(&expected) {
                        return Err(kryptopass_peers::PeerError::Authentication);
                    }
                    if ready.reconnect_port == 0 {
                        return Err(kryptopass_peers::PeerError::Link);
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
                    Err(kryptopass_peers::PeerError::Authentication) => {
                        object
                            .as_mut()
                            .set_status_code(QString::from("peer_not_authorized"));
                        object.as_mut().set_status(QString::from(
                            "Este dispositivo ya no está autorizado en este equipo.",
                        ));
                    }
                    Err(kryptopass_peers::PeerError::Link) => {
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
        if *self.busy() || self.rust().active.is_some() {
            return;
        }
        let profile_directory = match peers_directory() {
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
                    && network::lan_permitted(
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
            match PeerNetwork::auto(
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
            match PeerNetwork::internet(self.rust().relay_origin.to_string().trim()) {
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
            PeerNetwork::Lan { bind_ip }
        } else {
            PeerNetwork::EmulatorLoopback
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
                    PeerHost::open_existing_system_profile(&directory)
                } else {
                    // This is the only path that may initialize a profile, and it
                    // is reached only after the user explicitly starts pairing.
                    PeerHost::open_system_profile(&directory)
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
                    .map_err(|_| kryptopass_peers::PeerError::Link)?;
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
                        return Err(kryptopass_peers::PeerError::Authentication);
                    }
                    let reconnect_port = if matches!(&network, PeerNetwork::Lan { .. } | PeerNetwork::Auto { .. }) {
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
                        vault_sync::sync_binding(&directory, peer.device_id),
                    )
                    .map_err(|_| kryptopass_peers::PeerError::Session)?;
                    let session_generation = handle.generation();
                    owner
                        .register_synthetic_transfer_control(
                            handle.synthetic_key_transfer_control(),
                        )
                        .await
                        .map_err(|_| kryptopass_peers::PeerError::Session)?;
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

    pub(super) fn set_bootstrap(mut self: Pin<&mut Self>, value: QString) {
        if self.rust().bootstrap == value {
            return;
        }
        let source = if value.is_empty() {
            QString::default()
        } else {
            crate::ui_adapters::secret_qr::source(&value.to_string())
                .map(|source| QString::from(source.as_str()))
                .unwrap_or_default()
        };
        self.as_mut().set_bootstrap_qr(source);
        self.as_mut().rust_mut().bootstrap = value;
        self.as_mut().bootstrap_changed();
    }
}
