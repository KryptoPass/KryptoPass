//! The thread that runs one Sync session against the store, off the async loop.

use super::*;

pub(crate) enum SyncWorkerInput {
    Message(proto::MessageType, MessagePayload),
    /// One more round on the live session, asked by the person.
    RequestRound,
    Cancel,
}

pub(crate) enum SyncWorkerOutput {
    Outbound(SyncOutbound),
    Progress(SyncProgress),
    Failed(SyncSessionError),
    RoundComplete,
    Complete,
}

pub(crate) struct SyncWorker {
    pub(crate) input: SyncSender<SyncWorkerInput>,
    pub(crate) outputs: mpsc::Receiver<SyncWorkerOutput>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for SyncWorker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.input.try_send(SyncWorkerInput::Cancel);
    }
}

pub(crate) fn spawn_sync_worker(
    binding: SyncSessionBinding,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    continuous_rounds: bool,
) -> SyncWorker {
    let (input, input_rx) = sync_channel(SYNC_WORKER_INPUT_CAPACITY);
    let (output_tx, outputs) = mpsc::channel(SYNC_WORKER_OUTPUT_CAPACITY);
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    tokio::task::spawn_blocking(move || {
        let continuous_rounds = continuous_rounds && binding.automatic_rounds;
        let mut engine = match SyncSession::new(
            binding.store,
            binding.trust,
            local_device_id,
            expected_peer,
            binding.keys,
            std::time::Instant::now(),
        ) {
            Ok(engine) => engine,
            Err(error) => {
                let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                return;
            }
        };
        if continuous_rounds {
            if let Err(error) = engine.enable_continuous_rounds() {
                let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                return;
            }
        }
        let mut last_progress = engine.progress();
        if cfg!(debug_assertions) {
            tracing::info!(continuous_rounds, diagnostics = %engine.diagnostics(std::time::Instant::now()), "Sync started");
        }
        let mut round_complete_reported = false;
        let mut next_local_change_check = std::time::Instant::now();
        if output_tx
            .blocking_send(SyncWorkerOutput::Progress(last_progress))
            .is_err()
        {
            return;
        }

        loop {
            if worker_cancelled.load(Ordering::Acquire) {
                engine.cancel();
                return;
            }
            let now = std::time::Instant::now();
            if engine.is_complete() {
                if !continuous_rounds {
                    let _ = output_tx.blocking_send(SyncWorkerOutput::Progress(engine.progress()));
                    let _ = output_tx.blocking_send(SyncWorkerOutput::Complete);
                    return;
                }
                if !round_complete_reported {
                    let progress = engine.progress();
                    if progress != last_progress {
                        last_progress = progress;
                        if output_tx
                            .blocking_send(SyncWorkerOutput::Progress(progress))
                            .is_err()
                        {
                            return;
                        }
                    }
                    if output_tx
                        .blocking_send(SyncWorkerOutput::RoundComplete)
                        .is_err()
                    {
                        return;
                    }
                    round_complete_reported = true;
                }
            }
            if continuous_rounds && now >= next_local_change_check {
                match engine.has_uncovered_local_changes() {
                    Ok(true) => {
                        if !engine.request_round() {
                            let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(
                                SyncSessionError::Protocol,
                            ));
                            return;
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                        return;
                    }
                }
                next_local_change_check = now + kryptopass_sync::SESSION_LOCAL_CHANGE_POLL;
            }
            if let Err(error) = engine.check_time(now) {
                if cfg!(debug_assertions) {
                    tracing::warn!(error = %error, diagnostics = %engine.diagnostics(now), "Sync deadline or trust check failed");
                }
                let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                return;
            }
            match engine.poll_output(now) {
                Ok(Some(outbound)) => {
                    if cfg!(debug_assertions) {
                        let counts = match &outbound {
                            SyncOutbound::Hello(message) => {
                                format!("hello objects={}", message.known_object_count)
                            }
                            SyncOutbound::Manifest(message) => format!(
                                "manifest page={} last={} heads={}",
                                message.page_index,
                                message.last_page,
                                message.heads.len()
                            ),
                            SyncOutbound::Request(message) => {
                                format!("request wanted={}", message.wanted.len())
                            }
                            SyncOutbound::Chunk(message) => format!(
                                "chunk index={} chunks={} bytes={}",
                                message.chunk_index,
                                message.chunk_count,
                                message.data.len()
                            ),
                            SyncOutbound::Commit(_) => "commit".to_owned(),
                            SyncOutbound::Ack(message) => format!("ack status={}", message.status),
                        };
                        if !matches!(&outbound, SyncOutbound::Chunk(message) if message.chunk_index != 0 && message.chunk_index + 1 != message.chunk_count)
                        {
                            tracing::debug!(message = %counts, diagnostics = %engine.diagnostics(now), "Sync tx_queued");
                        }
                    }
                    if output_tx
                        .blocking_send(SyncWorkerOutput::Outbound(outbound))
                        .is_err()
                    {
                        return;
                    }
                    let progress = engine.progress();
                    if progress != last_progress {
                        last_progress = progress;
                        if output_tx
                            .blocking_send(SyncWorkerOutput::Progress(progress))
                            .is_err()
                        {
                            return;
                        }
                    }
                    if !engine.is_complete() {
                        round_complete_reported = false;
                    }
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    if cfg!(debug_assertions) {
                        tracing::warn!(error = %error, diagnostics = %engine.diagnostics(now), "Sync output failed");
                    }
                    let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                    return;
                }
            }
            let now = std::time::Instant::now();
            let wait = if continuous_rounds && engine.is_complete() {
                next_local_change_check.saturating_duration_since(now)
            } else {
                let timeout = engine.timeout_after(now);
                if continuous_rounds {
                    timeout.min(kryptopass_sync::SESSION_LOCAL_CHANGE_POLL)
                } else {
                    timeout
                }
            };
            let received = input_rx.recv_timeout(wait.max(Duration::from_millis(1)));
            let input = match received {
                Ok(input) => input,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    engine.cancel();
                    return;
                }
            };
            if worker_cancelled.load(Ordering::Acquire) {
                engine.cancel();
                return;
            }
            let result = match input {
                SyncWorkerInput::Message(message_type, payload) => {
                    if cfg!(debug_assertions) {
                        let counts = match &payload {
                            MessagePayload::SyncHello(message) => {
                                format!("objects={}", message.known_object_count)
                            }
                            MessagePayload::SyncManifest(message) => format!(
                                "page={} last={} heads={}",
                                message.page_index,
                                message.last_page,
                                message.heads.len()
                            ),
                            MessagePayload::SyncRequest(message) => {
                                format!("wanted={}", message.wanted.len())
                            }
                            MessagePayload::SyncChunk(message) => format!(
                                "chunk={} chunks={} bytes={}",
                                message.chunk_index,
                                message.chunk_count,
                                message.data.len()
                            ),
                            MessagePayload::SyncAck(message) => {
                                format!("status={}", message.status)
                            }
                            _ => String::new(),
                        };
                        if !matches!(&payload, MessagePayload::SyncChunk(message) if message.chunk_index != 0 && message.chunk_index + 1 != message.chunk_count)
                        {
                            tracing::debug!(message_type = message_type.as_str_name(), message = %counts, diagnostics = %engine.diagnostics(std::time::Instant::now()), "Sync rx");
                        }
                    }
                    engine.handle_message(message_type, payload, std::time::Instant::now())
                }
                SyncWorkerInput::RequestRound => {
                    if engine.request_round() {
                        tracing::info!("Sync: round requested on the live session");
                    } else {
                        tracing::info!(
                            "Sync: a round was requested but this session runs a single round"
                        );
                    }
                    Ok(())
                }
                SyncWorkerInput::Cancel => {
                    engine.cancel();
                    return;
                }
            };
            if let Err(error) = result {
                if cfg!(debug_assertions) {
                    tracing::warn!(error = %error, diagnostics = %engine.diagnostics(std::time::Instant::now()), "Sync input rejected");
                }
                let _ = output_tx.blocking_send(SyncWorkerOutput::Failed(error));
                return;
            }
            let progress = engine.progress();
            if progress != last_progress {
                if cfg!(debug_assertions) {
                    tracing::info!(diagnostics = %engine.diagnostics(std::time::Instant::now()), "Sync progress");
                }
                last_progress = progress;
                if output_tx
                    .blocking_send(SyncWorkerOutput::Progress(progress))
                    .is_err()
                {
                    return;
                }
            }
            if !engine.is_complete() {
                round_complete_reported = false;
            }
        }
    });
    SyncWorker {
        input,
        outputs,
        cancelled,
    }
}
