//! The task that owns one authenticated session: hello, the message loop and its close.

use super::*;

mod key_transfer;
mod sync;

use sync::SyncLink;

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

/// What a step of the session returns: `Err` ends the session with that exit.
type Step = Result<(), SessionExit>;

fn failed(failure: SessionFailure) -> SessionExit {
    SessionExit::Failed(failure)
}

/// Keepalive in both directions: our ping awaiting its pong, and the nonce the
/// peer's next ping must carry.
struct Keepalive {
    next_local_nonce: u64,
    last_remote_nonce: u64,
    /// The ping sent and not yet answered, with its deadline.
    outstanding: Option<(u64, Instant)>,
    next_ping_at: Instant,
}

/// One connected session between events: the transport, who the peer is, and
/// the state of each thing that runs over it.
struct Connected<'a, T: PeerTransport> {
    session: &'a mut EstablishedSession<T>,
    directory: &'a DeviceDirectory,
    peer: PublicIdentity,
    generation: u64,
    keepalive: Keepalive,
    control_rate: ControlRateLimiter,
    data_rate: DataRateLimiter,
    sync_rate: DataRateLimiter,
    sync: SyncLink,
    synthetic: SyntheticRuntime,
    approvals: approvals::ApprovalRuntime,
    /// The peer advertised `VaultKeys` support, so it may send that message.
    accepts_vault_keys: bool,
    next_observation: Instant,
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
        worker,
        round_requests,
        progress,
        synthetic,
        accepts_vault_keys,
        approvals,
    } = sync_runtime;
    let mut connected = Connected {
        session,
        directory,
        peer: expected_peer,
        generation,
        keepalive: Keepalive {
            next_local_nonce: 1,
            last_remote_nonce: 0,
            outstanding: None,
            next_ping_at: Instant::now() + SESSION_PING_INTERVAL,
        },
        control_rate: ControlRateLimiter::default(),
        data_rate: DataRateLimiter::default(),
        sync_rate: DataRateLimiter::default(),
        sync: SyncLink::new(worker, progress),
        synthetic,
        approvals,
        accepts_vault_keys,
        next_observation: Instant::now(),
    };
    loop {
        let event = match connected
            .next_event(directory_changes, commands, &round_requests)
            .await
        {
            Ok(event) => event,
            Err(exit) => return exit,
        };
        if let Err(exit) = connected.handle(event).await {
            return exit;
        }
    }
}

impl<T: PeerTransport> Connected<'_, T> {
    /// Revoked trust or a newer session for the same peer ends this one.
    fn ensure_current(&self) -> Step {
        if self
            .directory
            .session_is_current(&self.peer, self.generation)
        {
            Ok(())
        } else {
            Err(SessionExit::Disconnected(
                SessionCloseReason::TrustRevoked,
                true,
            ))
        }
    }

    /// Does the work that must not wait behind the next event, then waits for
    /// one. Trust is checked before and after the wait.
    async fn next_event(
        &mut self,
        directory_changes: &mut watch::Receiver<Arc<DeviceDirectorySnapshot>>,
        commands: &mut mpsc::Receiver<SessionCommand>,
        round_requests: &tokio::sync::Notify,
    ) -> Result<SelectedSessionEvent, SessionExit> {
        self.ensure_current()?;
        match commands.try_recv() {
            Ok(SessionCommand::Disconnect(reason)) => {
                return Err(SessionExit::Disconnected(reason, true));
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                return Err(SessionExit::Disconnected(
                    SessionCloseReason::AppClosing,
                    true,
                ));
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
        }
        self.drive_key_transfer().await?;
        self.sync.retry_pending()?;

        let ping_deadline = self
            .keepalive
            .outstanding
            .map(|(_, deadline)| deadline)
            .unwrap_or(self.keepalive.next_ping_at);
        let approval_deadline = self.approvals.next_deadline();
        let selected = tokio::select! {
            () = sleep_until(self.next_observation) => SelectedSessionEvent::ObservePath,
            () = round_requests.notified() => SelectedSessionEvent::SyncRoundRequested,
            command = commands.recv() => SelectedSessionEvent::Command(command),
            command = self.synthetic.commands.recv() => SelectedSessionEvent::SyntheticCommand(command),
            command = approvals::receive_command(&mut self.approvals.commands), if self.approvals.commands.is_some() => {
                SelectedSessionEvent::Approval(command)
            },
            () = sleep_until(approval_deadline.unwrap_or_else(Instant::now)), if approval_deadline.is_some() => {
                SelectedSessionEvent::ApprovalExpiry
            },
            _ = self.synthetic.cancel.changed() => SelectedSessionEvent::SyntheticTick,
            () = tokio::time::sleep(Duration::from_millis(if self.synthetic.actor.progress().phase == SyntheticTransferPhase::Sending { 1 } else { 100 })),
                if self.synthetic.sender_active() || self.synthetic.receiver.is_active() => SelectedSessionEvent::SyntheticTick,
            changed = directory_changes.changed() => SelectedSessionEvent::DirectoryChanged(changed),
            message = self.session.receive_message(), if self.sync.pending_input.is_none() => SelectedSessionEvent::Message(message),
            output = receive_sync_worker_output(&mut self.sync.worker), if self.sync.worker.is_some() => {
                SelectedSessionEvent::SyncOutput(output)
            },
            () = sleep_until(self.keepalive.next_ping_at), if self.keepalive.outstanding.is_none() => SelectedSessionEvent::PingDue,
            () = sleep_until(ping_deadline), if self.keepalive.outstanding.is_some() => SelectedSessionEvent::PingTimeout,
            () = tokio::time::sleep(Duration::from_millis(1)), if self.sync.pending_input.is_some() => SelectedSessionEvent::RetrySyncInput,
        };
        self.ensure_current()?;
        Ok(selected)
    }

    async fn handle(&mut self, event: SelectedSessionEvent) -> Step {
        match event {
            // Only wake the loop: the work happens before the next wait.
            SelectedSessionEvent::RetrySyncInput | SelectedSessionEvent::SyntheticTick => Ok(()),
            SelectedSessionEvent::ApprovalExpiry => {
                self.approvals.expire();
                Ok(())
            }
            SelectedSessionEvent::Approval(None) => {
                self.approvals.commands = None;
                Ok(())
            }
            SelectedSessionEvent::Approval(Some(command)) => self.send_approval(command).await,
            SelectedSessionEvent::SyntheticCommand(Some(command)) => {
                key_transfer::process_command(&mut *self.session, &mut self.synthetic, command)
                    .await
                    .map_err(failed)
            }
            SelectedSessionEvent::SyntheticCommand(None) | SelectedSessionEvent::Command(None) => {
                Err(SessionExit::Disconnected(
                    SessionCloseReason::AppClosing,
                    true,
                ))
            }
            SelectedSessionEvent::ObservePath => {
                self.observe_path().await;
                Ok(())
            }
            SelectedSessionEvent::SyncRoundRequested => {
                self.sync.request_round();
                Ok(())
            }
            SelectedSessionEvent::Command(Some(SessionCommand::Disconnect(reason))) => {
                Err(SessionExit::Disconnected(reason, true))
            }
            SelectedSessionEvent::DirectoryChanged(Ok(())) => {
                match self
                    .directory
                    .session_termination_reason(&self.peer, self.generation)
                {
                    Some(reason) => Err(SessionExit::Disconnected(reason, true)),
                    None => Ok(()),
                }
            }
            SelectedSessionEvent::DirectoryChanged(Err(_)) => Err(failed(SessionFailure::Protocol)),
            SelectedSessionEvent::PingDue => self.send_ping().await,
            SelectedSessionEvent::PingTimeout => Err(failed(SessionFailure::PingTimeout)),
            SelectedSessionEvent::SyncOutput(output) => self.on_sync_output(output).await,
            SelectedSessionEvent::Message(Err(error)) => Err(failed(classify_pairing_error(error))),
            SelectedSessionEvent::Message(Ok(message)) => self.on_message(message).await,
        }
    }

    /// Refreshes the route the directory shows for this connection.
    async fn observe_path(&mut self) {
        let info = tokio::time::timeout(Duration::from_millis(500), self.session.connection_info())
            .await
            .unwrap_or_default();
        let _ =
            self.directory
                .update_connection_info_for_generation(&self.peer, self.generation, info);
        self.next_observation = Instant::now() + Duration::from_secs(2);
    }

    async fn send_approval(&mut self, command: approvals::ApprovalCommand) -> Step {
        match self.approvals.command(command) {
            Some((message_type, approvals::ApprovalMessage::Request(request))) => {
                send_control_bounded(&mut *self.session, message_type, &request).await
            }
            Some((message_type, approvals::ApprovalMessage::Cancel(cancel))) => {
                send_control_bounded(&mut *self.session, message_type, &cancel).await
            }
            None => Ok(()),
        }
        .map_err(failed)
    }

    async fn send_ping(&mut self) -> Step {
        let nonce = self.keepalive.next_local_nonce;
        let next_nonce = nonce
            .checked_add(1)
            .ok_or(failed(SessionFailure::NonceSequence))?;
        send_control_bounded(
            &mut *self.session,
            proto::MessageType::SessionPing,
            &proto::SessionPing { nonce },
        )
        .await
        .map_err(failed)?;
        self.keepalive.next_local_nonce = next_nonce;
        self.keepalive.outstanding = Some((nonce, Instant::now() + SESSION_PING_TIMEOUT));
        Ok(())
    }

    /// The peer's ping must carry the next nonce; it is answered at once.
    async fn on_ping(&mut self, ping: proto::SessionPing) -> Step {
        let expected = self
            .keepalive
            .last_remote_nonce
            .checked_add(1)
            .ok_or(failed(SessionFailure::NonceSequence))?;
        if ping.nonce != expected {
            return Err(failed(SessionFailure::NonceSequence));
        }
        send_control_bounded(
            &mut *self.session,
            proto::MessageType::SessionPong,
            &proto::SessionPong { nonce: ping.nonce },
        )
        .await
        .map_err(failed)?;
        self.keepalive.last_remote_nonce = ping.nonce;
        Ok(())
    }

    /// Only the pong of the outstanding ping, and in time, keeps the session.
    fn on_pong(&mut self, pong: &proto::SessionPong) -> Step {
        match self.keepalive.outstanding {
            Some((_, deadline)) if Instant::now() >= deadline => {
                return Err(failed(SessionFailure::PingTimeout));
            }
            Some((nonce, _)) if nonce == pong.nonce => {}
            _ => return Err(failed(SessionFailure::NonceSequence)),
        }
        self.keepalive.outstanding = None;
        self.keepalive.next_ping_at = Instant::now() + SESSION_PING_INTERVAL;
        Ok(())
    }

    /// A peer over its per-second allowance ends the session.
    fn check_rate(&mut self, message: &kryptopass_protocol::DecodedMessage) -> Step {
        let now = Instant::now();
        match message.channel {
            Channel::Control => {
                let accepted = if is_sync_message(message.message_type) {
                    self.sync_rate
                        .accept_limit(now, MAX_SYNC_CONTROL_MESSAGES_PER_SECOND)
                } else {
                    self.control_rate.accept(now)
                };
                if !accepted {
                    return Err(failed(SessionFailure::ControlRate));
                }
            }
            Channel::Data => {
                if !self.data_rate.accept(now) {
                    return Err(failed(SessionFailure::DataRate));
                }
            }
        }
        Ok(())
    }

    /// Approval messages are only valid from a peer that advertised them.
    fn require_approvals(&self) -> Step {
        if self.approvals.enabled() {
            Ok(())
        } else {
            Err(failed(SessionFailure::Protocol))
        }
    }

    fn on_vault_keys(&mut self, keys: proto::VaultKeys) -> Step {
        // Only a peer that advertised the capability may send it.
        if !self.accepts_vault_keys {
            return Err(failed(SessionFailure::Protocol));
        }
        let count = keys.openpgp_fingerprints.len();
        self.directory
            .set_peer_vault_keys(&self.peer, self.generation, keys.openpgp_fingerprints)
            .map_err(|_| failed(SessionFailure::Protocol))?;
        tracing::info!(keys = count, "Peer declared its vault keys");
        Ok(())
    }

    /// Routes one decoded message. Anything not expected here is a protocol
    /// error that ends the session.
    async fn on_message(&mut self, message: kryptopass_protocol::DecodedMessage) -> Step {
        self.check_rate(&message)?;
        let channel = message.channel;
        match (message.message_type, message.payload) {
            (proto::MessageType::SessionPing, MessagePayload::SessionPing(ping)) => {
                self.on_ping(ping).await
            }
            (proto::MessageType::SessionPong, MessagePayload::SessionPong(pong)) => {
                self.on_pong(&pong)
            }
            (proto::MessageType::SessionClose, MessagePayload::SessionClose(close)) => {
                Err(match SessionCloseReason::from_wire(close.reason) {
                    Some(reason) => SessionExit::Disconnected(reason, false),
                    None => failed(SessionFailure::Protocol),
                })
            }
            (proto::MessageType::ApprovalResponse, MessagePayload::ApprovalResponse(response)) => {
                self.require_approvals()?;
                self.approvals.response(response);
                Ok(())
            }
            (proto::MessageType::ApprovalCancel, MessagePayload::ApprovalCancel(cancel)) => {
                self.require_approvals()?;
                self.approvals.remote_cancel(&cancel);
                Ok(())
            }
            // Desktop asks; it never answers a phone's request.
            (proto::MessageType::ApprovalRequest, _) => Err(failed(SessionFailure::Protocol)),
            (proto::MessageType::VaultKeys, MessagePayload::VaultKeys(keys)) => {
                self.on_vault_keys(keys)
            }
            (
                proto::MessageType::SyntheticKeyOffer
                | proto::MessageType::SyntheticKeyChunk
                | proto::MessageType::SyntheticKeyComplete,
                payload,
            ) => self.on_inbound_key_transfer(payload).await,
            (proto::MessageType::SyntheticKeyAbort, MessagePayload::SyntheticKeyAbort(abort))
                if self.synthetic.receiver.knows(&abort.transfer_id) =>
            {
                self.synthetic.receiver.remote_abort(&abort);
                self.synthetic.publish();
                Ok(())
            }
            (
                message_type @ (proto::MessageType::SyntheticKeyDecision
                | proto::MessageType::SyntheticKeyReceipt
                | proto::MessageType::SyntheticKeyAbort),
                payload,
            ) => {
                self.on_outbound_key_transfer_reply(message_type, payload);
                Ok(())
            }
            (message_type, payload)
                if is_sync_message(message_type) && channel == Channel::Control =>
            {
                self.sync.forward(message_type, payload)
            }
            (proto::MessageType::SyncChunk, payload) if channel == Channel::Data => {
                self.sync.forward(proto::MessageType::SyncChunk, payload)
            }
            _ => Err(failed(SessionFailure::Protocol)),
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
