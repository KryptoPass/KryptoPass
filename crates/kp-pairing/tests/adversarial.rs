#![allow(
    clippy::expect_used,
    reason = "Adversarial integration tests use expect only for isolated setup and local assertions."
)]

use async_trait::async_trait;
use kryptopass_identity::{
    DeviceIdentity, InvitationTokenHash, PublicIdentity, SqliteTrustStore, TrustStoreError,
};
use kryptopass_pairing::{
    CanonicalContext, LocalApproval, LocalDecision, PairingConfig, PairingEngine, PairingError,
    PairingInvitation, PairingMode, PairingRole, SasDisplay, build_prologue,
    identity_proof_message,
};
use kryptopass_protocol::{Channel, SessionCodec, proto};
use kryptopass_transport::{
    MemoryTransport, MemoryTransportConfig, PeerTransport, TransportError, TransportFrame,
};
use rusqlite::{Connection, OptionalExtension};
use snow::{Builder, HandshakeState, TransportState, params::NoiseParams};
use std::{
    fs,
    future::pending,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Barrier;

static NEXT_PROFILE_ID: AtomicU64 = AtomicU64::new(1);

struct TestProfile(PathBuf);

impl TestProfile {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let id = NEXT_PROFILE_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kryptopass-pairing-adversarial-{}-{stamp}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated test profile");
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .expect("restrict test profile to its owner");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

impl Drop for TestProfile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone)]
struct Decision(LocalDecision);

#[async_trait]
impl LocalApproval for Decision {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        self.0
    }
}

struct ObservedApproval {
    decision: Option<LocalDecision>,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl LocalApproval for ObservedApproval {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if let Some(decision) = self.decision {
            decision
        } else {
            pending().await
        }
    }
}

#[derive(Default)]
struct NoApproval {
    calls: Arc<Mutex<usize>>,
}

fn no_approval_call_count(approval: &NoApproval) -> usize {
    *approval.calls.lock().expect("approval call counter lock")
}

#[async_trait]
impl LocalApproval for NoApproval {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        *self.calls.lock().expect("approval call counter lock") += 1;
        LocalDecision::Cancel
    }
}

struct WaitForBothApprovals(Arc<Barrier>);

#[async_trait]
impl LocalApproval for WaitForBothApprovals {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        self.0.wait().await;
        pending().await
    }
}

fn open_new(profile: &TestProfile, identity: &DeviceIdentity) -> SqliteTrustStore {
    SqliteTrustStore::open_new(profile.path(), identity.device_id())
        .expect("open isolated trust store")
}

fn open_existing(profile: &TestProfile, identity: &DeviceIdentity) -> SqliteTrustStore {
    SqliteTrustStore::open_existing(profile.path(), identity.device_id())
        .expect("reopen isolated trust store")
}

fn config(session_id: [u8; 16], role: PairingRole, now_ms: i64, timeout_ms: u64) -> PairingConfig {
    let mut config = PairingConfig::pairing(session_id, role, now_ms);
    config.timeout = Duration::from_millis(timeout_ms);
    config
}

fn assert_not_trusted(profile: &TestProfile, local: &DeviceIdentity, peer: &DeviceIdentity) {
    let store = open_existing(profile, local);
    assert!(matches!(
        store.lookup_trusted_peer(&peer.device_id()),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
}

fn no_auto_trust_trigger(profile: &TestProfile) {
    let connection = Connection::open(profile.path().join("device-trust.sqlite3"))
        .expect("open test database to inject a write failure");
    connection
        .execute_batch(
            "CREATE TRIGGER fail_pairing_authorization
             BEFORE INSERT ON trusted_peers
             BEGIN SELECT RAISE(ABORT, 'test injected authorization failure'); END;",
        )
        .expect("install deterministic authorization failure");
}

fn remove_no_auto_trust_trigger(profile: &TestProfile) {
    let connection = Connection::open(profile.path().join("device-trust.sqlite3"))
        .expect("open test database to remove injected failure");
    connection
        .execute_batch("DROP TRIGGER IF EXISTS fail_pairing_authorization;")
        .expect("remove deterministic authorization failure");
}

fn invitation_consumed_at(profile: &TestProfile, invite_id: &[u8; 16]) -> Option<Option<i64>> {
    let connection = Connection::open(profile.path().join("device-trust.sqlite3"))
        .expect("open invitation ledger for read-only assertion");
    let consumed_at_ms: Option<Option<i64>> = connection
        .query_row(
            "SELECT consumed_at_ms FROM invitation_ledger WHERE invite_id = ?1",
            [invite_id.as_slice()],
            |row| row.get(0),
        )
        .optional()
        .expect("read invitation consumed state");
    consumed_at_ms
}

async fn send_noise_frame(
    transport: &mut MemoryTransport,
    codec: &mut SessionCodec,
    step: u32,
    handshake_message: Vec<u8>,
) {
    let frame = codec
        .encode(
            proto::MessageType::NoiseHandshake,
            &proto::NoiseHandshake {
                step,
                handshake_message,
            },
        )
        .expect("encode fake Noise handshake frame");
    transport
        .send(Channel::Control, &frame)
        .await
        .expect("send fake Noise handshake frame");
}

async fn receive_noise_frame(
    transport: &mut MemoryTransport,
    codec: &mut SessionCodec,
) -> proto::NoiseHandshake {
    let frame = transport
        .receive()
        .await
        .expect("receive Noise handshake frame");
    let decoded = codec
        .decode_on(frame.channel, &frame.data)
        .expect("decode Noise handshake frame");
    match decoded.payload {
        kryptopass_protocol::MessagePayload::NoiseHandshake(noise) => noise,
        _ => panic!("expected Noise handshake payload"),
    }
}

async fn fake_initiator_with_mismatched_ed25519_proof(
    mut transport: MemoryTransport,
    identity: &DeviceIdentity,
    claimed_signer: &DeviceIdentity,
    invitation: &PairingInvitation,
    session_id: [u8; 16],
    wrong_transcript_hash: bool,
) {
    let mut codec = SessionCodec::new(session_id);
    let hello = codec
        .encode(
            proto::MessageType::PairingHello,
            &proto::PairingHello {
                invite_id: invitation.invite_id().to_vec(),
            },
        )
        .expect("encode invitation hello");
    transport
        .send(Channel::Control, &hello)
        .await
        .expect("send invitation hello");

    let prologue = build_prologue(PairingMode::Pairing, session_id, Some(invitation))
        .expect("build valid invitation prologue");
    let params: NoiseParams = kryptopass_pairing::NOISE_SUITE
        .parse()
        .expect("parse supported Noise suite");
    let builder = Builder::new(params)
        .local_private_key(identity.static_secret())
        .expect("use attacker static key")
        .prologue(prologue.as_bytes())
        .expect("bind attacker handshake to invitation");
    let mut handshake: HandshakeState = builder.build_initiator().expect("build fake initiator");

    let mut message = vec![0_u8; 1024];
    let first_len = handshake
        .write_message(&[], &mut message)
        .expect("write valid Noise message one");
    send_noise_frame(&mut transport, &mut codec, 1, message[..first_len].to_vec()).await;

    let response = receive_noise_frame(&mut transport, &mut codec).await;
    assert_eq!(response.step, 2);
    let mut payload = vec![0_u8; 1024];
    let response_len = handshake
        .read_message(&response.handshake_message, &mut payload)
        .expect("read valid Noise message two");
    assert_eq!(response_len, 32);
    let responder_ed: [u8; 32] = payload[..response_len]
        .try_into()
        .expect("responder signing key has fixed width");

    let claimed_ed = claimed_signer.signing_public_key();
    let third_len = handshake
        .write_message(&claimed_ed, &mut message)
        .expect("write Noise message three claiming another signer");
    send_noise_frame(&mut transport, &mut codec, 3, message[..third_len].to_vec()).await;

    let noise_hash: [u8; 32] = handshake
        .get_handshake_hash()
        .try_into()
        .expect("Noise transcript hash has fixed width");
    let responder_static: [u8; 32] = handshake
        .get_remote_static()
        .expect("Noise authenticates the remote static key")
        .try_into()
        .expect("remote static key has fixed width");
    let mut noise: TransportState = handshake
        .into_transport_mode()
        .expect("complete Noise XX handshake");

    // Consume the responder's own identity frame to keep the exact codec and
    // Noise directions in sync before injecting the bad initiator proof.
    let responder_frame = transport
        .receive()
        .await
        .expect("receive responder's encrypted identity");
    let mut plaintext = vec![0_u8; 4096];
    let plaintext_len = noise
        .read_message(&responder_frame.data, &mut plaintext)
        .expect("decrypt responder identity");
    let responder_identity = codec
        .decode_on(Channel::Control, &plaintext[..plaintext_len])
        .expect("decode responder identity");
    assert!(matches!(
        responder_identity.payload,
        kryptopass_protocol::MessagePayload::PairingIdentity(_)
    ));

    let qr = invitation
        .to_qr_bytes()
        .expect("encode invitation for canonical proof context");
    let invite_wire = kryptopass_protocol::decode_pairing_invite(qr.as_bytes())
        .expect("decode invitation token for test proof");
    let raw_token: [u8; 32] = invite_wire
        .single_use_token
        .as_slice()
        .try_into()
        .expect("invitation token has fixed width");
    let token_hash = *InvitationTokenHash::from_token(&raw_token).as_bytes();
    let claimed_identity = PublicIdentity::new(claimed_ed, identity.static_public_key())
        .expect("construct claimed public identity");
    let context = CanonicalContext::new(
        PairingMode::Pairing,
        session_id,
        *invitation.invite_id(),
        token_hash,
        invitation.expires_at_ms() as u64,
        noise_hash,
        identity.static_public_key(),
        responder_static,
        claimed_ed,
        responder_ed,
    )
    .expect("construct valid context for the substituted signing key");
    let proof_message = identity_proof_message(PairingRole::Initiator, &context);
    let wrong_proof = identity.sign(&proof_message);
    let identity_frame = codec
        .encode(
            proto::MessageType::PairingIdentity,
            &proto::PairingIdentity {
                device_id: claimed_identity.device_id.as_bytes().to_vec(),
                signing_public_key: claimed_ed.to_vec(),
                transcript_hash: {
                    let mut transmitted = noise_hash;
                    if wrong_transcript_hash {
                        transmitted[0] ^= 1;
                    }
                    transmitted.to_vec()
                },
                proof: wrong_proof.to_vec(),
            },
        )
        .expect("encode identity with proof from a different signing key");
    let mut ciphertext = vec![0_u8; identity_frame.len() + 64];
    let ciphertext_len = noise
        .write_message(&identity_frame, &mut ciphertext)
        .expect("encrypt substituted proof");
    transport
        .send(Channel::Control, &ciphertext[..ciphertext_len])
        .await
        .expect("send substituted proof");
    // Keep the link alive until the responder consumes the frame and rejects
    // its proof; dropping immediately could close the in-memory queue first.
    let _ = transport.receive().await;
}

#[derive(Clone, Copy)]
enum SendFault {
    FlipByte,
    Close,
}

struct FaultTransport {
    inner: MemoryTransport,
    fault_on_send: usize,
    sends: usize,
    fault: SendFault,
}

impl FaultTransport {
    fn new(inner: MemoryTransport, fault_on_send: usize, fault: SendFault) -> Self {
        Self {
            inner,
            fault_on_send,
            sends: 0,
            fault,
        }
    }
}

#[async_trait]
impl PeerTransport for FaultTransport {
    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        self.sends += 1;
        if self.sends != self.fault_on_send {
            return self.inner.send(channel, data).await;
        }
        match self.fault {
            SendFault::FlipByte => {
                let mut changed = data.to_vec();
                let last = changed.last_mut().ok_or(TransportError::Closed)?;
                *last ^= 0x01;
                self.inner.send(channel, &changed).await
            }
            SendFault::Close => {
                let _ = self.inner.close().await;
                Err(TransportError::Closed)
            }
        }
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        self.inner.receive().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

struct CaptureSendTransport {
    inner: MemoryTransport,
    sends: usize,
    capture_on_send: usize,
    capture: Arc<Mutex<Option<Vec<u8>>>>,
}

struct ObserveReceiveTransport {
    inner: MemoryTransport,
    receives: Arc<AtomicUsize>,
}

#[async_trait]
impl PeerTransport for ObserveReceiveTransport {
    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        self.inner.send(channel, data).await
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        let frame = self.inner.receive().await?;
        self.receives.fetch_add(1, Ordering::Relaxed);
        Ok(frame)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

#[async_trait]
impl PeerTransport for CaptureSendTransport {
    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        self.sends += 1;
        if self.sends == self.capture_on_send {
            *self.capture.lock().expect("capture lock") = Some(data.to_vec());
        }
        self.inner.send(channel, data).await
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        self.inner.receive().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

struct ReplayReceiveTransport {
    inner: MemoryTransport,
    receives: usize,
    replace_on_receive: usize,
    replay: Arc<Mutex<Option<Vec<u8>>>>,
}

#[async_trait]
impl PeerTransport for ReplayReceiveTransport {
    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        self.inner.send(channel, data).await
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        let fresh = self.inner.receive().await?;
        self.receives += 1;
        if self.receives == self.replace_on_receive {
            let data = self
                .replay
                .lock()
                .expect("replay lock")
                .clone()
                .expect("previous authenticated confirmation was captured");
            return Ok(TransportFrame {
                channel: Channel::Control,
                data,
            });
        }
        Ok(fresh)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

struct RevokeOnSendTransport {
    inner: MemoryTransport,
    sends: usize,
    revoke_on_send: usize,
    store: SqliteTrustStore,
    peer_id: kryptopass_identity::DeviceId,
    revoked_at_ms: i64,
}

#[async_trait]
impl PeerTransport for RevokeOnSendTransport {
    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        self.sends += 1;
        if self.sends == self.revoke_on_send {
            self.store
                .revoke_peer(&self.peer_id, self.revoked_at_ms)
                .expect("inject durable peer revocation during handshake");
        }
        self.inner.send(channel, data).await
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        self.inner.receive().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

#[tokio::test]
async fn remote_confirmation_never_substitutes_for_local_sas_approval() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let mut store_r = open_new(&profile_r, &identity_r);
    let invitation_r = PairingInvitation::create(&mut store_r, 50_000, Duration::from_secs(60))
        .expect("create invitation");
    let qr = invitation_r.to_qr_bytes().expect("encode invitation");
    let invitation_i = PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
    let captured_confirmation = Arc::new(Mutex::new(None));
    let transport_i = CaptureSendTransport {
        inner: transport_i,
        sends: 0,
        capture_on_send: 5,
        capture: Arc::clone(&captured_confirmation),
    };
    let responder_receives = Arc::new(AtomicUsize::new(0));
    let transport_r = ObserveReceiveTransport {
        inner: transport_r,
        receives: Arc::clone(&responder_receives),
    };

    // The initiator explicitly accepts and sends accepted=true. The responder
    // receives it but its local decision remains pending until the deadline.
    let pending_callback_calls = Arc::new(AtomicUsize::new(0));
    let initiator_callback_calls = Arc::new(AtomicUsize::new(0));
    let mut hold_local = ObservedApproval {
        decision: None,
        calls: Arc::clone(&pending_callback_calls),
    };
    let mut approve_remote = ObservedApproval {
        decision: Some(LocalDecision::Approve),
        calls: Arc::clone(&initiator_callback_calls),
    };
    let (result_i, result_r) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &identity_i,
            open_new(&profile_i, &identity_i),
            config([0x71; 16], PairingRole::Initiator, 50_000, 1_000),
        )
        .expect("construct initiator")
        .run(Some(&invitation_i), &mut approve_remote),
        PairingEngine::new(
            transport_r,
            &identity_r,
            store_r,
            config([0x71; 16], PairingRole::Responder, 50_000, 180),
        )
        .expect("construct responder")
        .run(Some(&invitation_r), &mut hold_local),
    );

    assert!(result_i.is_err(), "initiator attempt must expire");
    assert!(matches!(result_r.as_ref(), Err(PairingError::Expired)));
    assert_eq!(initiator_callback_calls.load(Ordering::Relaxed), 1);
    assert_eq!(pending_callback_calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        responder_receives.load(Ordering::Relaxed),
        5,
        "responder received hello, two Noise frames, identity, then confirmation"
    );
    assert!(
        captured_confirmation
            .lock()
            .expect("confirmation capture lock")
            .as_ref()
            .is_some_and(|frame| !frame.is_empty())
    );
    assert_not_trusted(&profile_i, &identity_i, &identity_r);
    assert_not_trusted(&profile_r, &identity_r, &identity_i);
}

#[tokio::test]
async fn dropping_both_attempts_while_sas_is_pending_leaves_no_trust_after_reopen() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let mut store_r = open_new(&profile_r, &identity_r);
    let invitation_r = PairingInvitation::create(&mut store_r, 55_000, Duration::from_secs(60))
        .expect("create invitation");
    let qr = invitation_r.to_qr_bytes().expect("encode invitation");
    let invitation_i = PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
    let barrier = Arc::new(Barrier::new(3));
    let mut approval_i = WaitForBothApprovals(Arc::clone(&barrier));
    let mut approval_r = WaitForBothApprovals(Arc::clone(&barrier));
    let session_id = [0x81; 16];
    let initiator = PairingEngine::new(
        transport_i,
        &identity_i,
        open_new(&profile_i, &identity_i),
        config(session_id, PairingRole::Initiator, 55_000, 5_000),
    )
    .expect("construct initiator");
    let responder = PairingEngine::new(
        transport_r,
        &identity_r,
        store_r,
        config(session_id, PairingRole::Responder, 55_000, 5_000),
    )
    .expect("construct responder");

    {
        let run = async {
            tokio::join!(
                initiator.run(Some(&invitation_i), &mut approval_i),
                responder.run(Some(&invitation_r), &mut approval_r),
            )
        };
        tokio::pin!(run);
        tokio::select! {
            _ = barrier.wait() => {},
            _ = tokio::time::sleep(Duration::from_secs(3)) => {
                panic!("both peers did not reach the pending SAS approval callback");
            },
            _ = &mut run => {
                panic!("pairing unexpectedly completed while both approvals were pending");
            },
        }
    }

    // Leaving the scope drops both in-flight runs, like a process shutdown.
    // Reopening each SQLite store checks durable trust after cancellation.
    assert_not_trusted(&profile_i, &identity_i, &identity_r);
    assert_not_trusted(&profile_r, &identity_r, &identity_i);
}

#[tokio::test]
async fn local_rejection_or_cancellation_never_creates_trust() {
    for (case, initiator_decision, responder_decision) in [
        (0_u8, LocalDecision::Reject, LocalDecision::Approve),
        (1_u8, LocalDecision::Approve, LocalDecision::Reject),
        (2_u8, LocalDecision::Cancel, LocalDecision::Approve),
        (3_u8, LocalDecision::Approve, LocalDecision::Cancel),
    ] {
        let profile_i = TestProfile::new();
        let profile_r = TestProfile::new();
        let identity_i = DeviceIdentity::generate().expect("generate initiator");
        let identity_r = DeviceIdentity::generate().expect("generate responder");
        let mut store_r = open_new(&profile_r, &identity_r);
        let invitation_r = PairingInvitation::create(&mut store_r, 60_000, Duration::from_secs(60))
            .expect("create invitation");
        let qr = invitation_r.to_qr_bytes().expect("encode invitation");
        let invitation_i = PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
        let (transport_i, transport_r) =
            MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
        let session = [0x72 + case; 16];
        let mut approval_i = Decision(initiator_decision);
        let mut approval_r = Decision(responder_decision);

        let (result_i, result_r) = tokio::join!(
            PairingEngine::new(
                transport_i,
                &identity_i,
                open_new(&profile_i, &identity_i),
                config(session, PairingRole::Initiator, 60_000, 1_000),
            )
            .expect("construct initiator")
            .run(Some(&invitation_i), &mut approval_i),
            PairingEngine::new(
                transport_r,
                &identity_r,
                store_r,
                config(session, PairingRole::Responder, 60_000, 1_000),
            )
            .expect("construct responder")
            .run(Some(&invitation_r), &mut approval_r),
        );
        assert!(result_i.is_err(), "case {case}: initiator must not trust");
        assert!(result_r.is_err(), "case {case}: responder must not trust");
        match case {
            0 => assert!(matches!(
                result_i.as_ref(),
                Err(PairingError::LocalRejected)
            )),
            1 => assert!(matches!(
                result_r.as_ref(),
                Err(PairingError::LocalRejected)
            )),
            2 => assert!(matches!(result_i.as_ref(), Err(PairingError::Cancelled))),
            3 => assert!(matches!(result_r.as_ref(), Err(PairingError::Cancelled))),
            _ => unreachable!("the test enumerates only four local decisions"),
        }
        assert_not_trusted(&profile_i, &identity_i, &identity_r);
        assert_not_trusted(&profile_r, &identity_r, &identity_i);
    }
}

#[tokio::test]
async fn expired_invitation_is_rejected_before_any_trust_write() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let mut store_r = open_new(&profile_r, &identity_r);
    let invitation_r = PairingInvitation::create(&mut store_r, 70_000, Duration::from_secs(1))
        .expect("create short invitation");
    let qr = invitation_r.to_qr_bytes().expect("encode invitation");
    let invitation_i = PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
    let mut approval_i = Decision(LocalDecision::Approve);
    let mut approval_r = Decision(LocalDecision::Approve);
    let now = invitation_r.expires_at_ms();

    let (result_i, result_r) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &identity_i,
            open_new(&profile_i, &identity_i),
            config([0x73; 16], PairingRole::Initiator, now, 1_000),
        )
        .expect("construct initiator")
        .run(Some(&invitation_i), &mut approval_i),
        PairingEngine::new(
            transport_r,
            &identity_r,
            store_r,
            config([0x73; 16], PairingRole::Responder, now, 1_000),
        )
        .expect("construct responder")
        .run(Some(&invitation_r), &mut approval_r),
    );

    assert!(matches!(result_i.as_ref(), Err(PairingError::Expired)));
    assert!(matches!(result_r.as_ref(), Err(PairingError::Expired)));
    assert_not_trusted(&profile_i, &identity_i, &identity_r);
    assert_not_trusted(&profile_r, &identity_r, &identity_i);
}

#[tokio::test]
async fn invitation_expiring_after_engine_construction_is_not_consumed() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let mut store_r = open_new(&profile_r, &identity_r);
    let invitation_r = PairingInvitation::create(&mut store_r, 130_000, Duration::from_millis(50))
        .expect("create short-lived invitation");
    let invite_id = *invitation_r.invite_id();
    let qr = invitation_r.to_qr_bytes().expect("encode invitation");
    let invitation_i = PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
    let config_i = config([0x7a; 16], PairingRole::Initiator, 130_000, 1_000);
    let config_r = config([0x7a; 16], PairingRole::Responder, 130_000, 1_000);
    let initiator = PairingEngine::new(
        transport_i,
        &identity_i,
        open_new(&profile_i, &identity_i),
        config_i,
    )
    .expect("construct initiator before expiry");
    let responder = PairingEngine::new(transport_r, &identity_r, store_r, config_r)
        .expect("construct responder before expiry");
    let mut approval_i = Decision(LocalDecision::Approve);
    let mut approval_r = Decision(LocalDecision::Approve);

    tokio::time::sleep(Duration::from_millis(80)).await;
    let (result_i, result_r) = tokio::join!(
        initiator.run(Some(&invitation_i), &mut approval_i),
        responder.run(Some(&invitation_r), &mut approval_r),
    );

    assert!(matches!(result_i.as_ref(), Err(PairingError::Expired)));
    assert!(matches!(result_r.as_ref(), Err(PairingError::Expired)));
    assert_eq!(invitation_consumed_at(&profile_r, &invite_id), Some(None));
    assert_not_trusted(&profile_i, &identity_i, &identity_r);
    assert_not_trusted(&profile_r, &identity_r, &identity_i);
}

#[test]
fn reconnect_rejects_a_same_signer_with_a_replaced_static_key_and_keeps_trust() {
    let profile = TestProfile::new();
    let local_identity = DeviceIdentity::generate().expect("generate local identity");
    let trusted_peer = DeviceIdentity::generate().expect("generate trusted peer");
    let other_static = DeviceIdentity::generate().expect("generate replacement static key");
    let mut store = open_new(&profile, &local_identity);
    store
        .authorize_peer(&trusted_peer.public_identity(), 200_000)
        .expect("seed existing trust entry");
    let substituted = PublicIdentity::new(
        trusted_peer.signing_public_key(),
        other_static.static_public_key(),
    )
    .expect("same signing key remains a valid device ID with a different static key");
    assert_eq!(substituted.device_id, trusted_peer.device_id());
    let (transport, _untrusted_peer) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("create reconnect link");
    let config = PairingConfig::reconnect([0x7f; 16], PairingRole::Initiator, 200_000, substituted);

    let result = PairingEngine::new(transport, &local_identity, store, config);
    assert!(matches!(
        result.as_ref(),
        Err(PairingError::InvalidPeerIdentity)
    ));
    assert_eq!(
        open_existing(&profile, &local_identity)
            .lookup_trusted_peer(&trusted_peer.device_id())
            .expect("original peer remains trusted"),
        trusted_peer.public_identity()
    );
}

#[tokio::test]
async fn reconnect_rejects_a_different_peer_received_over_noise_without_replacing_trust() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let previously_trusted_r = DeviceIdentity::generate().expect("generate previous responder");
    let substituted_r = DeviceIdentity::generate().expect("generate substituted responder");

    let mut store_i = open_new(&profile_i, &identity_i);
    store_i
        .authorize_peer(&previously_trusted_r.public_identity(), 201_000)
        .expect("record the expected reconnect peer");
    let mut store_r = open_new(&profile_r, &substituted_r);
    store_r
        .authorize_peer(&identity_i.public_identity(), 201_000)
        .expect("record the initiator on the substituted responder");

    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect reconnect peers");
    let mut approval_i = NoApproval::default();
    let mut approval_r = NoApproval::default();
    let session_id = [0x82; 16];
    let mut config_i = PairingConfig::reconnect(
        session_id,
        PairingRole::Initiator,
        201_000,
        previously_trusted_r.public_identity(),
    );
    let mut config_r = PairingConfig::reconnect(
        session_id,
        PairingRole::Responder,
        201_000,
        identity_i.public_identity(),
    );
    config_i.timeout = Duration::from_secs(2);
    config_r.timeout = Duration::from_secs(2);

    let (result_i, result_r) = tokio::join!(
        PairingEngine::new(transport_i, &identity_i, store_i, config_i)
            .expect("construct initiator with original expected peer")
            .run(None, &mut approval_i),
        PairingEngine::new(transport_r, &substituted_r, store_r, config_r)
            .expect("construct responder expecting the unchanged initiator")
            .run(None, &mut approval_r),
    );

    assert!(matches!(
        result_i.as_ref(),
        Err(PairingError::InvalidPeerIdentity)
    ));
    assert!(result_r.is_err());
    assert_eq!(no_approval_call_count(&approval_i), 0);
    assert_eq!(no_approval_call_count(&approval_r), 0);
    assert_eq!(
        open_existing(&profile_i, &identity_i)
            .lookup_trusted_peer(&previously_trusted_r.device_id())
            .expect("previous peer remains trusted"),
        previously_trusted_r.public_identity()
    );
    assert!(matches!(
        open_existing(&profile_i, &identity_i).lookup_trusted_peer(&substituted_r.device_id()),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
    assert_eq!(
        open_existing(&profile_r, &substituted_r)
            .lookup_trusted_peer(&identity_i.device_id())
            .expect("responder's existing peer remains trusted"),
        identity_i.public_identity()
    );
}

#[tokio::test]
async fn reconnect_rechecks_revocation_before_return_and_never_repairs_trust() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let mut store_i = open_new(&profile_i, &identity_i);
    store_i
        .authorize_peer(&identity_r.public_identity(), 202_000)
        .expect("record initiator's trusted responder");
    let revoker = open_existing(&profile_i, &identity_i);
    let mut store_r = open_new(&profile_r, &identity_r);
    store_r
        .authorize_peer(&identity_i.public_identity(), 202_000)
        .expect("record responder's trusted initiator");

    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect reconnect peers");
    let transport_i = RevokeOnSendTransport {
        inner: transport_i,
        sends: 0,
        revoke_on_send: 3, // Initiator's Noise XX message three.
        store: revoker,
        peer_id: identity_r.device_id(),
        revoked_at_ms: 202_001,
    };
    let mut approval_i = NoApproval::default();
    let mut approval_r = NoApproval::default();
    let session_id = [0x83; 16];
    let mut config_i = PairingConfig::reconnect(
        session_id,
        PairingRole::Initiator,
        202_000,
        identity_r.public_identity(),
    );
    let mut config_r = PairingConfig::reconnect(
        session_id,
        PairingRole::Responder,
        202_000,
        identity_i.public_identity(),
    );
    config_i.timeout = Duration::from_secs(2);
    config_r.timeout = Duration::from_secs(2);

    let (result_i, result_r) = tokio::join!(
        PairingEngine::new(transport_i, &identity_i, store_i, config_i)
            .expect("construct initiator before revocation")
            .run(None, &mut approval_i),
        PairingEngine::new(transport_r, &identity_r, store_r, config_r)
            .expect("construct responder")
            .run(None, &mut approval_r),
    );

    assert!(matches!(
        result_i.as_ref(),
        Err(PairingError::TrustStore(
            TrustStoreError::UnknownOrRevokedPeer
        ))
    ));
    if let Ok(outcome) = result_r.as_ref() {
        assert_eq!(outcome.state, kryptopass_pairing::PairingState::Trusted);
        assert_eq!(outcome.peer, identity_i.public_identity());
    }
    assert_eq!(no_approval_call_count(&approval_i), 0);
    assert_eq!(no_approval_call_count(&approval_r), 0);
    assert!(matches!(
        open_existing(&profile_i, &identity_i).lookup_trusted_peer(&identity_r.device_id()),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
    assert_eq!(
        open_existing(&profile_r, &identity_r)
            .lookup_trusted_peer(&identity_i.device_id())
            .expect("unrevoked responder record remains trusted"),
        identity_i.public_identity()
    );
}

#[tokio::test]
async fn failed_attempt_consumes_invitation_and_cannot_be_replayed() {
    let profile = TestProfile::new();
    let identity = DeviceIdentity::generate().expect("generate responder");
    let mut store = open_new(&profile, &identity);
    let invitation = PairingInvitation::create(&mut store, 80_000, Duration::from_secs(60))
        .expect("create one-time invitation");
    let peer_identity = DeviceIdentity::generate().expect("generate fake initiator");
    let session_id = [0x74; 16];
    let (mut fake_peer, responder_transport) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect fake peer");
    let mut responder_approval = Decision(LocalDecision::Approve);
    let mut codec = SessionCodec::new(session_id);
    let wrong_hello = codec
        .encode(
            proto::MessageType::PairingHello,
            &proto::PairingHello {
                invite_id: vec![0x99; 16],
            },
        )
        .expect("encode valid but wrong invite hello");
    let engine = PairingEngine::new(
        responder_transport,
        &identity,
        store,
        config(session_id, PairingRole::Responder, 80_000, 1_000),
    )
    .expect("construct responder");
    let (failed_run, send_result) = tokio::join!(
        engine.run(Some(&invitation), &mut responder_approval),
        fake_peer.send(Channel::Control, &wrong_hello),
    );
    assert!(matches!(failed_run, Err(PairingError::InvalidInvitation)));
    send_result.expect("send wrong-invite frame");
    assert_not_trusted(&profile, &identity, &peer_identity);

    let (mut unused_peer, responder_transport) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("create replay link");
    let replay = PairingEngine::new(
        responder_transport,
        &identity,
        open_existing(&profile, &identity),
        config([0x75; 16], PairingRole::Responder, 80_001, 1_000),
    )
    .expect("construct responder for replay");
    let replay_error = replay.run(Some(&invitation), &mut responder_approval).await;
    assert!(matches!(
        replay_error,
        Err(PairingError::TrustStore(
            TrustStoreError::InvitationUnavailable
        ))
    ));
    let _ = unused_peer.close().await;
    assert_not_trusted(&profile, &identity, &peer_identity);
}

#[tokio::test]
async fn invitation_token_substitution_breaks_noise_authentication() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let mut store_r = open_new(&profile_r, &identity_r);
    let invitation_r = PairingInvitation::create(&mut store_r, 90_000, Duration::from_secs(60))
        .expect("create invitation");
    let qr = invitation_r.to_qr_bytes().expect("encode invitation");
    let mut wire = kryptopass_protocol::decode_pairing_invite(qr.as_bytes())
        .expect("decode invitation fixture");
    wire.single_use_token[0] ^= 0x80;
    let changed_qr = kryptopass_protocol::encode_pairing_invite(&wire)
        .expect("re-encode substituted invitation token");
    let invitation_i =
        PairingInvitation::from_qr(&changed_qr).expect("substituted token still has valid syntax");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
    let mut approval_i = Decision(LocalDecision::Approve);
    let mut approval_r = Decision(LocalDecision::Approve);
    let session_id = [0x76; 16];

    let (result_i, result_r) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &identity_i,
            open_new(&profile_i, &identity_i),
            config(session_id, PairingRole::Initiator, 90_000, 250),
        )
        .expect("construct initiator")
        .run(Some(&invitation_i), &mut approval_i),
        PairingEngine::new(
            transport_r,
            &identity_r,
            store_r,
            config(session_id, PairingRole::Responder, 90_000, 250),
        )
        .expect("construct responder")
        .run(Some(&invitation_r), &mut approval_r),
    );

    assert!(matches!(
        result_i.as_ref(),
        Err(PairingError::CryptoFailure)
    ));
    assert!(result_r.is_err());
    assert_not_trusted(&profile_i, &identity_i, &identity_r);
    assert_not_trusted(&profile_r, &identity_r, &identity_i);
}

#[tokio::test]
async fn corrupting_authenticated_noise_identity_frame_fails_before_commit() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let mut store_r = open_new(&profile_r, &identity_r);
    let invitation_r = PairingInvitation::create(&mut store_r, 100_000, Duration::from_secs(60))
        .expect("create invitation");
    let qr = invitation_r.to_qr_bytes().expect("encode invitation");
    let invitation_i = PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
    let transport_i = FaultTransport::new(transport_i, 4, SendFault::FlipByte);
    let mut approval_i = Decision(LocalDecision::Approve);
    let mut approval_r = Decision(LocalDecision::Approve);
    let session_id = [0x77; 16];

    let (result_i, result_r) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &identity_i,
            open_new(&profile_i, &identity_i),
            config(session_id, PairingRole::Initiator, 100_000, 350),
        )
        .expect("construct initiator")
        .run(Some(&invitation_i), &mut approval_i),
        PairingEngine::new(
            transport_r,
            &identity_r,
            store_r,
            config(session_id, PairingRole::Responder, 100_000, 350),
        )
        .expect("construct responder")
        .run(Some(&invitation_r), &mut approval_r),
    );

    assert!(result_i.is_err());
    assert!(matches!(
        result_r.as_ref(),
        Err(PairingError::CryptoFailure)
    ));
    assert_not_trusted(&profile_i, &identity_i, &identity_r);
    assert_not_trusted(&profile_r, &identity_r, &identity_i);
}

#[tokio::test]
async fn mutating_noise_message_three_fails_before_identity_or_trust() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let mut store_r = open_new(&profile_r, &identity_r);
    let invitation_r = PairingInvitation::create(&mut store_r, 101_000, Duration::from_secs(60))
        .expect("create invitation");
    let qr = invitation_r.to_qr_bytes().expect("encode invitation");
    let invitation_i = PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
    let transport_i = FaultTransport::new(transport_i, 3, SendFault::FlipByte);
    let mut approval_i = Decision(LocalDecision::Approve);
    let mut approval_r = Decision(LocalDecision::Approve);
    let session_id = [0x80; 16];

    let (result_i, result_r) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &identity_i,
            open_new(&profile_i, &identity_i),
            config(session_id, PairingRole::Initiator, 101_000, 1_000),
        )
        .expect("construct initiator")
        .run(Some(&invitation_i), &mut approval_i),
        PairingEngine::new(
            transport_r,
            &identity_r,
            store_r,
            config(session_id, PairingRole::Responder, 101_000, 1_000),
        )
        .expect("construct responder")
        .run(Some(&invitation_r), &mut approval_r),
    );

    assert!(matches!(
        result_r.as_ref(),
        Err(PairingError::CryptoFailure)
    ));
    assert!(result_i.is_err());
    assert_not_trusted(&profile_i, &identity_i, &identity_r);
    assert_not_trusted(&profile_r, &identity_r, &identity_i);
}

#[tokio::test]
async fn noise_static_key_cannot_substitute_for_a_different_ed25519_signer() {
    let profile = TestProfile::new();
    let attacker_static = DeviceIdentity::generate().expect("generate Noise static identity");
    let claimed_signer = DeviceIdentity::generate().expect("generate substituted signer");
    let responder_identity = DeviceIdentity::generate().expect("generate responder");
    let mut responder_store = open_new(&profile, &responder_identity);
    let invitation =
        PairingInvitation::create(&mut responder_store, 125_000, Duration::from_secs(60))
            .expect("create invitation");
    let (attacker_transport, responder_transport) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect fake peers");
    let mut approval = Decision(LocalDecision::Approve);
    let session_id = [0x7b; 16];
    let responder = PairingEngine::new(
        responder_transport,
        &responder_identity,
        responder_store,
        config(session_id, PairingRole::Responder, 125_000, 1_000),
    )
    .expect("construct responder");

    let (result, ()) = tokio::join!(
        responder.run(Some(&invitation), &mut approval),
        fake_initiator_with_mismatched_ed25519_proof(
            attacker_transport,
            &attacker_static,
            &claimed_signer,
            &invitation,
            session_id,
            false,
        ),
    );

    assert!(
        matches!(result.as_ref(), Err(PairingError::InvalidProof)),
        "actual error: {:?}",
        result.as_ref().err()
    );
    let claimed_identity = PublicIdentity::new(
        claimed_signer.signing_public_key(),
        attacker_static.static_public_key(),
    )
    .expect("construct adversary's claimed identity");
    assert!(matches!(
        open_existing(&profile, &responder_identity)
            .lookup_trusted_peer(&claimed_identity.device_id),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
}

#[tokio::test]
async fn identity_claim_with_a_changed_transcript_hash_is_rejected() {
    let profile = TestProfile::new();
    let initiator_identity = DeviceIdentity::generate().expect("generate initiator");
    let responder_identity = DeviceIdentity::generate().expect("generate responder");
    let mut responder_store = open_new(&profile, &responder_identity);
    let invitation =
        PairingInvitation::create(&mut responder_store, 126_000, Duration::from_secs(60))
            .expect("create invitation");
    let (fake_transport, responder_transport) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect fake peers");
    let mut approval = Decision(LocalDecision::Approve);
    let session_id = [0x7e; 16];
    let responder = PairingEngine::new(
        responder_transport,
        &responder_identity,
        responder_store,
        config(session_id, PairingRole::Responder, 126_000, 1_000),
    )
    .expect("construct responder");

    let (result, ()) = tokio::join!(
        responder.run(Some(&invitation), &mut approval),
        fake_initiator_with_mismatched_ed25519_proof(
            fake_transport,
            &initiator_identity,
            &initiator_identity,
            &invitation,
            session_id,
            true,
        ),
    );

    assert!(
        matches!(result.as_ref(), Err(PairingError::InvalidPeerIdentity)),
        "actual error: {:?}",
        result.as_ref().err()
    );
    assert!(matches!(
        open_existing(&profile, &responder_identity)
            .lookup_trusted_peer(&initiator_identity.device_id()),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
}

#[tokio::test]
async fn network_send_failure_before_readiness_never_commits_trust() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let mut store_r = open_new(&profile_r, &identity_r);
    let invitation_r = PairingInvitation::create(&mut store_r, 110_000, Duration::from_secs(60))
        .expect("create invitation");
    let qr = invitation_r.to_qr_bytes().expect("encode invitation");
    let invitation_i = PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
    let transport_r = FaultTransport::new(transport_r, 4, SendFault::Close);
    let mut approval_i = Decision(LocalDecision::Approve);
    let mut approval_r = Decision(LocalDecision::Approve);
    let session_id = [0x78; 16];

    let (result_i, result_r) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &identity_i,
            open_new(&profile_i, &identity_i),
            config(session_id, PairingRole::Initiator, 110_000, 350),
        )
        .expect("construct initiator")
        .run(Some(&invitation_i), &mut approval_i),
        PairingEngine::new(
            transport_r,
            &identity_r,
            store_r,
            config(session_id, PairingRole::Responder, 110_000, 350),
        )
        .expect("construct responder")
        .run(Some(&invitation_r), &mut approval_r),
    );

    assert!(result_i.is_err());
    assert!(matches!(
        result_r.as_ref(),
        Err(PairingError::Transport(TransportError::Closed))
    ));
    assert_not_trusted(&profile_i, &identity_i, &identity_r);
    assert_not_trusted(&profile_r, &identity_r, &identity_i);
}

#[tokio::test]
async fn failed_local_sqlite_commit_rolls_back_trust_after_both_readiness_messages() {
    let profile_i = TestProfile::new();
    let profile_r = TestProfile::new();
    let identity_i = DeviceIdentity::generate().expect("generate initiator");
    let identity_r = DeviceIdentity::generate().expect("generate responder");
    let store_i = open_new(&profile_i, &identity_i);
    let mut store_r = open_new(&profile_r, &identity_r);
    let invitation_r = PairingInvitation::create(&mut store_r, 120_000, Duration::from_secs(60))
        .expect("create invitation");
    let qr = invitation_r.to_qr_bytes().expect("encode invitation");
    let invitation_i = PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
    no_auto_trust_trigger(&profile_i);
    no_auto_trust_trigger(&profile_r);
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect peers");
    let mut approval_i = Decision(LocalDecision::Approve);
    let mut approval_r = Decision(LocalDecision::Approve);
    let session_id = [0x79; 16];
    let (result_i, result_r) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &identity_i,
            store_i,
            config(session_id, PairingRole::Initiator, 120_000, 1_000),
        )
        .expect("construct initiator")
        .run(Some(&invitation_i), &mut approval_i),
        PairingEngine::new(
            transport_r,
            &identity_r,
            store_r,
            config(session_id, PairingRole::Responder, 120_000, 1_000),
        )
        .expect("construct responder")
        .run(Some(&invitation_r), &mut approval_r),
    );

    let initiator_failed = matches!(result_i.as_ref(), Err(PairingError::TrustStore(_)));
    let responder_failed = matches!(result_r.as_ref(), Err(PairingError::TrustStore(_)));
    assert!(
        initiator_failed || responder_failed,
        "at least one injected local commit must fail; initiator={:?}, responder={:?}",
        result_i.as_ref().err(),
        result_r.as_ref().err()
    );
    remove_no_auto_trust_trigger(&profile_i);
    remove_no_auto_trust_trigger(&profile_r);
    if result_i.is_ok() {
        assert_eq!(
            open_existing(&profile_i, &identity_i)
                .lookup_trusted_peer(&identity_r.device_id())
                .expect("successful local commit is durable"),
            identity_r.public_identity()
        );
    } else {
        assert_not_trusted(&profile_i, &identity_i, &identity_r);
    }
    if result_r.is_ok() {
        assert_eq!(
            open_existing(&profile_r, &identity_r)
                .lookup_trusted_peer(&identity_i.device_id())
                .expect("successful local commit is durable"),
            identity_i.public_identity()
        );
    } else {
        assert_not_trusted(&profile_r, &identity_r, &identity_i);
    }
}

#[tokio::test]
async fn old_encrypted_confirmation_cannot_be_replayed_in_a_fresh_noise_session() {
    let first_profile_i = TestProfile::new();
    let first_profile_r = TestProfile::new();
    let first_identity_i = DeviceIdentity::generate().expect("generate first initiator");
    let first_identity_r = DeviceIdentity::generate().expect("generate first responder");
    let mut first_store_r = open_new(&first_profile_r, &first_identity_r);
    let first_invitation_r =
        PairingInvitation::create(&mut first_store_r, 140_000, Duration::from_secs(60))
            .expect("create first invitation");
    let first_qr = first_invitation_r
        .to_qr_bytes()
        .expect("encode first invitation");
    let first_invitation_i =
        PairingInvitation::from_qr(first_qr.as_bytes()).expect("parse first invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect first pair");
    let captured_confirmation = Arc::new(Mutex::new(None));
    let transport_i = CaptureSendTransport {
        inner: transport_i,
        sends: 0,
        capture_on_send: 5,
        capture: Arc::clone(&captured_confirmation),
    };
    let mut approve_i = Decision(LocalDecision::Approve);
    let mut approve_r = Decision(LocalDecision::Approve);
    let first_session_id = [0x7c; 16];

    let (first_i, first_r) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &first_identity_i,
            open_new(&first_profile_i, &first_identity_i),
            config(first_session_id, PairingRole::Initiator, 140_000, 1_000),
        )
        .expect("construct first initiator")
        .run(Some(&first_invitation_i), &mut approve_i),
        PairingEngine::new(
            transport_r,
            &first_identity_r,
            first_store_r,
            config(first_session_id, PairingRole::Responder, 140_000, 1_000),
        )
        .expect("construct first responder")
        .run(Some(&first_invitation_r), &mut approve_r),
    );
    let mut first_i = first_i.expect("first pairing succeeds");
    let mut first_r = first_r.expect("first pairing succeeds");
    let old_confirmation = captured_confirmation
        .lock()
        .expect("captured frame lock")
        .clone()
        .expect("capture the old encrypted PairingConfirmation frame");
    first_i
        .session
        .close()
        .await
        .expect("close first initiator session");
    first_r
        .session
        .close()
        .await
        .expect("close first responder session");

    let second_profile_i = TestProfile::new();
    let second_profile_r = TestProfile::new();
    let mut second_store_r = open_new(&second_profile_r, &first_identity_r);
    let second_invitation_r =
        PairingInvitation::create(&mut second_store_r, 150_000, Duration::from_secs(60))
            .expect("create second invitation");
    let second_qr = second_invitation_r
        .to_qr_bytes()
        .expect("encode second invitation");
    let second_invitation_i =
        PairingInvitation::from_qr(second_qr.as_bytes()).expect("parse second invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect second pair");
    let transport_r = ReplayReceiveTransport {
        inner: transport_r,
        receives: 0,
        replace_on_receive: 5,
        replay: Arc::new(Mutex::new(Some(old_confirmation))),
    };
    let mut second_approve_i = Decision(LocalDecision::Approve);
    let mut second_approve_r = Decision(LocalDecision::Approve);
    let second_session_id = [0x7d; 16];

    let (second_i, second_r) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &first_identity_i,
            open_new(&second_profile_i, &first_identity_i),
            config(second_session_id, PairingRole::Initiator, 150_000, 1_000),
        )
        .expect("construct second initiator")
        .run(Some(&second_invitation_i), &mut second_approve_i),
        PairingEngine::new(
            transport_r,
            &first_identity_r,
            second_store_r,
            config(second_session_id, PairingRole::Responder, 150_000, 1_000),
        )
        .expect("construct second responder")
        .run(Some(&second_invitation_r), &mut second_approve_r),
    );

    assert!(matches!(
        second_r.as_ref(),
        Err(PairingError::CryptoFailure)
    ));
    assert!(second_i.is_err());
    assert_not_trusted(&second_profile_i, &first_identity_i, &first_identity_r);
    assert_not_trusted(&second_profile_r, &first_identity_r, &first_identity_i);
}
