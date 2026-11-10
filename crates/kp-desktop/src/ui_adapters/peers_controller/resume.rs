//! The process-owned listener that keeps paired devices reconnecting while the app runs.

use super::*;

impl qobject::PeersController {
    /// Starts the process-owned durable listener independently of the dialog.
    /// Opening a missing profile is intentionally left to explicit pairing.
    pub fn start_resume_owner(mut self: Pin<&mut Self>) {
        if self.rust().resume_owner.is_some() {
            return;
        }
        let directory = match peers_directory() {
            Ok(directory) => directory,
            Err(()) => return,
        };
        let owner = match spawn_resume_process_owner(
            directory.clone(),
            vault_sync::sync_binder(directory.clone()),
        ) {
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
                if matches!(
                    PeerHost::system_profile_needs_pairing(&directory),
                    Ok(false)
                ) {
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

    pub(super) fn stop_resume_owner(mut self: Pin<&mut Self>) -> Option<ResumeProcessOwner> {
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
                if progress.phase == kryptopass_devices::SyntheticTransferPhase::Complete {
                    self.as_mut().sync_key_after_send(*peer);
                }
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
}
