//! Sending an OpenPGP identity to a paired device.

use super::*;

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

pub(super) fn synthetic_transfer_phase_code(
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

impl qobject::PeersController {
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

    /// Local confirmation callback for the exact OpenPGP identity chosen for the phone.
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
        if !cfg!(debug_assertions) && matches!(purpose, KeyTransferPurpose::SyntheticFixture) {
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
        let (valid_approval, minimum_version) = match purpose {
            KeyTransferPurpose::SyntheticFixture => (
                !approved_fingerprint.is_empty()
                    && approved_fingerprint
                        == self.rust().synthetic_transfer_fingerprint.to_string(),
                1,
            ),
            KeyTransferPurpose::VaultIdentity => (
                kryptopass_core::KeyFingerprint::parse(&approved_fingerprint)
                    .is_ok_and(|fingerprint| fingerprint.as_str().len() == 40),
                2,
            ),
        };
        if !valid_approval {
            self.as_mut().set_synthetic_transfer_error(QString::from(
                "La clave elegida no es válida. Vuelve a abrir la selección y revisa su huella.",
            ));
            return;
        }
        if matches!(purpose, KeyTransferPurpose::VaultIdentity) {
            self.as_mut().rust_mut().vault_key_send = Some((peer, approved_fingerprint.clone()));
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
                        crate::ui_adapters::synthetic_key_transfer::prepare(&approved_fingerprint)?
                    }
                    KeyTransferPurpose::VaultIdentity => {
                        crate::ui_adapters::synthetic_key_transfer::prepare_selected_identity(
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

    /// Cancels preparation or the exact active transfer when the view closes,
    /// navigation changes, or the user disconnects the session.
    pub fn cancel_synthetic_key_transfer(mut self: Pin<&mut Self>) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
    }

    #[track_caller]
    pub(super) fn cancel_synthetic_key_transfer_operation(mut self: Pin<&mut Self>) {
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
}
