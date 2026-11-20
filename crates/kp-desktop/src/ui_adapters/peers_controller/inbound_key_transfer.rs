//! Receiving an OpenPGP identity from a paired device, reviewed before import.

use super::*;

/// Secret-free UI state of the phone → Desktop vault-identity transfer.
pub(super) struct InboundTransferUi {
    peer: DeviceId,
    generation: u64,
    transfer_id: [u8; 16],
    fingerprint: [u8; 20],
    device_name: String,
    assessment: Option<crate::ui_adapters::synthetic_key_transfer::InboundAssessment>,
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

impl qobject::PeersController {
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

    pub(super) fn track_inbound_key_transfer(
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
                    .unwrap_or_else(|| {
                        crate::i18n::trf("Device %1", &[id[..12.min(id.len())].to_owned()])
                    });
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
                    message: String::from(crate::i18n::tr_noop("Reviewing the offered key…")),
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
                        ui.message =
                            String::from(crate::i18n::tr_noop("Receiving the key from the phone…"));
                    }
                    InboundTransferPhase::Importing => {
                        if !ui.import_started {
                            ui.import_started = true;
                            ui.phase = "importing";
                            ui.message = String::from(crate::i18n::tr_noop(
                                "Key received and verified (SHA-256). Importing into GnuPG…",
                            ));
                            start_import = true;
                        }
                    }
                    InboundTransferPhase::Complete => {
                        ui.phase = "complete";
                        ui.error = false;
                        ui.message = String::from(crate::i18n::tr_noop(
                            "Key imported into GnuPG and checked with the challenge. The phone received the confirmation.",
                        ));
                    }
                    InboundTransferPhase::Rejected => {
                        ui.phase = "rejected";
                        if ui.message.is_empty() || !ui.error {
                            ui.message = String::from(crate::i18n::tr_noop(
                                "Transfer rejected. Nothing was received.",
                            ));
                        }
                    }
                    InboundTransferPhase::Cancelled => {
                        ui.phase = "cancelled";
                        ui.error = true;
                        ui.message = String::from(crate::i18n::tr_noop(
                            "The phone canceled the transfer. What was received was discarded.",
                        ));
                    }
                    InboundTransferPhase::Expired => {
                        ui.phase = "expired";
                        ui.error = true;
                        ui.message = String::from(crate::i18n::tr_noop(
                            "The transfer expired (5 minutes after the offer). What was received was discarded.",
                        ));
                    }
                    InboundTransferPhase::Failed => {
                        if previous != "failed" {
                            ui.phase = "failed";
                            // A local import failure already set a precise message.
                            if !ui.error {
                                ui.error = true;
                                ui.message = String::from(match inbound.failure {
                                    Some(InboundTransferFailure::Protocol) => crate::i18n::tr_noop(
                                        "The transfer did not respect the announced order or size. What was received was discarded.",
                                    ),
                                    Some(InboundTransferFailure::Digest) => crate::i18n::tr_noop(
                                        "The received key does not match the offer (SHA-256). It was discarded.",
                                    ),
                                    Some(InboundTransferFailure::Remote(_)) => {
                                        crate::i18n::tr_noop(
                                            "The phone interrupted the transfer. What was received was discarded.",
                                        )
                                    }
                                    Some(InboundTransferFailure::Disconnected) => {
                                        crate::i18n::tr_noop(
                                            "The connection was lost before finishing. What was received was discarded.",
                                        )
                                    }
                                    Some(InboundTransferFailure::Local) | None => {
                                        crate::i18n::tr_noop(
                                            "The key could not be imported or checked. What was received was discarded.",
                                        )
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
            let assessment =
                crate::ui_adapters::synthetic_key_transfer::assess_inbound(&fingerprint);
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
                crate::i18n::tr_noop("Accepted. Waiting for the phone's key…")
            } else {
                crate::i18n::tr_noop("You rejected the transfer. Nothing will be received.")
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
                        crate::i18n::tr_noop("Your decision could not be sent: the session changed or the offer expired."),
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
                let result = crate::ui_adapters::synthetic_key_transfer::import_inbound(
                    &material,
                    |stage| {
                        let _ = stage_thread.queue(move |mut object| {
                        object.as_mut().update_inbound(transfer_id, |ui| {
                            if ui.error || ui.phase == "complete" {
                                return;
                            }
                            ui.phase = stage;
                            ui.message = String::from(if stage == "verifying" {
                                crate::i18n::tr_noop("Key imported. Checking that it decrypts the challenge; GnuPG may ask for its passphrase…")
                            } else {
                                crate::i18n::tr_noop("Importing the key into GnuPG…")
                            });
                        });
                    });
                    },
                );
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
                                crate::i18n::tr_noop("Key imported and checked. Sending the confirmation to the phone…"),
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
                            crate::i18n::tr_noop("The session changed before importing. Nothing was imported."),
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
                let fingerprint =
                    crate::ui_adapters::synthetic_key_transfer::fingerprint_hex(&ui.fingerprint);
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
