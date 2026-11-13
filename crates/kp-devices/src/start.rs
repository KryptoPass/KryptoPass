//! Starting a session task: the public entry points and the wiring they share.

use super::*;

/// Starts one owner task for a pairing outcome's authenticated Noise session.
///
/// The `EstablishedSession` can only be obtained after `PairingEngine::run`
/// successfully persists trust. The task confirms that the exact peer remains
/// trusted, exchanges bilateral session hellos, and only then publishes
/// `Connected`. The session handle is the sole mutable owner of its crypto
/// state; no cipher state or sequence counter is recreated here.
pub fn start_session<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_with_options(
        directory,
        session,
        local_device_id,
        expected_peer,
        SessionOptions::default(),
    )
}

/// Starts a session while advertising the selected local LAN reconnect port.
///
/// Port zero keeps the endpoint unavailable for LAN resume. The remote port is
/// surfaced only after the authenticated bilateral session hello succeeds.
pub fn start_session_with_options<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    options: SessionOptions,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_internal(
        directory,
        session,
        local_device_id,
        expected_peer,
        options,
        None,
        false,
    )
}

/// Starts a session with an explicit ledger binding and default reconnect options.
pub fn start_session_with_sync<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    sync_binding: SyncSessionBinding,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_with_options_and_sync(
        directory,
        session,
        local_device_id,
        expected_peer,
        SessionOptions::default(),
        sync_binding,
    )
}

/// Starts a session with an explicit ledger binding and reconnect options.
pub fn start_session_with_options_and_sync<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    options: SessionOptions,
    sync_binding: SyncSessionBinding,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_internal(
        directory,
        session,
        local_device_id,
        expected_peer,
        options,
        Some(sync_binding),
        false,
    )
}

/// Starts the unique owner with explicit optional extensions. Production callers
/// keep synthetic provisioning disabled; laboratory callers opt in locally.
pub fn start_session_with_extensions<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    options: SessionOptions,
    sync_binding: Option<SyncSessionBinding>,
    synthetic_key_transfer: bool,
) -> Result<SessionHandle, DeviceDirectoryError> {
    start_session_internal(
        directory,
        session,
        local_device_id,
        expected_peer,
        options,
        sync_binding,
        synthetic_key_transfer,
    )
}

fn start_session_internal<T: PeerTransport + 'static>(
    directory: DeviceDirectory,
    mut session: EstablishedSession<T>,
    local_device_id: DeviceId,
    expected_peer: PublicIdentity,
    options: SessionOptions,
    sync_binding: Option<SyncSessionBinding>,
    synthetic_key_transfer: bool,
) -> Result<SessionHandle, DeviceDirectoryError> {
    let synthetic_key_transfer = synthetic_key_transfer && cfg!(debug_assertions);
    if sync_binding.as_ref().is_some_and(|binding| {
        binding.keys.is_empty() || binding.keys.len() > kryptopass_sync::SYNC_MAX_RECIPIENTS
    }) {
        return Err(DeviceDirectoryError::InvalidSyncBinding);
    }
    if directory.local_device_id() != local_device_id
        || session.local_device_id() != local_device_id
    {
        return Err(DeviceDirectoryError::LocalDeviceMismatch);
    }
    expected_peer
        .validate()
        .map_err(|_| DeviceDirectoryError::InvalidIdentity)?;
    if session.peer_identity() != expected_peer
        || expected_peer.device_id == local_device_id
        || !directory.contains_exact_peer(&expected_peer)
    {
        return Err(DeviceDirectoryError::PeerNotTrusted);
    }
    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| DeviceDirectoryError::RuntimeUnavailable)?;
    let generation = directory.begin_session(&expected_peer)?;

    let peer_id = expected_peer.device_id;
    let (command_tx, mut command_rx) = mpsc::channel(SESSION_COMMAND_CAPACITY);
    let sync_round = Arc::new(tokio::sync::Notify::new());
    let task_sync_round = Arc::clone(&sync_round);
    let (event_tx, _event_rx) = broadcast::channel(SESSION_EVENT_CAPACITY);
    let (lifecycle_tx, lifecycle_rx) = watch::channel(None);
    let (sync_progress_tx, sync_progress_rx) = watch::channel(None);
    let (closed_tx, closed_rx) = watch::channel(false);
    let (synthetic_tx, synthetic_rx) = mpsc::channel(2);
    let (synthetic_cancel_tx, synthetic_cancel_rx) = watch::channel(None);
    let (synthetic_progress_tx, synthetic_progress_rx) =
        watch::channel(SyntheticTransferSender::new(false).progress());
    let inbound_material: InboundMaterialSlot = Arc::new(Mutex::new(None));
    let owner_inbound_material = Arc::clone(&inbound_material);
    let owner_directory = directory.clone();
    let owner_events = event_tx.clone();
    let mut directory_changes = directory.subscribe();
    runtime.spawn(async move {
        let session_started = Instant::now();
        let local_sync_rounds_version = u32::from(
            sync_binding
                .as_ref()
                .is_some_and(|binding| binding.automatic_rounds),
        );
        let hello_context = SessionHelloContext {
            local_device_id,
            options,
            sync_version: u32::from(sync_binding.is_some()),
            sync_rounds_version: local_sync_rounds_version,
            synthetic_key_transfer_version: if synthetic_key_transfer {
                KEY_TRANSFER_VERSION
            } else {
                0
            },
            expected_peer,
            generation,
            directory: &owner_directory,
        };
        let initial = exchange_session_hello(
            &mut session,
            &hello_context,
            &mut directory_changes,
            &mut command_rx,
        )
        .await;
        let exit = match initial {
            Ok((
                remote_reconnect_port,
                remote_sync_version,
                remote_sync_rounds_version,
                remote_synthetic_version,
                remote_vault_keys_version,
                remote_approval_version,
                remote_key_transfer_unavailable_reason,
            )) if owner_directory.session_is_current(&expected_peer, generation) => {
                if owner_directory
                    .update_connection_state_for_generation(
                        &expected_peer,
                        generation,
                        ConnectionState::Connected,
                    )
                    .is_err()
                {
                    SessionExit::Disconnected(
                        owner_directory
                            .session_termination_reason(&expected_peer, generation)
                            .unwrap_or(SessionCloseReason::TrustRevoked),
                        true,
                    )
                } else {
                    let event = SessionEvent::Connected {
                        peer_id,
                        generation,
                        reconnect_port: remote_reconnect_port,
                    };
                    let _ = owner_events.send(event);
                    lifecycle_tx.send_replace(Some(event));
                    tracing::info!(
                        elapsed_ms = session_started.elapsed().as_secs_f64() * 1000.0,
                        "authenticated session hello complete"
                    );
                    let sync_enabled = sync_binding.is_some() && remote_sync_version == 1;
                    let continuous_rounds = sync_enabled
                        && local_sync_rounds_version == 1
                        && remote_sync_rounds_version == 1;
                    tracing::info!(
                        local_binding = sync_binding.is_some(),
                        remote_sync_version,
                        local_sync_rounds_version,
                        remote_sync_rounds_version,
                        sync_enabled,
                        continuous_rounds,
                        "Sync negotiation"
                    );
                    let sync_worker = if sync_enabled {
                        sync_binding.map(|binding| {
                            spawn_sync_worker(
                                binding,
                                local_device_id,
                                expected_peer,
                                continuous_rounds,
                            )
                        })
                    } else {
                        None
                    };
                    let effective_synthetic_version = if synthetic_key_transfer {
                        remote_synthetic_version.min(KEY_TRANSFER_VERSION)
                    } else {
                        0
                    };
                    tracing::info!(
                        local_enabled = synthetic_key_transfer,
                        local_version = KEY_TRANSFER_VERSION,
                        remote_synthetic_version,
                        effective_synthetic_version,
                        remote_key_transfer_unavailable_reason,
                        "Key transfer negotiation"
                    );
                    let synthetic_actor =
                        SyntheticTransferSender::with_version(effective_synthetic_version)
                            .with_peer_unavailable_reason(remote_key_transfer_unavailable_reason);
                    let synthetic_receiver = SyntheticTransferReceiver::new(
                        effective_synthetic_version,
                        owner_inbound_material,
                    );
                    synthetic_progress_tx.send_replace(synthetic_actor.progress());
                    run_connected_session(
                        &mut session,
                        &owner_directory,
                        &mut directory_changes,
                        &mut command_rx,
                        expected_peer,
                        generation,
                        SyncRuntime {
                            worker: sync_worker,
                            round_requests: task_sync_round,
                            progress: sync_progress_tx.clone(),
                            accepts_vault_keys: remote_vault_keys_version == VAULT_KEYS_VERSION,
                            approvals: approvals::ApprovalRuntime::new(
                                expected_peer.device_id,
                                generation,
                                remote_approval_version,
                            ),
                            synthetic: SyntheticRuntime {
                                actor: synthetic_actor,
                                receiver: synthetic_receiver,
                                commands: synthetic_rx,
                                cancel: synthetic_cancel_rx,
                                progress: synthetic_progress_tx.clone(),
                            },
                        },
                    )
                    .await
                }
            }
            Ok(_) => SessionExit::Disconnected(
                owner_directory
                    .session_termination_reason(&expected_peer, generation)
                    .unwrap_or(SessionCloseReason::TrustRevoked),
                true,
            ),
            Err(exit) => exit,
        };
        tracing::info!(
            reason = ?exit,
            elapsed_ms = session_started.elapsed().as_secs_f64() * 1000.0,
            "authenticated session ending"
        );
        finish_session(
            &mut session,
            SessionFinishContext {
                directory: &owner_directory,
                expected_peer,
                peer_id,
                generation,
                events: &owner_events,
                lifecycle: &lifecycle_tx,
                closed: &closed_tx,
            },
            exit,
        )
        .await;
    });

    Ok(SessionHandle {
        peer_id,
        directory,
        commands: command_tx,
        events: event_tx,
        lifecycle: lifecycle_rx,
        sync_progress: sync_progress_rx,
        synthetic_commands: synthetic_tx,
        synthetic_cancel: synthetic_cancel_tx,
        synthetic_progress: synthetic_progress_rx,
        inbound_material,
        closed: closed_rx,
        sync_round,
        generation,
    })
}
