//! The first exchange of an authenticated session: versions and capabilities.

use super::*;

pub(crate) struct SessionHelloContext<'a> {
    pub(crate) local_device_id: DeviceId,
    pub(crate) options: SessionOptions,
    pub(crate) sync_version: u32,
    pub(crate) sync_rounds_version: u32,
    pub(crate) synthetic_key_transfer_version: u32,
    pub(crate) expected_peer: PublicIdentity,
    pub(crate) generation: u64,
    pub(crate) directory: &'a DeviceDirectory,
}

pub(crate) async fn exchange_session_hello<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    context: &SessionHelloContext<'_>,
    directory_changes: &mut watch::Receiver<Arc<DeviceDirectorySnapshot>>,
    commands: &mut mpsc::Receiver<SessionCommand>,
) -> Result<(u16, u32, u32, u32, u32, u32, u32), SessionExit> {
    let hello_deadline = Instant::now() + SESSION_HELLO_TIMEOUT;
    if !context
        .directory
        .session_is_current(&context.expected_peer, context.generation)
    {
        return Err(SessionExit::Disconnected(
            context
                .directory
                .session_termination_reason(&context.expected_peer, context.generation)
                .unwrap_or(SessionCloseReason::TrustRevoked),
            true,
        ));
    }
    let hello = proto::SessionHello {
        session_version: ACTIVE_SESSION_VERSION,
        device_id: context.local_device_id.as_bytes().to_vec(),
        reconnect_port: u32::from(context.options.reconnect_port),
        synthetic_key_transfer_version: context.synthetic_key_transfer_version,
        sync_version: context.sync_version,
        sync_rounds_version: context.sync_rounds_version,
        vault_keys_version: VAULT_KEYS_VERSION,
        approval_version: approvals::APPROVAL_VERSION,
        key_transfer_unavailable_reason: 0,
    };
    let send = session.send_message(proto::MessageType::SessionHello, &hello);
    tokio::select! {
        result = send => result.map_err(|error| SessionExit::Failed(classify_pairing_error(error)))?,
        command = commands.recv() => return Err(SessionExit::Disconnected(
            command.map_or(SessionCloseReason::AppClosing, |SessionCommand::Disconnect(reason)| reason),
            true,
        )),
        termination = wait_for_session_termination(context.directory, directory_changes, &context.expected_peer, context.generation) => {
            return Err(termination.map_or_else(
                SessionExit::Failed,
                |reason| SessionExit::Disconnected(reason, true),
            ));
        }
        () = sleep_until(hello_deadline) => return Err(SessionExit::Failed(SessionFailure::HelloTimeout)),
    }

    let receive = session.receive_message();
    let message = tokio::select! {
        result = receive => result.map_err(|error| SessionExit::Failed(classify_pairing_error(error)))?,
        command = commands.recv() => return Err(SessionExit::Disconnected(
            command.map_or(SessionCloseReason::AppClosing, |SessionCommand::Disconnect(reason)| reason),
            true,
        )),
        termination = wait_for_session_termination(context.directory, directory_changes, &context.expected_peer, context.generation) => {
            return Err(termination.map_or_else(
                SessionExit::Failed,
                |reason| SessionExit::Disconnected(reason, true),
            ));
        }
        () = sleep_until(hello_deadline) => return Err(SessionExit::Failed(SessionFailure::HelloTimeout)),
    };
    match (message.message_type, message.payload) {
        (proto::MessageType::SessionHello, MessagePayload::SessionHello(hello)) => {
            if hello.session_version != ACTIVE_SESSION_VERSION
                || hello.reconnect_port > u32::from(u16::MAX)
                || hello.device_id.as_slice() != context.expected_peer.device_id.as_bytes()
            {
                return Err(SessionExit::Failed(SessionFailure::HelloRejected));
            }
            Ok((
                hello.reconnect_port as u16,
                hello.sync_version,
                hello.sync_rounds_version,
                hello.synthetic_key_transfer_version,
                hello.vault_keys_version,
                hello.approval_version,
                hello.key_transfer_unavailable_reason,
            ))
        }
        (proto::MessageType::SessionClose, MessagePayload::SessionClose(close)) => {
            let Some(reason) = SessionCloseReason::from_wire(close.reason) else {
                return Err(SessionExit::Failed(SessionFailure::Protocol));
            };
            Err(SessionExit::Disconnected(reason, false))
        }
        _ => Err(SessionExit::Failed(SessionFailure::HelloRejected)),
    }
}

async fn wait_for_session_termination(
    directory: &DeviceDirectory,
    directory_changes: &mut watch::Receiver<Arc<DeviceDirectorySnapshot>>,
    peer: &PublicIdentity,
    generation: u64,
) -> Result<SessionCloseReason, SessionFailure> {
    loop {
        directory_changes
            .changed()
            .await
            .map_err(|_| SessionFailure::Protocol)?;
        if let Some(reason) = directory.session_termination_reason(peer, generation) {
            return Ok(reason);
        }
    }
}
