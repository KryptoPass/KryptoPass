//! Sending with a deadline, so a stalled peer cannot hold the session loop.

use super::*;

struct SensitiveSyntheticChunk(proto::SyntheticKeyChunk);

impl Drop for SensitiveSyntheticChunk {
    fn drop(&mut self) {
        self.0.data.zeroize();
    }
}

pub(crate) async fn send_synthetic_outbound_bounded<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    outbound: SyntheticTransferOutbound,
) -> Result<(), SessionFailure> {
    match outbound {
        SyntheticTransferOutbound::Offer(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyOffer, &payload).await
        }
        SyntheticTransferOutbound::Chunk(payload) => {
            let sensitive = SensitiveSyntheticChunk(payload);
            send_control_bounded(session, proto::MessageType::SyntheticKeyChunk, &sensitive.0).await
        }
        SyntheticTransferOutbound::Complete(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyComplete, &payload).await
        }
        SyntheticTransferOutbound::Abort(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyAbort, &payload).await
        }
        SyntheticTransferOutbound::Decision(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyDecision, &payload).await
        }
        SyntheticTransferOutbound::Receipt(payload) => {
            send_control_bounded(session, proto::MessageType::SyntheticKeyReceipt, &payload).await
        }
    }
}

pub(crate) async fn send_sync_outbound_bounded<T: PeerTransport>(
    session: &mut EstablishedSession<T>,
    outbound: SyncOutbound,
) -> Result<(), SessionFailure> {
    match outbound {
        SyncOutbound::Hello(payload) => {
            send_control_bounded(session, proto::MessageType::SyncHello, &payload).await
        }
        SyncOutbound::Manifest(payload) => {
            send_control_bounded(session, proto::MessageType::SyncManifest, &payload).await
        }
        SyncOutbound::Request(payload) => {
            send_control_bounded(session, proto::MessageType::SyncRequest, &payload).await
        }
        SyncOutbound::Chunk(payload) => {
            send_control_bounded(session, proto::MessageType::SyncChunk, &payload).await
        }
        SyncOutbound::Commit(payload) => {
            send_control_bounded(session, proto::MessageType::SyncCommit, &payload).await
        }
        SyncOutbound::Ack(payload) => {
            send_control_bounded(session, proto::MessageType::SyncAck, &payload).await
        }
    }
}

pub(crate) fn classify_pairing_error(error: PairingError) -> SessionFailure {
    match error {
        PairingError::Transport(_) => SessionFailure::Transport,
        PairingError::CryptoFailure => SessionFailure::Authentication,
        PairingError::Protocol(_) | PairingError::UnexpectedMessage => SessionFailure::Protocol,
        _ => SessionFailure::Protocol,
    }
}

pub(crate) async fn send_control_bounded<T: PeerTransport, M: prost::Message>(
    session: &mut EstablishedSession<T>,
    message_type: proto::MessageType,
    payload: &M,
) -> Result<(), SessionFailure> {
    match tokio::time::timeout(
        SESSION_CONTROL_SEND_TIMEOUT,
        session.send_message(message_type, payload),
    )
    .await
    {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(classify_pairing_error(error)),
        Err(_) => Err(SessionFailure::Transport),
    }
}
