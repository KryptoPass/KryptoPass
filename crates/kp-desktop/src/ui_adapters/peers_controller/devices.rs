//! The trusted-device list: loading, selecting, naming, disconnecting and forgetting.

use super::*;

impl qobject::PeersController {
    pub fn load_devices(mut self: Pin<&mut Self>) {
        if self
            .rust()
            .draining_worker
            .as_ref()
            .is_some_and(|done| !done.load(Ordering::Acquire))
        {
            return;
        }
        if *self.busy() || self.rust().alias_busy {
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
        let directory = match peers_directory() {
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
            let transfer_fingerprint = crate::ui_adapters::synthetic_key_transfer::fingerprint();
            let vault_fingerprint =
                crate::ui_adapters::synthetic_key_transfer::vault_identity_fingerprint();
            let result = (|| {
                if *cancelled.borrow() {
                    return Err(kryptopass_peers::PeerError::Cancelled);
                }
                if PeerHost::system_profile_needs_pairing(&directory)? {
                    return Ok(None);
                }
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| kryptopass_peers::PeerError::Link)?;
                let ready = runtime.block_on(owner.ensure_ready())?;
                let host = PeerHost::open_existing_system_profile(&directory)?;
                let aliases = host.device_aliases();
                if *cancelled.borrow() {
                    return Err(kryptopass_peers::PeerError::Cancelled);
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
                        // The peers must be known first: the selection is read per peer.
                        object.as_mut().refresh_vault_sync_peers();
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

    pub(super) fn replace_trusted_devices(
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
        // The vault mirror only runs for phones listed here, so read the Sync selection as
        // soon as the peers are known instead of waiting for the devices window to open.
        self.as_mut().refresh_vault_sync_peers();
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
        if self.rust().alias_busy {
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
        let Ok(directory) = peers_directory() else {
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
                let host = PeerHost::open_existing_system_profile(&directory)
                    .map_err(|_| kryptopass_peers::AliasError::Storage)?;
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
                            kryptopass_peers::AliasError::InvalidName => "Usa hasta 64 caracteres y 256 bytes, sin controles ni saltos de línea.",
                            kryptopass_peers::AliasError::UnknownDevice => "El dispositivo ya no está autorizado.",
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
        if self.rust().maintenance_busy || self.rust().alias_busy {
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
        let Ok(directory) = peers_directory() else {
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
                            return Err(kryptopass_peers::PeerError::Session);
                        }
                        std::thread::sleep(Duration::from_millis(25));
                    }
                }
                if let Some(owner) = resume_owner {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if !owner.shutdown_and_wait(remaining) {
                        return Err(kryptopass_peers::PeerError::Session);
                    }
                }
                let Some(peer) = forget else {
                    PeerHost::reset_system_profile(&directory)?;
                    // The reset may have replaced the identity this process cached.
                    crate::vault_access::forget_identity();
                    return Ok(None);
                };
                let host = PeerHost::open_existing_system_profile(&directory)?;
                host.forget_peer(peer)?;
                if let Err(error) = SyncSelection::in_profile(&directory).forget(&peer.to_string())
                {
                    tracing::warn!(error = %format!("{error:#}"), "Sync selection of a forgotten device kept");
                }
                let peers = host.trusted_peers()?;
                let aliases = host
                    .device_aliases()
                    .map_err(|_| kryptopass_peers::PeerError::Profile)?;
                let devices = DeviceDirectory::new(host.public_identity().device_id);
                devices
                    .replace_trusted(peers.clone())
                    .map_err(|_| kryptopass_peers::PeerError::Profile)?;
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

    pub(super) fn apply_devices_snapshot(
        mut self: Pin<&mut Self>,
        snapshot: &DeviceDirectorySnapshot,
    ) {
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
                // Keys the phone declared for this connection; null while unknown.
                let vault_keys = self
                    .rust()
                    .devices
                    .as_ref()
                    .and_then(|devices| devices.peer_vault_keys(&device.identity.device_id))
                    .map(|keys| {
                        keys.iter()
                            .map(|bytes| bytes.iter().map(|byte| format!("{byte:02X}")).collect())
                            .collect::<Vec<String>>()
                    });
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
                    "syntheticTransferPhase": synthetic_progress.map_or("unsupported", |progress| key_transfer::synthetic_transfer_phase_code(progress.phase)),
                    "syntheticTransferId": synthetic_transfer_id,
                    "keyTransferVersion": synthetic_progress.map_or(0, |progress| progress.version),
                    "keyTransferUnavailableReason": synthetic_progress.map_or(0, |progress| progress.peer_unavailable_reason),
                    "alias": alias,
                    "vaultKeys": vault_keys,
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
            "Device session state updated"
        );
    }
}
