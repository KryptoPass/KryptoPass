//! Closing a session in order and telling the directory and the application why.

use super::*;

pub(crate) async fn finish_session<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    context: SessionFinishContext<'_>,
    exit: SessionExit,
) {
    let (event, connection) = match exit {
        SessionExit::Disconnected(reason, send_close) => {
            if send_close {
                let close_sent = tokio::time::timeout(
                    Duration::from_secs(2),
                    session.send_message(
                        proto::MessageType::SessionClose,
                        &proto::SessionClose {
                            reason: reason as u32,
                        },
                    ),
                )
                .await
                .is_ok_and(|result| result.is_ok());
                if close_sent {
                    wait_for_peer_close(session).await;
                }
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
            (
                SessionEvent::Disconnected {
                    peer_id: context.peer_id,
                    generation: context.generation,
                    reason,
                },
                ConnectionState::Disconnected,
            )
        }
        SessionExit::Failed(failure) => {
            // A transient carrier failure must not tell the peer to stop recovery.
            if !matches!(
                failure,
                SessionFailure::Transport
                    | SessionFailure::PingTimeout
                    | SessionFailure::SyncStorage
                    | SessionFailure::SyncTimeout
            ) {
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    session.send_message(
                        proto::MessageType::SessionClose,
                        &proto::SessionClose {
                            reason: SessionCloseReason::ProtocolError as u32,
                        },
                    ),
                )
                .await;
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
            (
                SessionEvent::Failed {
                    peer_id: context.peer_id,
                    generation: context.generation,
                    failure,
                },
                ConnectionState::Failed,
            )
        }
    };
    let _ = context.directory.update_connection_state_for_generation(
        &context.expected_peer,
        context.generation,
        connection,
    );
    let _ = context.events.send(event);
    context.lifecycle.send_replace(Some(event));
    context.closed.send_replace(true);
}

async fn wait_for_peer_close<T: PeerTransport>(session: &mut EstablishedSession<T>) {
    let deadline = Instant::now() + Duration::from_secs(2);
    for _ in 0..64 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, session.receive_message()).await {
            Ok(Ok(message)) if message.message_type == proto::MessageType::SessionClose => break,
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => break,
        }
    }
}

pub(crate) struct SessionFinishContext<'a> {
    pub(crate) directory: &'a DeviceDirectory,
    pub(crate) expected_peer: PublicIdentity,
    pub(crate) peer_id: DeviceId,
    pub(crate) generation: u64,
    pub(crate) events: &'a broadcast::Sender<SessionEvent>,
    pub(crate) lifecycle: &'a watch::Sender<Option<SessionEvent>>,
    pub(crate) closed: &'a watch::Sender<bool>,
}
