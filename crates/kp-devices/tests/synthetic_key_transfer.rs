#![allow(clippy::expect_used)] // Assertions name the synthetic protocol boundary under test.

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use kryptopass_devices::{
    ConnectionState, DeviceDirectory, SYNTHETIC_KEY_CHUNK_BYTES, SYNTHETIC_KEY_TIMEOUT,
    SessionCloseReason, SessionEvent, SessionHandle, SessionOptions, SyntheticKeyTransferSend,
    SyntheticTransferError, SyntheticTransferOutbound, SyntheticTransferPhase,
    SyntheticTransferSender, start_session, start_session_with_extensions,
};
use kryptopass_identity::{DeviceIdentity, SqliteTrustStore};
use kryptopass_pairing::{
    EstablishedSession, LocalApproval, LocalDecision, PairingConfig, PairingEngine,
    PairingInvitation, PairingRole, SasDisplay,
};
use kryptopass_protocol::{MessagePayload, proto};
use kryptopass_transport::{
    ConnectionInfo, MemoryTransport, MemoryTransportConfig, PeerTransport, TransportError,
    TransportFrame,
};
use sha2::{Digest, Sha256};
use tokio::time::{Duration, Instant};

const TRANSFER_ID: [u8; 16] = [0x35; 16];
const FINGERPRINT: [u8; 20] = [0x42; 20];
const CHALLENGE_SHA256: [u8; 32] = [0x91; 32];

struct TestProfile(PathBuf);

impl TestProfile {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kryptopass-synthetic-transfer-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated temporary device profile");
        #[cfg(unix)]
        fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .expect("restrict temporary profile to its owner");
        Self(path)
    }

    fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TestProfile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Default)]
struct SentChannels(Arc<Mutex<Vec<kryptopass_protocol::Channel>>>);

impl SentChannels {
    fn record(&self, channel: kryptopass_protocol::Channel) {
        self.0
            .lock()
            .expect("test channel trace is not poisoned")
            .push(channel);
    }

    fn count(&self, channel: kryptopass_protocol::Channel) -> usize {
        self.0
            .lock()
            .expect("test channel trace is not poisoned")
            .iter()
            .filter(|sent| **sent == channel)
            .count()
    }
}

struct RecordingTransport {
    inner: MemoryTransport,
    sent: SentChannels,
}

#[async_trait]
impl PeerTransport for RecordingTransport {
    async fn connection_info(&mut self) -> ConnectionInfo {
        self.inner.connection_info().await
    }

    async fn send(
        &mut self,
        channel: kryptopass_protocol::Channel,
        data: &[u8],
    ) -> Result<(), TransportError> {
        self.inner.send(channel, data).await?;
        self.sent.record(channel);
        Ok(())
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        self.inner.receive().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

struct Approve;

#[async_trait]
impl LocalApproval for Approve {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        LocalDecision::Approve
    }
}

struct PairedSessions {
    _owner_profile: TestProfile,
    _peer_profile: TestProfile,
    owner_identity: DeviceIdentity,
    peer_identity: DeviceIdentity,
    owner_session: EstablishedSession<RecordingTransport>,
    peer_session: EstablishedSession<RecordingTransport>,
    owner_sent: SentChannels,
    peer_sent: SentChannels,
}

async fn pair_ephemeral_sessions() -> PairedSessions {
    let owner_profile = TestProfile::new();
    let peer_profile = TestProfile::new();
    let owner_identity = DeviceIdentity::generate().expect("generate ephemeral owner identity");
    let peer_identity = DeviceIdentity::generate().expect("generate ephemeral peer identity");

    let mut peer_store = SqliteTrustStore::open_new(peer_profile.path(), peer_identity.device_id())
        .expect("create temporary peer trust database");
    let invitation = PairingInvitation::create(&mut peer_store, 10_000, Duration::from_secs(60))
        .expect("create a temporary invitation");
    let (owner_inner, peer_inner) = MemoryTransport::pair(MemoryTransportConfig::default())
        .expect("create the authenticated in-memory carrier");
    let owner_sent = SentChannels::default();
    let peer_sent = SentChannels::default();
    let owner_transport = RecordingTransport {
        inner: owner_inner,
        sent: owner_sent.clone(),
    };
    let peer_transport = RecordingTransport {
        inner: peer_inner,
        sent: peer_sent.clone(),
    };
    let mut owner_approval = Approve;
    let mut peer_approval = Approve;
    let owner_config = PairingConfig::pairing([0x51; 16], PairingRole::Initiator, 10_000);
    let peer_config = PairingConfig::pairing([0x51; 16], PairingRole::Responder, 10_000);

    let (owner_pairing, peer_pairing) = tokio::join!(
        PairingEngine::new(
            owner_transport,
            &owner_identity,
            SqliteTrustStore::open_new(owner_profile.path(), owner_identity.device_id())
                .expect("create temporary owner trust database"),
            owner_config,
        )
        .expect("construct owner Noise pairing")
        .run(Some(&invitation), &mut owner_approval),
        PairingEngine::new(peer_transport, &peer_identity, peer_store, peer_config)
            .expect("construct peer Noise pairing")
            .run(Some(&invitation), &mut peer_approval),
    );

    PairedSessions {
        _owner_profile: owner_profile,
        _peer_profile: peer_profile,
        owner_identity,
        peer_identity,
        owner_session: owner_pairing
            .expect("owner Noise pairing authenticates")
            .session,
        peer_session: peer_pairing
            .expect("peer Noise pairing authenticates")
            .session,
        owner_sent,
        peer_sent,
    }
}

async fn wait_until_connected(handle: &SessionHandle) {
    let mut lifecycle = handle.subscribe_lifecycle();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(
                *lifecycle.borrow_and_update(),
                Some(SessionEvent::Connected { .. })
            ) {
                return;
            }
            lifecycle
                .changed()
                .await
                .expect("session lifecycle remains observable");
        }
    })
    .await
    .expect("both active-session hellos should complete");
}

async fn wait_for_transfer_phase(
    progress: &mut tokio::sync::watch::Receiver<kryptopass_devices::SyntheticTransferProgress>,
    phase: SyntheticTransferPhase,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if progress.borrow_and_update().phase == phase {
                return;
            }
            progress
                .changed()
                .await
                .expect("synthetic transfer progress remains observable");
        }
    })
    .await
    .expect("the transfer owner should reach the expected phase");
}

async fn wait_for_more_control_frames(owner: &SentChannels, peer: &SentChannels) {
    let owner_before = owner.count(kryptopass_protocol::Channel::Control);
    let peer_before = peer.count(kryptopass_protocol::Channel::Control);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if owner.count(kryptopass_protocol::Channel::Control) > owner_before
                && peer.count(kryptopass_protocol::Channel::Control) > peer_before
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("session keepalive control frames should continue on both owners");
}

#[tokio::test]
async fn default_session_disables_transfer_keeps_keepalive_and_rejects_stale_generation() {
    let paired = pair_ephemeral_sessions().await;
    let owner_id = paired.owner_identity.device_id();
    let peer_id = paired.peer_identity.device_id();
    let owner_public = paired.owner_identity.public_identity();
    let peer_public = paired.peer_identity.public_identity();
    let owner_directory = DeviceDirectory::new(owner_id);
    owner_directory
        .replace_trusted(vec![peer_public])
        .expect("owner directory contains the paired peer");
    let peer_directory = DeviceDirectory::new(peer_id);
    peer_directory
        .replace_trusted(vec![owner_public])
        .expect("peer directory contains the paired owner");

    let owner_handle = start_session(
        owner_directory.clone(),
        paired.owner_session,
        owner_id,
        peer_public,
    )
    .expect("start default owner session");
    let peer_handle = start_session(
        peer_directory.clone(),
        paired.peer_session,
        peer_id,
        owner_public,
    )
    .expect("start default peer session");

    wait_until_connected(&owner_handle).await;
    wait_until_connected(&peer_handle).await;

    let control = owner_handle.synthetic_key_transfer_control();
    let generation = owner_handle.generation();
    assert_eq!(control.generation(), generation);
    assert_eq!(
        control.synthetic_key_transfer_progress().phase,
        SyntheticTransferPhase::Unsupported
    );
    assert!(matches!(
        control.start_synthetic_key_transfer(generation, prepared(TRANSFER_ID, 8)),
        Err(SyntheticTransferError::Unsupported)
    ));

    wait_for_more_control_frames(&paired.owner_sent, &paired.peer_sent).await;
    assert_eq!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    );
    assert_eq!(
        peer_directory
            .snapshot()
            .get(&owner_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    );

    owner_directory
        .update_connection_state(peer_id, ConnectionState::Disconnected)
        .expect("a newer directory generation supersedes this owner");
    let replaced_generation = owner_directory
        .snapshot()
        .get(&peer_id)
        .expect("trusted peer remains visible")
        .connection_generation;
    assert_ne!(replaced_generation, generation);
    assert!(matches!(
        control.start_synthetic_key_transfer(generation, prepared(TRANSFER_ID, 8)),
        Err(SyntheticTransferError::WrongSession)
    ));
}

#[tokio::test]
async fn owner_roundtrips_transfer_only_after_acceptance_and_valid_receipt() {
    let paired = pair_ephemeral_sessions().await;
    let owner_id = paired.owner_identity.device_id();
    let peer_id = paired.peer_identity.device_id();
    let peer_public = paired.peer_identity.public_identity();
    let owner_sent = paired.owner_sent.clone();
    let peer_sent = paired.peer_sent.clone();
    let owner_directory = DeviceDirectory::new(owner_id);
    owner_directory
        .replace_trusted(vec![peer_public])
        .expect("owner directory contains the authenticated test peer");

    let mut peer_session = paired.peer_session;
    let mut owner_handle = start_session_with_extensions(
        owner_directory.clone(),
        paired.owner_session,
        owner_id,
        peer_public,
        SessionOptions::default(),
        None,
        true,
    )
    .expect("start the explicitly enabled synthetic sender");
    let owner_hello = peer_session
        .receive_message()
        .await
        .expect("receive the owner's authenticated session hello");
    assert!(matches!(
        owner_hello.payload,
        MessagePayload::SessionHello(ref hello) if hello.synthetic_key_transfer_version == 2
    ));
    peer_session
        .send_message(
            proto::MessageType::SessionHello,
            &proto::SessionHello {
                session_version: 1,
                device_id: peer_id.as_bytes().to_vec(),
                reconnect_port: 0,
                synthetic_key_transfer_version: 1,
                sync_version: 0,
                sync_rounds_version: 0,
            },
        )
        .await
        .expect("advertise bilateral laboratory support in the authenticated hello");
    wait_until_connected(&owner_handle).await;

    let control = owner_handle.synthetic_key_transfer_control();
    let generation = owner_handle.generation();
    let mut progress = control.subscribe_synthetic_key_transfer();
    wait_for_transfer_phase(&mut progress, SyntheticTransferPhase::Ready).await;

    let byte_count = SYNTHETIC_KEY_CHUNK_BYTES + 3;
    let expected_key = (0..byte_count)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let expected_digest = Sha256::digest(&expected_key).to_vec();
    control
        .start_synthetic_key_transfer(generation, prepared(TRANSFER_ID, byte_count))
        .expect("queue a locally approved operation for this exact owner generation");

    let offer = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let incoming = peer_session
                .receive_message()
                .await
                .expect("receive an authenticated owner frame");
            match incoming.payload {
                MessagePayload::SyntheticKeyOffer(offer) => break offer,
                MessagePayload::SessionPing(ping) => {
                    peer_session
                        .send_message(
                            proto::MessageType::SessionPong,
                            &proto::SessionPong { nonce: ping.nonce },
                        )
                        .await
                        .expect("answer the active-session keepalive");
                }
                _ => panic!("the owner must offer before any key chunks"),
            }
        }
    })
    .await
    .expect("the active owner should send its offer");
    assert_eq!(offer.transfer_id, TRANSFER_ID);
    assert_eq!(offer.openpgp_fingerprint, FINGERPRINT);
    assert_eq!(offer.total_size, byte_count as u64);
    assert_eq!(offer.sha256_digest, expected_digest);
    assert_eq!(offer.encrypted_challenge.as_slice(), &[0x10, 0x20, 0x30]);
    wait_for_transfer_phase(&mut progress, SyntheticTransferPhase::WaitingAcceptance).await;

    let data_before_acceptance = owner_sent.count(kryptopass_protocol::Channel::Data);
    assert_eq!(
        data_before_acceptance, 0,
        "an offer alone cannot send key bytes"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        owner_sent.count(kryptopass_protocol::Channel::Data),
        data_before_acceptance,
        "the sender waits for the peer's explicit acceptance before any data frame"
    );
    peer_session
        .send_message(
            proto::MessageType::SyntheticKeyDecision,
            &proto::SyntheticKeyDecision {
                transfer_id: TRANSFER_ID.to_vec(),
                accepted: true,
            },
        )
        .await
        .expect("send the peer's explicit acceptance");

    let expected_chunk_count = byte_count.div_ceil(SYNTHETIC_KEY_CHUNK_BYTES);
    let mut received_key = Vec::with_capacity(byte_count);
    let mut received_indexes = Vec::with_capacity(expected_chunk_count);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let incoming = peer_session
                .receive_message()
                .await
                .expect("receive the accepted owner's transfer frames");
            match incoming.payload {
                MessagePayload::SyntheticKeyChunk(chunk) => {
                    let expected_index = received_indexes.len();
                    let start = expected_index * SYNTHETIC_KEY_CHUNK_BYTES;
                    let expected_length = (byte_count - start).min(SYNTHETIC_KEY_CHUNK_BYTES);
                    assert_eq!(chunk.transfer_id, TRANSFER_ID);
                    assert_eq!(chunk.chunk_index as usize, expected_index);
                    assert_eq!(chunk.chunk_count as usize, expected_chunk_count);
                    assert_eq!(chunk.total_size, byte_count as u64);
                    assert_eq!(chunk.sha256_digest, expected_digest);
                    assert_eq!(chunk.data.len(), expected_length);
                    received_indexes.push(expected_index);
                    received_key.extend_from_slice(&chunk.data);
                }
                MessagePayload::SyntheticKeyComplete(complete) => {
                    assert_eq!(complete.transfer_id, TRANSFER_ID);
                    assert_eq!(received_indexes.len(), expected_chunk_count);
                    assert_eq!(received_key.len(), byte_count);
                    assert!(
                        received_key.as_slice() == expected_key.as_slice(),
                        "ordered chunks reconstruct the exact opaque prepared bytes"
                    );
                    assert_eq!(
                        Sha256::digest(&received_key).as_slice(),
                        expected_digest.as_slice()
                    );
                    break;
                }
                MessagePayload::SessionPing(ping) => {
                    peer_session
                        .send_message(
                            proto::MessageType::SessionPong,
                            &proto::SessionPong { nonce: ping.nonce },
                        )
                        .await
                        .expect("answer the active-session keepalive");
                }
                _ => panic!("the sender must finish the ordered object before receipt"),
            }
        }
    })
    .await
    .expect("accepted chunks and completion should arrive in order");
    assert_eq!(
        owner_sent.count(kryptopass_protocol::Channel::Data),
        expected_chunk_count,
        "each data frame is one bounded ordered chunk"
    );
    wait_for_transfer_phase(&mut progress, SyntheticTransferPhase::WaitingReceipt).await;

    // This opaque test receipt exercises the sender state machine only; no remote durable store is involved.
    let receipt = proto::SyntheticKeyReceipt {
        transfer_id: TRANSFER_ID.to_vec(),
        openpgp_fingerprint: offer.openpgp_fingerprint,
        challenge_sha256: CHALLENGE_SHA256.to_vec(),
    };
    peer_session
        .send_message(proto::MessageType::SyntheticKeyReceipt, &receipt)
        .await
        .expect("send the matching synthetic receipt");
    wait_for_transfer_phase(&mut progress, SyntheticTransferPhase::Complete).await;

    let peer_task = tokio::spawn(async move {
        loop {
            match peer_session
                .receive_message()
                .await
                .expect("the authenticated session remains active after the transfer")
                .payload
            {
                MessagePayload::SessionPing(ping) => {
                    peer_session
                        .send_message(
                            proto::MessageType::SessionPong,
                            &proto::SessionPong { nonce: ping.nonce },
                        )
                        .await
                        .expect("answer keepalive after the transfer");
                }
                MessagePayload::SessionClose(close) => {
                    peer_session
                        .send_message(proto::MessageType::SessionClose, &close)
                        .await
                        .expect("complete orderly close after the transfer");
                    break;
                }
                _ => {}
            }
        }
    });
    wait_for_more_control_frames(&owner_sent, &peer_sent).await;
    assert_eq!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected),
        "transfer completion leaves the authenticated session connected"
    );

    owner_handle
        .disconnect(SessionCloseReason::UserRequested)
        .expect("close the owner session cleanly after keepalive verification");
    owner_handle.wait_closed().await;
    peer_task
        .await
        .expect("raw authenticated peer should finish the close exchange");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObservedTransferFrame {
    Offer,
    Chunk,
    Abort,
    RemoteAbortRoundtrip,
}

#[tokio::test]
async fn queued_start_then_cancel_is_processed_before_any_key_chunk() {
    let paired = pair_ephemeral_sessions().await;
    let owner_id = paired.owner_identity.device_id();
    let peer_id = paired.peer_identity.device_id();
    let peer_public = paired.peer_identity.public_identity();
    let owner_directory = DeviceDirectory::new(owner_id);
    owner_directory
        .replace_trusted(vec![peer_public])
        .expect("owner directory contains the paired peer");

    let (observed_tx, mut observed_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut peer_session = paired.peer_session;
    let peer_task = tokio::spawn(async move {
        let incoming = peer_session
            .receive_message()
            .await
            .expect("receive the sender's authenticated session hello");
        assert!(matches!(incoming.payload, MessagePayload::SessionHello(_)));
        peer_session
            .send_message(
                proto::MessageType::SessionHello,
                &proto::SessionHello {
                    session_version: 1,
                    device_id: peer_id.as_bytes().to_vec(),
                    reconnect_port: 0,
                    synthetic_key_transfer_version: 1,
                    sync_version: 0,
                    sync_rounds_version: 0,
                },
            )
            .await
            .expect("advertise the laboratory capability in the authenticated hello");

        while let Ok(message) = peer_session.receive_message().await {
            match message.payload {
                MessagePayload::SyntheticKeyOffer(_) => {
                    let _ = observed_tx.send(ObservedTransferFrame::Offer);
                }
                MessagePayload::SyntheticKeyChunk(_) => {
                    let _ = observed_tx.send(ObservedTransferFrame::Chunk);
                }
                MessagePayload::SyntheticKeyAbort(_) => {
                    let _ = observed_tx.send(ObservedTransferFrame::Abort);
                    peer_session
                        .send_message(
                            proto::MessageType::SyntheticKeyAbort,
                            &proto::SyntheticKeyAbort {
                                transfer_id: TRANSFER_ID.to_vec(),
                                code: 1,
                            },
                        )
                        .await
                        .expect("reply with an authenticated abort for the same terminal transfer");
                    const PROBE_NONCE: u64 = 1;
                    peer_session
                        .send_message(
                            proto::MessageType::SessionPing,
                            &proto::SessionPing { nonce: PROBE_NONCE },
                        )
                        .await
                        .expect("probe the owner after the peer's terminal abort");
                    loop {
                        match peer_session
                            .receive_message()
                            .await
                            .expect("owner should keep the authenticated session active")
                            .payload
                        {
                            MessagePayload::SessionPong(pong) if pong.nonce == PROBE_NONCE => {
                                let _ =
                                    observed_tx.send(ObservedTransferFrame::RemoteAbortRoundtrip);
                                break;
                            }
                            MessagePayload::SessionPing(ping) => {
                                peer_session
                                    .send_message(
                                        proto::MessageType::SessionPong,
                                        &proto::SessionPong { nonce: ping.nonce },
                                    )
                                    .await
                                    .expect(
                                        "answer an owner keepalive while waiting for the probe",
                                    );
                            }
                            MessagePayload::SessionClose(_) => {
                                panic!("terminal peer abort must not close the session")
                            }
                            _ => {}
                        }
                    }
                }
                MessagePayload::SessionPing(ping) => {
                    peer_session
                        .send_message(
                            proto::MessageType::SessionPong,
                            &proto::SessionPong { nonce: ping.nonce },
                        )
                        .await
                        .expect("answer the sender's keepalive");
                }
                MessagePayload::SessionClose(close) => {
                    peer_session
                        .send_message(proto::MessageType::SessionClose, &close)
                        .await
                        .expect("complete the sender's orderly close");
                    break;
                }
                _ => {}
            }
        }
    });

    let mut owner_handle = start_session_with_extensions(
        owner_directory.clone(),
        paired.owner_session,
        owner_id,
        peer_public,
        SessionOptions::default(),
        None,
        true,
    )
    .expect("start the explicitly enabled synthetic sender");
    wait_until_connected(&owner_handle).await;

    let control = owner_handle.synthetic_key_transfer_control();
    let generation = owner_handle.generation();
    let mut progress = control.subscribe_synthetic_key_transfer();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if progress.borrow_and_update().phase == SyntheticTransferPhase::Ready {
                return;
            }
            progress
                .changed()
                .await
                .expect("synthetic transfer progress remains observable");
        }
    })
    .await
    .expect("bilateral capability should make the scoped sender ready");

    control
        .start_synthetic_key_transfer(generation, prepared(TRANSFER_ID, 8))
        .expect("queue the explicitly approved operation");
    control
        .cancel_synthetic_key_transfer(generation, TRANSFER_ID)
        .expect("queue local cancellation even if Start is not consumed yet");

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if progress.borrow_and_update().phase == SyntheticTransferPhase::Cancelled {
                return;
            }
            progress
                .changed()
                .await
                .expect("synthetic transfer progress remains observable");
        }
    })
    .await
    .expect("the session owner should apply cancellation before sending chunks");

    let mut observed = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !observed.contains(&ObservedTransferFrame::RemoteAbortRoundtrip) {
            observed.push(
                observed_rx
                    .recv()
                    .await
                    .expect("the session owner should preserve the channel after remote abort"),
            );
        }
    })
    .await
    .expect("the raw authenticated peer should receive a ping response after its abort");
    assert!(observed.contains(&ObservedTransferFrame::Offer));
    assert!(observed.contains(&ObservedTransferFrame::Abort));
    assert!(observed.contains(&ObservedTransferFrame::RemoteAbortRoundtrip));
    assert!(!observed.contains(&ObservedTransferFrame::Chunk));
    assert_eq!(
        paired.owner_sent.count(kryptopass_protocol::Channel::Data),
        0
    );
    assert_eq!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    );

    owner_handle
        .disconnect(SessionCloseReason::UserRequested)
        .expect("close the owner session cleanly after the cancellation assertion");
    owner_handle.wait_closed().await;
    peer_task
        .await
        .expect("raw peer should finish after the session close exchange");
}

fn prepared(transfer_id: [u8; 16], byte_count: usize) -> SyntheticKeyTransferSend {
    // Opaque deterministic bytes exercise bounds and ordering; these are not a key fixture.
    let dummy_bytes = (0..byte_count)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    SyntheticKeyTransferSend::new(
        transfer_id,
        FINGERPRINT.to_vec(),
        dummy_bytes,
        vec![0x10, 0x20, 0x30],
        CHALLENGE_SHA256,
    )
    .expect("accept bounded opaque test bytes")
}

fn start_offer(
    sender: &mut SyntheticTransferSender,
    transfer_id: [u8; 16],
    byte_count: usize,
    now: Instant,
) {
    let outbound = sender
        .start(prepared(transfer_id, byte_count), now)
        .expect("explicit local start should create an offer");
    let SyntheticTransferOutbound::Offer(offer) = outbound else {
        panic!("starting should emit an offer");
    };
    assert_eq!(offer.transfer_id, transfer_id);
    assert_eq!(offer.openpgp_fingerprint, FINGERPRINT);
    assert_eq!(offer.total_size, byte_count as u64);
    assert_eq!(
        offer.sha256_digest,
        Sha256::digest(
            (0..byte_count)
                .map(|index| (index % 251) as u8)
                .collect::<Vec<_>>(),
        )
        .as_slice()
    );
}

fn decision(transfer_id: [u8; 16], accepted: bool) -> MessagePayload {
    MessagePayload::SyntheticKeyDecision(proto::SyntheticKeyDecision {
        transfer_id: transfer_id.to_vec(),
        accepted,
    })
}

fn accept(sender: &mut SyntheticTransferSender, transfer_id: [u8; 16], now: Instant) {
    sender
        .receive(decision(transfer_id, true), now)
        .expect("matching remote approval should authorize chunk output");
}

fn send_until_complete(
    sender: &mut SyntheticTransferSender,
    now: Instant,
) -> (Vec<u8>, proto::SyntheticKeyComplete) {
    let mut chunks = Vec::new();
    loop {
        match sender
            .next_outbound(now)
            .expect("next bounded frame should be available")
        {
            Some(SyntheticTransferOutbound::Chunk(chunk)) => {
                assert_eq!(chunk.transfer_id, TRANSFER_ID);
                assert_eq!(
                    chunk.chunk_index as usize,
                    chunks.len() / SYNTHETIC_KEY_CHUNK_BYTES
                );
                assert_eq!(chunk.total_size, (SYNTHETIC_KEY_CHUNK_BYTES + 3) as u64);
                assert_eq!(chunk.chunk_count, 2);
                assert_eq!(chunk.sha256_digest.len(), 32);
                chunks.extend_from_slice(&chunk.data);
            }
            Some(SyntheticTransferOutbound::Complete(complete)) => return (chunks, complete),
            Some(SyntheticTransferOutbound::Offer(_)) => panic!("offer is emitted only by start"),
            Some(SyntheticTransferOutbound::Abort(_)) => panic!("unexpected abort before timeout"),
            None => panic!("accepted transfer should produce chunks and completion"),
        }
    }
}

fn enter_waiting_receipt(now: Instant) -> (SyntheticTransferSender, Vec<u8>) {
    let byte_count = SYNTHETIC_KEY_CHUNK_BYTES + 3;
    let mut sender = SyntheticTransferSender::new(true);
    start_offer(&mut sender, TRANSFER_ID, byte_count, now);
    assert_eq!(
        sender.progress().phase,
        SyntheticTransferPhase::WaitingAcceptance
    );
    assert!(matches!(sender.next_outbound(now), Ok(None)));
    accept(&mut sender, TRANSFER_ID, now);
    let (chunks, complete) = send_until_complete(&mut sender, now);
    assert_eq!(complete.transfer_id, TRANSFER_ID);
    assert_eq!(
        sender.progress().phase,
        SyntheticTransferPhase::WaitingReceipt
    );
    (sender, chunks)
}

#[test]
fn bilateral_capability_and_explicit_local_start_are_required() {
    let now = Instant::now();
    let mut unsupported = SyntheticTransferSender::new(false);
    assert_eq!(
        unsupported.progress().phase,
        SyntheticTransferPhase::Unsupported
    );
    assert!(matches!(
        unsupported.start(prepared(TRANSFER_ID, 8), now),
        Err(SyntheticTransferError::Unsupported)
    ));
    assert_eq!(unsupported.progress().transfer_id, None);

    let mut supported = SyntheticTransferSender::new(true);
    assert_eq!(supported.progress().phase, SyntheticTransferPhase::Ready);
    assert!(matches!(supported.next_outbound(now), Ok(None)));
    start_offer(&mut supported, TRANSFER_ID, 8, now);
    assert_eq!(
        supported.progress().phase,
        SyntheticTransferPhase::WaitingAcceptance
    );
    assert!(matches!(supported.next_outbound(now), Ok(None)));
}

#[test]
fn only_remote_acceptance_releases_ordered_chunks_and_complete() {
    let now = Instant::now();
    let byte_count = SYNTHETIC_KEY_CHUNK_BYTES + 3;
    let mut sender = SyntheticTransferSender::new(true);
    start_offer(&mut sender, TRANSFER_ID, byte_count, now);

    assert!(matches!(sender.next_outbound(now), Ok(None)));
    accept(&mut sender, TRANSFER_ID, now);
    let (chunks, complete) = send_until_complete(&mut sender, now);

    let expected = (0..byte_count)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    assert_eq!(chunks, expected);
    assert_eq!(complete.transfer_id, TRANSFER_ID);
    assert_eq!(
        sender.progress().phase,
        SyntheticTransferPhase::WaitingReceipt
    );
    assert!(matches!(sender.next_outbound(now), Ok(None)));
}

#[test]
fn decline_is_terminal_and_a_transfer_id_cannot_be_replayed() {
    let now = Instant::now();
    let mut sender = SyntheticTransferSender::new(true);
    start_offer(&mut sender, TRANSFER_ID, 8, now);
    sender
        .receive(decision(TRANSFER_ID, false), now)
        .expect("decline should end the operation cleanly");
    assert_eq!(sender.progress().phase, SyntheticTransferPhase::Rejected);
    assert!(matches!(sender.next_outbound(now), Ok(None)));

    assert!(matches!(
        sender.start(prepared(TRANSFER_ID, 8), now),
        Err(SyntheticTransferError::Busy)
    ));
    let different_id = [0x36; 16];
    assert!(matches!(
        sender.start(prepared(different_id, 8), now),
        Err(SyntheticTransferError::Busy)
    ));
}

#[test]
fn cancellation_emits_abort_and_consumes_the_session_operation() {
    let now = Instant::now();
    let mut sender = SyntheticTransferSender::new(true);
    start_offer(&mut sender, TRANSFER_ID, 8, now);
    let outbound = sender
        .cancel(TRANSFER_ID, now)
        .expect("current transfer can be cancelled");
    let SyntheticTransferOutbound::Abort(abort) = outbound else {
        panic!("cancellation should emit an abort");
    };
    assert_eq!(abort.transfer_id, TRANSFER_ID);
    assert_eq!(abort.code, 1);
    assert_eq!(sender.progress().phase, SyntheticTransferPhase::Cancelled);
    assert!(matches!(
        sender.start(prepared(TRANSFER_ID, 8), now),
        Err(SyntheticTransferError::Busy)
    ));
    assert!(matches!(
        sender.cancel(TRANSFER_ID, now),
        Err(SyntheticTransferError::WrongSession)
    ));
}

#[test]
fn fixed_monotonic_timeout_expires_at_120_seconds_and_consumes_the_operation() {
    let now = Instant::now();
    let mut sender = SyntheticTransferSender::new(true);
    start_offer(&mut sender, TRANSFER_ID, 8, now);

    assert!(
        sender
            .expire(now + SYNTHETIC_KEY_TIMEOUT - Duration::from_nanos(1))
            .is_none()
    );
    let outbound = sender
        .expire(now + SYNTHETIC_KEY_TIMEOUT)
        .expect("operation should expire at its fixed deadline");
    let SyntheticTransferOutbound::Abort(abort) = outbound else {
        panic!("timeout should emit an abort");
    };
    assert_eq!(abort.transfer_id, TRANSFER_ID);
    assert_eq!(abort.code, 2);
    assert_eq!(sender.progress().phase, SyntheticTransferPhase::Expired);
    assert!(matches!(
        sender.start(prepared(TRANSFER_ID, 8), now + SYNTHETIC_KEY_TIMEOUT),
        Err(SyntheticTransferError::Busy)
    ));
}

#[test]
fn wrong_order_and_replayed_decisions_fail_the_operation() {
    let now = Instant::now();
    let mut sender = SyntheticTransferSender::new(true);
    start_offer(&mut sender, TRANSFER_ID, 8, now);

    let premature_receipt = MessagePayload::SyntheticKeyReceipt(proto::SyntheticKeyReceipt {
        transfer_id: TRANSFER_ID.to_vec(),
        openpgp_fingerprint: FINGERPRINT.to_vec(),
        challenge_sha256: CHALLENGE_SHA256.to_vec(),
    });
    assert!(matches!(
        sender.receive(premature_receipt, now),
        Err(SyntheticTransferError::Invalid)
    ));
    assert_eq!(sender.progress().phase, SyntheticTransferPhase::Failed);
    assert!(matches!(sender.next_outbound(now), Ok(None)));
    assert!(matches!(
        sender.start(prepared(TRANSFER_ID, 8), now),
        Err(SyntheticTransferError::Busy)
    ));

    let mut replayed = SyntheticTransferSender::new(true);
    start_offer(&mut replayed, TRANSFER_ID, 8, now);
    accept(&mut replayed, TRANSFER_ID, now);
    assert!(matches!(
        replayed.receive(decision(TRANSFER_ID, true), now),
        Err(SyntheticTransferError::Invalid)
    ));
    assert_eq!(replayed.progress().phase, SyntheticTransferPhase::Failed);
    assert!(matches!(replayed.next_outbound(now), Ok(None)));
}

#[test]
fn mismatched_receipt_fingerprint_or_challenge_never_marks_complete() {
    let now = Instant::now();
    let (mut fingerprint_mismatch, _) = enter_waiting_receipt(now);
    let mut other_fingerprint = FINGERPRINT;
    other_fingerprint[0] ^= 1;
    assert!(matches!(
        fingerprint_mismatch.receive(
            MessagePayload::SyntheticKeyReceipt(proto::SyntheticKeyReceipt {
                transfer_id: TRANSFER_ID.to_vec(),
                openpgp_fingerprint: other_fingerprint.to_vec(),
                challenge_sha256: CHALLENGE_SHA256.to_vec(),
            }),
            now,
        ),
        Err(SyntheticTransferError::Invalid)
    ));
    assert_eq!(
        fingerprint_mismatch.progress().phase,
        SyntheticTransferPhase::Uncertain
    );

    let (mut challenge_mismatch, _) = enter_waiting_receipt(now);
    let mut other_challenge = CHALLENGE_SHA256;
    other_challenge[0] ^= 1;
    assert!(matches!(
        challenge_mismatch.receive(
            MessagePayload::SyntheticKeyReceipt(proto::SyntheticKeyReceipt {
                transfer_id: TRANSFER_ID.to_vec(),
                openpgp_fingerprint: FINGERPRINT.to_vec(),
                challenge_sha256: other_challenge.to_vec(),
            }),
            now,
        ),
        Err(SyntheticTransferError::Invalid)
    ));
    assert_eq!(
        challenge_mismatch.progress().phase,
        SyntheticTransferPhase::Uncertain
    );
}

#[test]
fn matching_receipt_completes_the_only_operation_in_the_session() {
    let now = Instant::now();
    let (mut sender, _) = enter_waiting_receipt(now);
    sender
        .receive(
            MessagePayload::SyntheticKeyReceipt(proto::SyntheticKeyReceipt {
                transfer_id: TRANSFER_ID.to_vec(),
                openpgp_fingerprint: FINGERPRINT.to_vec(),
                challenge_sha256: CHALLENGE_SHA256.to_vec(),
            }),
            now,
        )
        .expect("matching receipt should complete the authenticated exercise");
    assert_eq!(sender.progress().phase, SyntheticTransferPhase::Complete);
    assert!(matches!(
        sender.start(prepared([0x36; 16], 8), now),
        Err(SyntheticTransferError::Busy)
    ));
}

#[test]
fn connection_loss_after_complete_reports_uncertain() {
    let now = Instant::now();
    let (mut sender, _) = enter_waiting_receipt(now);

    sender.disconnected();

    assert_eq!(sender.progress().phase, SyntheticTransferPhase::Uncertain);
    assert!(matches!(sender.next_outbound(now), Ok(None)));
    assert!(matches!(
        sender.start(prepared(TRANSFER_ID, 8), now),
        Err(SyntheticTransferError::Busy)
    ));
}

#[test]
fn post_complete_cancel_expiry_and_remote_abort_preserve_uncertain_state() {
    let now = Instant::now();

    let (mut locally_cancelled, _) = enter_waiting_receipt(now);
    let abort = locally_cancelled
        .cancel(TRANSFER_ID, now + Duration::from_secs(1))
        .expect("the owner may still stop waiting for a receipt");
    assert!(matches!(abort, SyntheticTransferOutbound::Abort(_)));
    assert_eq!(
        locally_cancelled.progress().phase,
        SyntheticTransferPhase::Uncertain,
        "the receiver may already have persisted the complete object"
    );

    let (mut expired, _) = enter_waiting_receipt(now);
    assert!(matches!(
        expired.expire(now + SYNTHETIC_KEY_TIMEOUT),
        Some(SyntheticTransferOutbound::Abort(_))
    ));
    assert_eq!(
        expired.progress().phase,
        SyntheticTransferPhase::Uncertain,
        "expiry after Complete cannot establish that the receiver discarded the object"
    );

    let (mut remotely_aborted, _) = enter_waiting_receipt(now);
    remotely_aborted
        .receive(
            MessagePayload::SyntheticKeyAbort(proto::SyntheticKeyAbort {
                transfer_id: TRANSFER_ID.to_vec(),
                code: 1,
            }),
            now + Duration::from_secs(1),
        )
        .expect("accept the peer's authenticated abort");
    assert_eq!(
        remotely_aborted.progress().phase,
        SyntheticTransferPhase::Uncertain,
        "a peer abort after Complete does not prove that its local import rolled back"
    );
}
