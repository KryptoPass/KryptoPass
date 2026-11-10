//! Pairing and reconnecting: what the UI thread checks and prepares before a
//! session worker starts, and the local decisions on a running one.

use super::*;

mod worker;

use worker::PairingJob;

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

    /// Makes the recovery listener ready for the selected device; the phone
    /// then reconnects on its own.
    fn start_durable_reconnect(mut self: Pin<&mut Self>) {
        if self.worker_draining()
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
        let Some(owner) = self.as_mut().require_resume_owner() else {
            return;
        };

        self.as_mut().rust_mut().generation = self.rust().generation.wrapping_add(1);
        let generation = self.rust().generation;
        self.as_mut().set_busy(true);
        self.as_mut().clear_pairing_display();
        self.as_mut().show(
            "preparing_reconnect",
            "Preparando la reconexión LAN con este dispositivo…",
        );

        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = allow_reconnect(&owner, expected);
            let _ = thread.queue(move |mut object| {
                if object.rust().generation != generation {
                    return;
                }
                object.as_mut().set_busy(false);
                let (code, text) = match result {
                    Ok(()) => (
                        "resume_ready",
                        "Listener LAN listo. Abre KryptoPass en Android y pulsa Reconectar para este Desktop.",
                    ),
                    Err(kryptopass_peers::PeerError::Authentication) => (
                        "peer_not_authorized",
                        "Este dispositivo ya no está autorizado en este equipo.",
                    ),
                    Err(kryptopass_peers::PeerError::Link) => (
                        "lan_error",
                        "No se pudo iniciar el listener LAN. Revisa la red e inténtalo de nuevo.",
                    ),
                    Err(_) => (
                        "profile_error",
                        "No se pudo abrir el perfil existente. No se creó una identidad nueva.",
                    ),
                };
                object.show(code, text);
            });
        });
    }

    /// Pairs a new device, or reconnects the selected one over the explicit flow.
    fn start(mut self: Pin<&mut Self>, reconnect: bool) {
        if self.worker_draining() || *self.busy() || self.rust().active.is_some() {
            return;
        }
        let Some((directory, guard)) = self.as_mut().network_policy_guard() else {
            return;
        };
        self.as_mut().rust_mut().active_network_guard = Some(guard.clone());
        let Some(owner) = self.as_mut().require_resume_owner() else {
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
        let Some(network) = self.as_mut().select_network() else {
            return;
        };

        let (cancel, cancelled) = watch::channel(false);
        let completed = Arc::new(AtomicBool::new(false));
        let worker_completed = completed.clone();
        let (decision, decisions) = oneshot::channel();
        self.as_mut().rust_mut().active = None;
        self.as_mut().rust_mut().generation = self.rust().generation.wrapping_add(1);
        self.as_mut().rust_mut().pending = Some(PendingOperation {
            cancel,
            decision: Some(decision),
            completed,
        });
        self.as_mut().set_busy(true);
        self.as_mut().set_awaiting(false);
        self.as_mut().clear_pairing_display();
        self.as_mut().show(
            "connecting",
            "Abriendo identidad protegida por el llavero del sistema…",
        );

        let job = PairingJob {
            reconnect,
            directory,
            expected,
            network,
            guard,
            owner,
            cancelled,
            generation: self.rust().generation,
            thread: self.qt_thread(),
        };
        std::thread::spawn(move || {
            let _completed = WorkerCompletion(worker_completed);
            job.run(decisions);
        });
    }

    /// A cancelled worker that has not finished yet still owns the profile.
    fn worker_draining(&self) -> bool {
        self.rust()
            .draining_worker
            .as_ref()
            .is_some_and(|done| !done.load(Ordering::Acquire))
    }

    /// The profile directory and a guard for its stored network policy, or
    /// `None` after telling the user why no connection was started.
    fn network_policy_guard(mut self: Pin<&mut Self>) -> Option<(PathBuf, NetworkPolicyGuard)> {
        let Ok(directory) = peers_directory() else {
            self.show(
                "network_policy_blocked",
                "No se pudo localizar el archivo de política de red. No se inició una conexión.",
            );
            return None;
        };
        let Ok(stored) = load_network_privacy_settings(&directory) else {
            self.show(
                "network_policy_blocked",
                "No se pudo leer la política de privacidad de red. No se inició una conexión.",
            );
            return None;
        };
        let settings = NetworkPrivacySettings {
            policy: stored.policy,
            lan_bind_ip: stored.lan_bind_ip,
        };
        if save_network_privacy_settings(&directory, settings).is_err() {
            self.as_mut().show(
                "network_policy_blocked",
                "No se pudo guardar la política de red. No se inició una conexión.",
            );
            return None;
        }
        Some((directory, NetworkPolicyGuard::new(stored.policy)))
    }

    fn require_resume_owner(mut self: Pin<&mut Self>) -> Option<ResumeProcessOwner> {
        self.as_mut().start_resume_owner();
        let owner = self.rust().resume_owner.clone();
        if owner.is_none() {
            self.show(
                "profile_error",
                "No se pudo iniciar el listener de recuperación local.",
            );
        }
        owner
    }

    /// The address chosen in the UI for LAN, if any.
    fn chosen_lan_address(&self) -> Option<IpAddr> {
        usize::try_from(self.rust().network_index)
            .ok()
            .and_then(|index| self.rust().network_ips.get(index))
            .copied()
    }

    /// The routes the invitation offers, from the enabled carriers. `None`
    /// after telling the user what is missing.
    fn select_network(mut self: Pin<&mut Self>) -> Option<PeerNetwork> {
        // Without an explicit choice (e.g. after a reset), use the address the
        // system would pick for LAN, the same rule the recovery listener uses.
        let lan_bind_ip = self
            .chosen_lan_address()
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
        let internet = *self.internet_enabled();
        tracing::info!(
            lan = lan_bind_ip.is_some(),
            internet,
            "Pairing carrier selected"
        );
        if internet {
            let relay = self.rust().relay_origin.to_string();
            let network = match lan_bind_ip {
                // One invitation with both routes: the phone tries LAN, then the relay.
                Some(bind_ip) => PeerNetwork::auto(bind_ip, relay.trim()).ok(),
                None => PeerNetwork::internet(relay.trim()).ok(),
            };
            if network.is_none() {
                self.show(
                    "network_error",
                    "No se pudo preparar el servidor de conexión por Internet.",
                );
            }
            return network;
        }
        if *self.lan_enabled() {
            let Some(bind_ip) = self.chosen_lan_address() else {
                self.as_mut().set_status(QString::from(
                    "Selecciona la dirección de Desktop en tu red local.",
                ));
                return None;
            };
            return Some(PeerNetwork::Lan { bind_ip });
        }
        Some(PeerNetwork::EmulatorLoopback)
    }

    pub(super) fn show(mut self: Pin<&mut Self>, code: &str, text: &str) {
        self.as_mut().set_status_code(QString::from(code));
        self.as_mut().set_status(QString::from(text));
    }

    fn clear_pairing_display(mut self: Pin<&mut Self>) {
        self.as_mut().set_bootstrap(QString::default());
        self.as_mut().set_sas(QString::default());
        self.as_mut().set_sas_emojis(QString::from("[]"));
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

/// Checks, off the UI thread, that `expected` is still trusted and the LAN
/// listener is up, then lifts this Desktop's own block on it.
fn allow_reconnect(
    owner: &ResumeProcessOwner,
    expected: PublicIdentity,
) -> Result<(), kryptopass_peers::PeerError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| kryptopass_peers::PeerError::Link)?;
    let ready = runtime.block_on(owner.refresh_trust())?;
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
}
