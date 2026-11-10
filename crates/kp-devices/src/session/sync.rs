//! Sync over a connected session: the worker thread, its bounded queue and
//! what happens when either side of it fails.

use super::*;

/// The session's side of the Sync worker.
pub(super) struct SyncLink {
    pub(super) worker: Option<SyncWorker>,
    progress: watch::Sender<Option<SyncProgress>>,
    /// At most one decoded frame held while the worker's bounded queue is
    /// busy, with the time it may keep waiting. Meanwhile the loop keeps
    /// draining worker output and observing cancellation instead of treating
    /// an ordinary scheduler or SQLite delay as a malformed peer message.
    pub(super) pending_input: Option<(SyncWorkerInput, Instant)>,
    /// A Sync failure suspends Sync for the rest of this session instead of
    /// closing it; only revoked trust still ends the authenticated connection.
    suspended: bool,
}

impl SyncLink {
    pub(super) fn new(
        worker: Option<SyncWorker>,
        progress: watch::Sender<Option<SyncProgress>>,
    ) -> Self {
        Self {
            worker,
            progress,
            pending_input: None,
            suspended: false,
        }
    }

    fn suspend(&mut self, reason: &'static str) {
        if !self.suspended {
            tracing::warn!(
                reason,
                "Sync suspended for this session; the connection stays up"
            );
        }
        self.worker = None;
        self.pending_input = None;
        self.suspended = true;
        self.progress.send_modify(|progress| {
            if let Some(progress) = progress {
                progress.state = kryptopass_sync::SyncProgressState::Failed;
            }
        });
    }

    /// Offers the held frame to the worker again, until its deadline.
    pub(super) fn retry_pending(&mut self) -> Step {
        let Some((input, deadline)) = self.pending_input.take() else {
            return Ok(());
        };
        let Some(worker) = self.worker.as_ref() else {
            if self.suspended {
                return Ok(());
            }
            tracing::warn!("Sync input arrived without a sync worker");
            return Err(failed(SessionFailure::Sync));
        };
        match worker.input.try_send(input) {
            Ok(()) => {}
            Err(StdTrySendError::Full(input)) if Instant::now() < deadline => {
                self.pending_input = Some((input, deadline));
            }
            Err(_) => self.suspend("worker input queue full past its deadline or closed"),
        }
        Ok(())
    }

    pub(super) fn request_round(&self) {
        match self.worker.as_ref() {
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
        }
    }

    /// Hands a peer's Sync message to the worker. A full queue holds the
    /// frame for a moment; without a worker the message is a protocol error.
    pub(super) fn forward(
        &mut self,
        message_type: proto::MessageType,
        payload: MessagePayload,
    ) -> Step {
        // The peer may still send its pending round after a local failure.
        if self.suspended {
            return Ok(());
        }
        let Some(worker) = self.worker.as_ref() else {
            return Err(failed(SessionFailure::Protocol));
        };
        match worker
            .input
            .try_send(SyncWorkerInput::Message(message_type, payload))
        {
            Ok(()) => {}
            Err(StdTrySendError::Full(input)) => {
                self.pending_input = Some((input, Instant::now() + Duration::from_secs(2)));
            }
            Err(StdTrySendError::Disconnected(_)) => self.suspend("worker closed"),
        }
        Ok(())
    }
}

impl<T: PeerTransport> Connected<'_, T> {
    /// What the worker produced: a message to send, progress, or its end.
    pub(super) async fn on_sync_output(&mut self, output: Option<SyncWorkerOutput>) -> Step {
        match output {
            Some(SyncWorkerOutput::Outbound(outbound)) => {
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
                send_sync_outbound_bounded(&mut *self.session, outbound)
                    .await
                    .map_err(failed)?;
                if cfg!(debug_assertions) {
                    if let Some(kind) = kind {
                        tracing::debug!(message_type = kind, "Sync tx_sent");
                    }
                }
            }
            Some(SyncWorkerOutput::Progress(progress)) => {
                self.sync.progress.send_replace(Some(progress));
            }
            Some(SyncWorkerOutput::Failed(error)) => {
                tracing::warn!(error = %error, "Sync worker failed");
                if matches!(error, SyncSessionError::TrustRevoked) {
                    return Err(SessionExit::Disconnected(
                        SessionCloseReason::TrustRevoked,
                        true,
                    ));
                }
                self.sync.suspend("worker failed");
            }
            Some(SyncWorkerOutput::Complete) => self.sync.worker = None,
            Some(SyncWorkerOutput::RoundComplete) => {}
            None => self.sync.suspend("worker stopped without a result"),
        }
        Ok(())
    }
}
