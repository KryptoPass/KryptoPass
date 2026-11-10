//! Key transfer over a connected session, in both directions. A transfer
//! error is answered with an abort and never tears down the session or Sync.

use super::*;

/// Applies a local command: start sending, or decide and finish a receive.
pub(super) async fn process_command<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    runtime: &mut SyntheticRuntime,
    command: SyntheticCommand,
) -> Result<(), SessionFailure> {
    let now = Instant::now();
    let outbound = match command {
        SyntheticCommand::Start(_) if runtime.receiver.is_active() => {
            tracing::warn!("Key transfer: start refused, an inbound operation is active");
            None
        }
        SyntheticCommand::Start(prepared) => runtime.actor.start(*prepared, now).ok(),
        SyntheticCommand::InboundDecision(transfer_id, accepted) => {
            runtime.receiver.decide(transfer_id, accepted, now)
        }
        SyntheticCommand::InboundFinished(transfer_id, challenge_sha256) => runtime
            .receiver
            .finish_import(transfer_id, challenge_sha256, now),
    };
    if let Some(outbound) = outbound {
        send_synthetic_outbound_bounded(session, outbound).await?;
    }
    runtime.progress.send_replace(runtime.snapshot());
    Ok(())
}

impl<T: PeerTransport> Connected<'_, T> {
    /// Before each wait: queued local commands, a requested cancellation, the
    /// next outbound frame and the receiver's expiry.
    pub(super) async fn drive_key_transfer(&mut self) -> Step {
        // Give already queued human cancellation priority over the next key chunk.
        // The separate disconnect queue remains observable even under provisioning.
        for _ in 0..2 {
            let Ok(command) = self.synthetic.commands.try_recv() else {
                break;
            };
            process_command(&mut *self.session, &mut self.synthetic, command)
                .await
                .map_err(failed)?;
        }
        let requested_cancel = *self.synthetic.cancel.borrow_and_update();
        if let Some(id) = requested_cancel {
            if self.synthetic.actor.progress().transfer_id == Some(id)
                && self.synthetic.sender_active()
            {
                if let Ok(abort) = self.synthetic.actor.cancel(id, Instant::now()) {
                    send_synthetic_outbound_bounded(&mut *self.session, abort)
                        .await
                        .map_err(failed)?;
                }
            }
        }
        match self.synthetic.actor.next_outbound(Instant::now()) {
            Ok(Some(outbound)) => {
                send_synthetic_outbound_bounded(&mut *self.session, outbound)
                    .await
                    .map_err(failed)?;
            }
            Ok(None) => {}
            Err(_) => return Err(failed(SessionFailure::Protocol)),
        }
        if let Some(abort) = self.synthetic.receiver.expire(Instant::now()) {
            send_synthetic_outbound_bounded(&mut *self.session, abort)
                .await
                .map_err(failed)?;
        }
        self.synthetic.publish();
        Ok(())
    }

    /// Offer, Chunk and Complete always target the local receiver. Failures
    /// are transfer errors answered with an abort; the authenticated session
    /// and Sync stay up.
    pub(super) async fn on_inbound_key_transfer(&mut self, payload: MessagePayload) -> Step {
        let now = Instant::now();
        let outbound = match &payload {
            MessagePayload::SyntheticKeyOffer(offer) => {
                let busy = self.synthetic.sender_active();
                match self.synthetic.receiver.offer(offer, busy, now) {
                    Ok(()) => None,
                    Err(refusal) => {
                        tracing::warn!(
                            refusal = ?refusal,
                            "Inbound key transfer: answering offer with abort code 4"
                        );
                        Some(SyntheticTransferOutbound::Abort(proto::SyntheticKeyAbort {
                            transfer_id: offer.transfer_id.clone(),
                            code: 4,
                        }))
                    }
                }
            }
            MessagePayload::SyntheticKeyChunk(chunk) => self.synthetic.receiver.chunk(chunk, now),
            MessagePayload::SyntheticKeyComplete(complete) => {
                self.synthetic.receiver.complete(complete, now)
            }
            _ => None,
        };
        if let MessagePayload::SyntheticKeyChunk(mut chunk) = payload {
            chunk.data.zeroize();
        }
        if let Some(outbound) = outbound {
            send_synthetic_outbound_bounded(&mut *self.session, outbound)
                .await
                .map_err(failed)?;
        }
        self.synthetic.publish();
        Ok(())
    }

    /// The peer's Decision, Receipt or Abort for the transfer this side sends.
    pub(super) fn on_outbound_key_transfer_reply(
        &mut self,
        message_type: proto::MessageType,
        payload: MessagePayload,
    ) {
        let progress = self.synthetic.actor.progress();
        let supplied_id = match &payload {
            MessagePayload::SyntheticKeyDecision(message) => message.transfer_id.as_slice(),
            MessagePayload::SyntheticKeyReceipt(message) => message.transfer_id.as_slice(),
            MessagePayload::SyntheticKeyAbort(message) => message.transfer_id.as_slice(),
            _ => &[],
        };
        // Crossing cancellations and already-in-flight decisions for the
        // consumed operation never reopen it or terminate unrelated Sync.
        if matches!(
            progress.phase,
            SyntheticTransferPhase::Complete
                | SyntheticTransferPhase::Rejected
                | SyntheticTransferPhase::Cancelled
                | SyntheticTransferPhase::Expired
                | SyntheticTransferPhase::Failed
                | SyntheticTransferPhase::Uncertain
        ) && progress
            .transfer_id
            .as_ref()
            .is_some_and(|id| supplied_id == id)
        {
            return;
        }
        // A frame for no active outbound operation, or for another id,
        // is a transfer error; it never tears down the session.
        if !self.synthetic.sender_active()
            || progress
                .transfer_id
                .as_ref()
                .is_none_or(|id| supplied_id != id)
        {
            tracing::warn!(
                message_type = message_type.as_str_name(),
                phase = ?progress.phase,
                "Key transfer: frame for an unknown outbound operation ignored"
            );
            return;
        }
        if let Err(error) = self.synthetic.actor.receive(payload, Instant::now()) {
            tracing::warn!(
                error = ?error,
                phase = ?self.synthetic.actor.progress().phase,
                "Key transfer: outbound operation failed on a peer frame"
            );
        }
        self.synthetic.publish();
    }
}
