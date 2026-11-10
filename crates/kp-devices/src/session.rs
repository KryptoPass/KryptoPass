//! The task that owns one authenticated session: hello, the message loop and its close.

use super::*;

pub(crate) struct SyncRuntime {
    pub(crate) worker: Option<SyncWorker>,
    /// Notified by [`SessionHandle::request_sync_round`].
    pub(crate) round_requests: Arc<tokio::sync::Notify>,
    pub(crate) progress: watch::Sender<Option<SyncProgress>>,
    pub(crate) synthetic: SyntheticRuntime,
    /// The peer advertised `VaultKeys` support, so it may send that message.
    pub(crate) accepts_vault_keys: bool,
    /// Approval requests to this peer, when it advertised the extension.
    pub(crate) approvals: approvals::ApprovalRuntime,
}

pub(crate) struct SyntheticRuntime {
    pub(crate) actor: SyntheticTransferSender,
    pub(crate) receiver: SyntheticTransferReceiver,
    pub(crate) commands: mpsc::Receiver<SyntheticCommand>,
    pub(crate) cancel: watch::Receiver<Option<[u8; 16]>>,
    pub(crate) progress: watch::Sender<SyntheticTransferProgress>,
}

impl SyntheticRuntime {
    /// Combined secret-free state of both directions.
    fn snapshot(&self) -> SyntheticTransferProgress {
        let mut progress = self.actor.progress();
        progress.inbound = self.receiver.progress();
        progress
    }

    fn publish(&self) {
        let current = self.snapshot();
        self.progress.send_if_modified(|value| {
            if *value == current {
                false
            } else {
                *value = current;
                true
            }
        });
    }

    fn sender_active(&self) -> bool {
        matches!(
            self.actor.progress().phase,
            SyntheticTransferPhase::WaitingAcceptance
                | SyntheticTransferPhase::Sending
                | SyntheticTransferPhase::WaitingReceipt
        )
    }
}

impl Drop for SyntheticRuntime {
    fn drop(&mut self) {
        self.actor.disconnected();
        self.receiver.disconnected();
        self.progress.send_replace(self.snapshot());
    }
}

#[derive(Debug)]
pub(crate) enum SessionExit {
    Disconnected(SessionCloseReason, bool),
    Failed(SessionFailure),
}

pub(crate) async fn run_connected_session<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    directory: &DeviceDirectory,
    directory_changes: &mut watch::Receiver<Arc<DeviceDirectorySnapshot>>,
    commands: &mut mpsc::Receiver<SessionCommand>,
    expected_peer: PublicIdentity,
    generation: u64,
    sync_runtime: SyncRuntime,
) -> SessionExit {
    let SyncRuntime {
        worker: mut sync_worker,
        round_requests,
        progress: sync_progress,
        mut synthetic,
        accepts_vault_keys,
        mut approvals,
    } = sync_runtime;
    let mut next_observation = Instant::now();
    let mut next_local_nonce = 1_u64;
    let mut last_remote_nonce = 0_u64;
    let mut outstanding: Option<(u64, Instant)> = None;
    let mut next_ping_at = Instant::now() + SESSION_PING_INTERVAL;
    let mut rate_limiter = ControlRateLimiter::default();
    let mut data_rate_limiter = DataRateLimiter::default();
    let mut sync_rate_limiter = DataRateLimiter::default();
    // Retain at most one decoded frame while a bounded storage queue is busy.
    // Keep draining worker output and observing cancellation instead of treating
    // an ordinary scheduler/SQLite delay as a malformed peer message.
    let mut pending_sync_input: Option<(SyncWorkerInput, Instant)> = None;
    // A Sync failure suspends Sync for the rest of this session instead of
    // closing it; only revoked trust still ends the authenticated connection.
    let mut sync_suspended = false;
    let suspend_sync = |worker: &mut Option<SyncWorker>,
                        pending: &mut Option<(SyncWorkerInput, Instant)>,
                        suspended: &mut bool,
                        reason: &'static str| {
        if !*suspended {
            tracing::warn!(
                reason,
                "Sync suspended for this session; the connection stays up"
            );
        }
        *worker = None;
        *pending = None;
        *suspended = true;
        sync_progress.send_modify(|progress| {
            if let Some(progress) = progress {
                progress.state = kryptopass_sync::SyncProgressState::Failed;
            }
        });
    };

    loop {
        if !directory.session_is_current(&expected_peer, generation) {
            return SessionExit::Disconnected(SessionCloseReason::TrustRevoked, true);
        }
        match commands.try_recv() {
            Ok(SessionCommand::Disconnect(reason)) => {
                return SessionExit::Disconnected(reason, true);
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                return SessionExit::Disconnected(SessionCloseReason::AppClosing, true);
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
        }
        // Give already queued human cancellation priority over the next key chunk.
        // The separate disconnect queue remains observable even under provisioning.
        for _ in 0..2 {
            let Ok(command) = synthetic.commands.try_recv() else {
                break;
            };
            if let Err(failure) = process_synthetic_command(session, &mut synthetic, command).await
            {
                return SessionExit::Failed(failure);
            }
        }
        let requested_cancel = *synthetic.cancel.borrow_and_update();
        if let Some(id) = requested_cancel {
            if synthetic.actor.progress().transfer_id == Some(id)
                && matches!(
                    synthetic.actor.progress().phase,
                    SyntheticTransferPhase::WaitingAcceptance
                        | SyntheticTransferPhase::Sending
                        | SyntheticTransferPhase::WaitingReceipt
                )
            {
                if let Ok(abort) = synthetic.actor.cancel(id, Instant::now()) {
                    if let Err(failure) = send_synthetic_outbound_bounded(session, abort).await {
                        return SessionExit::Failed(failure);
                    }
                }
            }
        }
        match synthetic.actor.next_outbound(Instant::now()) {
            Ok(Some(outbound)) => {
                if let Err(error) = send_synthetic_outbound_bounded(session, outbound).await {
                    return SessionExit::Failed(error);
                }
            }
            Ok(None) => {}
            Err(_) => return SessionExit::Failed(SessionFailure::Protocol),
        }
        if let Some(abort) = synthetic.receiver.expire(Instant::now()) {
            if let Err(error) = send_synthetic_outbound_bounded(session, abort).await {
                return SessionExit::Failed(error);
            }
        }
        synthetic.publish();
        if let Some((input, deadline)) = pending_sync_input.take() {
            if let Some(worker) = sync_worker.as_ref() {
                match worker.input.try_send(input) {
                    Ok(()) => {}
                    Err(StdTrySendError::Full(input)) if Instant::now() < deadline => {
                        pending_sync_input = Some((input, deadline));
                    }
                    Err(_) => suspend_sync(
                        &mut sync_worker,
                        &mut pending_sync_input,
                        &mut sync_suspended,
                        "worker input queue full past its deadline or closed",
                    ),
                }
            } else if !sync_suspended {
                tracing::warn!("Sync input arrived without a sync worker");
                return SessionExit::Failed(SessionFailure::Sync);
            }
        }
        let ping_deadline = outstanding
            .map(|(_, deadline)| deadline)
            .unwrap_or(next_ping_at);
        let approval_deadline = approvals.next_deadline();
        let selected = tokio::select! {
            () = sleep_until(next_observation) => SelectedSessionEvent::ObservePath,
            () = round_requests.notified() => SelectedSessionEvent::SyncRoundRequested,
            command = commands.recv() => SelectedSessionEvent::Command(command),
            command = synthetic.commands.recv() => SelectedSessionEvent::SyntheticCommand(command),
            command = approvals::receive_command(&mut approvals.commands), if approvals.commands.is_some() => {
                SelectedSessionEvent::Approval(command)
            },
            () = sleep_until(approval_deadline.unwrap_or_else(Instant::now)), if approval_deadline.is_some() => {
                SelectedSessionEvent::ApprovalExpiry
            },
            _ = synthetic.cancel.changed() => SelectedSessionEvent::SyntheticTick,
            () = tokio::time::sleep(Duration::from_millis(if synthetic.actor.progress().phase == SyntheticTransferPhase::Sending { 1 } else { 100 })),
                if synthetic.sender_active() || synthetic.receiver.is_active() => SelectedSessionEvent::SyntheticTick,
            changed = directory_changes.changed() => SelectedSessionEvent::DirectoryChanged(changed),
            message = session.receive_message(), if pending_sync_input.is_none() => SelectedSessionEvent::Message(message),
            output = receive_sync_worker_output(&mut sync_worker), if sync_worker.is_some() => {
                SelectedSessionEvent::SyncOutput(output)
            },
            () = sleep_until(next_ping_at), if outstanding.is_none() => SelectedSessionEvent::PingDue,
            () = sleep_until(ping_deadline), if outstanding.is_some() => SelectedSessionEvent::PingTimeout,
            () = tokio::time::sleep(Duration::from_millis(1)), if pending_sync_input.is_some() => SelectedSessionEvent::RetrySyncInput,
        };

        if !directory.session_is_current(&expected_peer, generation) {
            return SessionExit::Disconnected(SessionCloseReason::TrustRevoked, true);
        }
        match selected {
            SelectedSessionEvent::RetrySyncInput | SelectedSessionEvent::SyntheticTick => {}
            SelectedSessionEvent::ApprovalExpiry => approvals.expire(),
            SelectedSessionEvent::Approval(None) => approvals.commands = None,
            SelectedSessionEvent::Approval(Some(command)) => {
                let sent = match approvals.command(command) {
                    Some((message_type, approvals::ApprovalMessage::Request(request))) => {
                        send_control_bounded(session, message_type, &request).await
                    }
                    Some((message_type, approvals::ApprovalMessage::Cancel(cancel))) => {
                        send_control_bounded(session, message_type, &cancel).await
                    }
                    None => Ok(()),
                };
                if let Err(failure) = sent {
                    return SessionExit::Failed(failure);
                }
            }
            SelectedSessionEvent::SyntheticCommand(Some(command)) => {
                if let Err(failure) =
                    process_synthetic_command(session, &mut synthetic, command).await
                {
                    return SessionExit::Failed(failure);
                }
            }
            SelectedSessionEvent::SyntheticCommand(None) => {
                return SessionExit::Disconnected(SessionCloseReason::AppClosing, true);
            }
            SelectedSessionEvent::ObservePath => {
                let info =
                    tokio::time::timeout(Duration::from_millis(500), session.connection_info())
                        .await
                        .unwrap_or_default();
                let _ = directory.update_connection_info_for_generation(
                    &expected_peer,
                    generation,
                    info,
                );
                next_observation = Instant::now() + Duration::from_secs(2);
            }
            SelectedSessionEvent::SyncRoundRequested => match sync_worker.as_ref() {
                Some(worker) => {
                    if worker
                        .input
                        .try_send(SyncWorkerInput::RequestRound)
                        .is_err()
                    {
                        tracing::warn!(
                            "Sync: round request dropped, the worker queue is full or closed"
                        );
                    }
                }
                None => {
                    tracing::info!("Sync: round requested but this session has no Sync running")
                }
            },
            SelectedSessionEvent::Command(Some(SessionCommand::Disconnect(reason))) => {
                return SessionExit::Disconnected(reason, true);
            }
            SelectedSessionEvent::Command(None) => {
                return SessionExit::Disconnected(SessionCloseReason::AppClosing, true);
            }
            SelectedSessionEvent::DirectoryChanged(Ok(())) => {
                if let Some(reason) =
                    directory.session_termination_reason(&expected_peer, generation)
                {
                    return SessionExit::Disconnected(reason, true);
                }
            }
            SelectedSessionEvent::DirectoryChanged(Err(_)) => {
                return SessionExit::Failed(SessionFailure::Protocol);
            }
            SelectedSessionEvent::PingDue => {
                let nonce = next_local_nonce;
                let Some(next_nonce) = nonce.checked_add(1) else {
                    return SessionExit::Failed(SessionFailure::NonceSequence);
                };
                if let Err(failure) = send_control_bounded(
                    session,
                    proto::MessageType::SessionPing,
                    &proto::SessionPing { nonce },
                )
                .await
                {
                    return SessionExit::Failed(failure);
                }
                next_local_nonce = next_nonce;
                outstanding = Some((nonce, Instant::now() + SESSION_PING_TIMEOUT));
            }
            SelectedSessionEvent::PingTimeout => {
                return SessionExit::Failed(SessionFailure::PingTimeout);
            }
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::Outbound(outbound))) => {
                let kind = match &outbound {
                    SyncOutbound::Hello(_) => Some("hello"),
                    SyncOutbound::Manifest(_) => Some("manifest"),
                    SyncOutbound::Request(_) => Some("request"),
                    SyncOutbound::Chunk(message)
                        if message.chunk_index == 0
                            || message.chunk_index + 1 == message.chunk_count =>
                    {
                        Some("chunk")
                    }
                    SyncOutbound::Chunk(_) => None,
                    SyncOutbound::Commit(_) => Some("commit"),
                    SyncOutbound::Ack(_) => Some("ack"),
                };
                if let Err(failure) = send_sync_outbound_bounded(session, outbound).await {
                    return SessionExit::Failed(failure);
                }
                if cfg!(debug_assertions) {
                    if let Some(kind) = kind {
                        tracing::debug!(message_type = kind, "Sync tx_sent");
                    }
                }
            }
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::Progress(progress))) => {
                sync_progress.send_replace(Some(progress));
            }
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::Failed(error))) => {
                tracing::warn!(error = %error, "Sync worker failed");
                if matches!(error, SyncSessionError::TrustRevoked) {
                    return SessionExit::Disconnected(SessionCloseReason::TrustRevoked, true);
                }
                suspend_sync(
                    &mut sync_worker,
                    &mut pending_sync_input,
                    &mut sync_suspended,
                    "worker failed",
                );
            }
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::Complete)) => {
                sync_worker = None;
            }
            SelectedSessionEvent::SyncOutput(Some(SyncWorkerOutput::RoundComplete)) => {}
            SelectedSessionEvent::SyncOutput(None) => {
                suspend_sync(
                    &mut sync_worker,
                    &mut pending_sync_input,
                    &mut sync_suspended,
                    "worker stopped without a result",
                );
            }
            SelectedSessionEvent::Message(Err(error)) => {
                return SessionExit::Failed(classify_pairing_error(error));
            }
            SelectedSessionEvent::Message(Ok(message)) => {
                match message.channel {
                    Channel::Control
                        if !(if is_sync_message(message.message_type) {
                            sync_rate_limiter
                                .accept_limit(Instant::now(), MAX_SYNC_CONTROL_MESSAGES_PER_SECOND)
                        } else {
                            rate_limiter.accept(Instant::now())
                        }) =>
                    {
                        return SessionExit::Failed(SessionFailure::ControlRate);
                    }
                    Channel::Data if !data_rate_limiter.accept(Instant::now()) => {
                        return SessionExit::Failed(SessionFailure::DataRate);
                    }
                    Channel::Control | Channel::Data => {}
                }
                match (message.message_type, message.payload) {
                    (proto::MessageType::SessionPing, MessagePayload::SessionPing(ping)) => {
                        let Some(expected_nonce) = last_remote_nonce.checked_add(1) else {
                            return SessionExit::Failed(SessionFailure::NonceSequence);
                        };
                        if ping.nonce != expected_nonce {
                            return SessionExit::Failed(SessionFailure::NonceSequence);
                        }
                        if let Err(failure) = send_control_bounded(
                            session,
                            proto::MessageType::SessionPong,
                            &proto::SessionPong { nonce: ping.nonce },
                        )
                        .await
                        {
                            return SessionExit::Failed(failure);
                        }
                        last_remote_nonce = ping.nonce;
                    }
                    (proto::MessageType::SessionPong, MessagePayload::SessionPong(pong)) => {
                        match outstanding {
                            Some((_, deadline)) if Instant::now() >= deadline => {
                                return SessionExit::Failed(SessionFailure::PingTimeout);
                            }
                            Some((nonce, _)) if nonce == pong.nonce => {}
                            _ => return SessionExit::Failed(SessionFailure::NonceSequence),
                        }
                        outstanding = None;
                        next_ping_at = Instant::now() + SESSION_PING_INTERVAL;
                    }
                    (proto::MessageType::SessionClose, MessagePayload::SessionClose(close)) => {
                        let Some(reason) = SessionCloseReason::from_wire(close.reason) else {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        };
                        return SessionExit::Disconnected(reason, false);
                    }
                    (
                        proto::MessageType::ApprovalResponse,
                        MessagePayload::ApprovalResponse(response),
                    ) => {
                        if !approvals.enabled() {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        }
                        approvals.response(response);
                    }
                    (
                        proto::MessageType::ApprovalCancel,
                        MessagePayload::ApprovalCancel(cancel),
                    ) => {
                        if !approvals.enabled() {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        }
                        approvals.remote_cancel(&cancel);
                    }
                    (proto::MessageType::ApprovalRequest, _) => {
                        // Desktop asks; it never answers a phone's request.
                        return SessionExit::Failed(SessionFailure::Protocol);
                    }
                    (proto::MessageType::VaultKeys, MessagePayload::VaultKeys(keys)) => {
                        // Only a peer that advertised the capability may send it.
                        if !accepts_vault_keys {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        }
                        let count = keys.openpgp_fingerprints.len();
                        if directory
                            .set_peer_vault_keys(
                                &expected_peer,
                                generation,
                                keys.openpgp_fingerprints,
                            )
                            .is_err()
                        {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        }
                        tracing::info!(keys = count, "Peer declared its vault keys");
                    }
                    (
                        proto::MessageType::SyntheticKeyOffer
                        | proto::MessageType::SyntheticKeyChunk
                        | proto::MessageType::SyntheticKeyComplete,
                        payload,
                    ) => {
                        // Offer, Chunk and Complete always target the local receiver.
                        // Failures are transfer errors answered with an abort; the
                        // authenticated session and Sync stay up.
                        let now = Instant::now();
                        let outbound = match &payload {
                            MessagePayload::SyntheticKeyOffer(offer) => {
                                let busy = synthetic.sender_active();
                                match synthetic.receiver.offer(offer, busy, now) {
                                    Ok(()) => None,
                                    Err(refusal) => {
                                        tracing::warn!(
                                            refusal = ?refusal,
                                            "Inbound key transfer: answering offer with abort code 4"
                                        );
                                        Some(SyntheticTransferOutbound::Abort(
                                            proto::SyntheticKeyAbort {
                                                transfer_id: offer.transfer_id.clone(),
                                                code: 4,
                                            },
                                        ))
                                    }
                                }
                            }
                            MessagePayload::SyntheticKeyChunk(chunk) => {
                                synthetic.receiver.chunk(chunk, now)
                            }
                            MessagePayload::SyntheticKeyComplete(complete) => {
                                synthetic.receiver.complete(complete, now)
                            }
                            _ => None,
                        };
                        if let MessagePayload::SyntheticKeyChunk(mut chunk) = payload {
                            chunk.data.zeroize();
                        }
                        if let Some(outbound) = outbound {
                            if let Err(failure) =
                                send_synthetic_outbound_bounded(session, outbound).await
                            {
                                return SessionExit::Failed(failure);
                            }
                        }
                        synthetic.publish();
                    }
                    (
                        proto::MessageType::SyntheticKeyAbort,
                        MessagePayload::SyntheticKeyAbort(abort),
                    ) if synthetic.receiver.knows(&abort.transfer_id) => {
                        synthetic.receiver.remote_abort(&abort);
                        synthetic.publish();
                    }
                    (
                        proto::MessageType::SyntheticKeyDecision
                        | proto::MessageType::SyntheticKeyReceipt
                        | proto::MessageType::SyntheticKeyAbort,
                        payload,
                    ) => {
                        let progress = synthetic.actor.progress();
                        let supplied_id = match &payload {
                            MessagePayload::SyntheticKeyDecision(message) => {
                                message.transfer_id.as_slice()
                            }
                            MessagePayload::SyntheticKeyReceipt(message) => {
                                message.transfer_id.as_slice()
                            }
                            MessagePayload::SyntheticKeyAbort(message) => {
                                message.transfer_id.as_slice()
                            }
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
                            continue;
                        }
                        // A frame for no active outbound operation, or for another id,
                        // is a transfer error; it never tears down the session.
                        if !synthetic.sender_active()
                            || progress
                                .transfer_id
                                .as_ref()
                                .is_none_or(|id| supplied_id != id)
                        {
                            tracing::warn!(
                                message_type = message.message_type.as_str_name(),
                                phase = ?progress.phase,
                                "Key transfer: frame for an unknown outbound operation ignored"
                            );
                            continue;
                        }
                        if let Err(error) = synthetic.actor.receive(payload, Instant::now()) {
                            tracing::warn!(
                                error = ?error,
                                phase = ?synthetic.actor.progress().phase,
                                "Key transfer: outbound operation failed on a peer frame"
                            );
                        }
                        synthetic.publish();
                    }
                    (message_type, payload)
                        if is_sync_message(message_type) && message.channel == Channel::Control =>
                    {
                        // The peer may still send its pending round after a local failure.
                        if sync_suspended {
                            continue;
                        }
                        let Some(worker) = sync_worker.as_ref() else {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        };
                        match worker
                            .input
                            .try_send(SyncWorkerInput::Message(message_type, payload))
                        {
                            Ok(()) => {}
                            Err(StdTrySendError::Full(input)) => {
                                pending_sync_input =
                                    Some((input, Instant::now() + Duration::from_secs(2)));
                            }
                            Err(StdTrySendError::Disconnected(_)) => suspend_sync(
                                &mut sync_worker,
                                &mut pending_sync_input,
                                &mut sync_suspended,
                                "worker closed",
                            ),
                        }
                    }
                    (proto::MessageType::SyncChunk, payload)
                        if message.channel == Channel::Data =>
                    {
                        if sync_suspended {
                            continue;
                        }
                        let Some(worker) = sync_worker.as_ref() else {
                            return SessionExit::Failed(SessionFailure::Protocol);
                        };
                        match worker.input.try_send(SyncWorkerInput::Message(
                            proto::MessageType::SyncChunk,
                            payload,
                        )) {
                            Ok(()) => {}
                            Err(StdTrySendError::Full(input)) => {
                                pending_sync_input =
                                    Some((input, Instant::now() + Duration::from_secs(2)));
                            }
                            Err(StdTrySendError::Disconnected(_)) => suspend_sync(
                                &mut sync_worker,
                                &mut pending_sync_input,
                                &mut sync_suspended,
                                "worker closed",
                            ),
                        }
                    }
                    _ => return SessionExit::Failed(SessionFailure::Protocol),
                }
            }
        }
    }
}

enum SelectedSessionEvent {
    Approval(Option<approvals::ApprovalCommand>),
    ApprovalExpiry,
    SyncRoundRequested,
    SyntheticCommand(Option<SyntheticCommand>),
    SyntheticTick,
    RetrySyncInput,
    ObservePath,
    Command(Option<SessionCommand>),
    DirectoryChanged(Result<(), watch::error::RecvError>),
    Message(Result<kryptopass_protocol::DecodedMessage, PairingError>),
    SyncOutput(Option<SyncWorkerOutput>),
    PingDue,
    PingTimeout,
}

async fn receive_sync_worker_output(worker: &mut Option<SyncWorker>) -> Option<SyncWorkerOutput> {
    match worker {
        Some(worker) => worker.outputs.recv().await,
        None => std::future::pending().await,
    }
}

fn is_sync_message(message_type: proto::MessageType) -> bool {
    matches!(
        message_type,
        proto::MessageType::SyncHello
            | proto::MessageType::SyncManifest
            | proto::MessageType::SyncRequest
            | proto::MessageType::SyncChunk
            | proto::MessageType::SyncCommit
            | proto::MessageType::SyncAck
    )
}

async fn process_synthetic_command<T: PeerTransport>(
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
