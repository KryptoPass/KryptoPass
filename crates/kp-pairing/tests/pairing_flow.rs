#![allow(
    clippy::expect_used,
    reason = "Pairing integration tests use expect only for isolated fixture setup and local assertions."
)]

use async_trait::async_trait;
use kryptopass_identity::{DeviceIdentity, PublicIdentity, SqliteTrustStore};
use kryptopass_pairing::{
    LocalApproval, LocalDecision, PairingConfig, PairingEngine, PairingInvitation, PairingMode,
    PairingRole, PairingState, SasDisplay,
};
use kryptopass_protocol::decode_pairing_invite;
use kryptopass_transport::{MemoryTransport, MemoryTransportConfig};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

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
            "kryptopass-pairing-test-{}-{stamp}-{id}",
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
struct Approval {
    decision: LocalDecision,
    seen: Arc<Mutex<Vec<SasDisplay>>>,
}

impl Approval {
    fn new(decision: LocalDecision) -> Self {
        Self {
            decision,
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn decisions(&self) -> Vec<SasDisplay> {
        self.seen.lock().expect("approval lock").clone()
    }
}

#[async_trait]
impl LocalApproval for Approval {
    async fn confirm_sas(&mut self, sas: &SasDisplay) -> LocalDecision {
        self.seen.lock().expect("approval lock").push(*sas);
        self.decision
    }
}

#[derive(Default)]
struct NoApproval {
    calls: Arc<Mutex<usize>>,
}

impl NoApproval {
    fn call_count(&self) -> usize {
        *self.calls.lock().expect("approval lock")
    }
}

#[async_trait]
impl LocalApproval for NoApproval {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        *self.calls.lock().expect("approval lock") += 1;
        LocalDecision::Cancel
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

fn short_config(session_id: [u8; 16], role: PairingRole, now_ms: i64) -> PairingConfig {
    let mut config = PairingConfig::pairing(session_id, role, now_ms);
    config.timeout = Duration::from_secs(3);
    config
}

fn assert_trusted(profile: &TestProfile, local: &DeviceIdentity, peer: PublicIdentity) {
    let store = open_existing(profile, local);
    assert_eq!(
        store
            .lookup_trusted_peer(&peer.device_id)
            .expect("lookup persisted peer"),
        peer,
        "a successful pairing must persist exactly the authenticated identity"
    );
}

#[tokio::test]
async fn two_nodes_compare_the_same_sas_and_commit_trust_before_returning() {
    let initiator_profile = TestProfile::new();
    let responder_profile = TestProfile::new();
    let initiator_identity = DeviceIdentity::generate().expect("generate initiator identity");
    let responder_identity = DeviceIdentity::generate().expect("generate responder identity");
    let initiator_peer = initiator_identity.public_identity();
    let responder_peer = responder_identity.public_identity();

    let initiator_store = open_new(&initiator_profile, &initiator_identity);
    let mut responder_store = open_new(&responder_profile, &responder_identity);
    let hosted_invitation =
        PairingInvitation::create(&mut responder_store, 10_000, Duration::from_secs(60))
            .expect("create one-shot invitation on the responder");
    let qr = hosted_invitation
        .to_qr_bytes()
        .expect("encode invitation QR");
    let invitation_for_initiator =
        PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation received out of band");

    let (initiator_transport, responder_transport) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect memory nodes");
    let mut initiator_approval = Approval::new(LocalDecision::Approve);
    let mut responder_approval = Approval::new(LocalDecision::Approve);
    let session_id = [0x5a; 16];

    let initiator = PairingEngine::new(
        initiator_transport,
        &initiator_identity,
        initiator_store,
        short_config(session_id, PairingRole::Initiator, 10_000),
    )
    .expect("construct initiator");
    let responder = PairingEngine::new(
        responder_transport,
        &responder_identity,
        responder_store,
        short_config(session_id, PairingRole::Responder, 10_000),
    )
    .expect("construct responder");

    let (initiator_result, responder_result) = tokio::join!(
        initiator.run(Some(&invitation_for_initiator), &mut initiator_approval),
        responder.run(Some(&hosted_invitation), &mut responder_approval),
    );
    let mut initiator_outcome = initiator_result.expect("initiator completes pairing");
    let mut responder_outcome = responder_result.expect("responder completes pairing");

    assert_eq!(initiator_outcome.state, PairingState::Trusted);
    assert_eq!(responder_outcome.state, PairingState::Trusted);
    assert_eq!(initiator_outcome.mode, PairingMode::Pairing);
    assert_eq!(responder_outcome.mode, PairingMode::Pairing);
    assert_eq!(initiator_outcome.peer, responder_peer);
    assert_eq!(responder_outcome.peer, initiator_peer);

    let initiator_sas = initiator_outcome.sas.expect("pairing displays a SAS");
    let responder_sas = responder_outcome.sas.expect("pairing displays a SAS");
    assert_eq!(initiator_sas.indices(), responder_sas.indices());
    assert_eq!(initiator_sas.emojis, responder_sas.emojis);
    assert_eq!(initiator_approval.decisions(), vec![initiator_sas]);
    assert_eq!(responder_approval.decisions(), vec![responder_sas]);

    // Each successful result is already locally persisted when `run` returns.
    assert_trusted(&initiator_profile, &initiator_identity, responder_peer);
    assert_trusted(&responder_profile, &responder_identity, initiator_peer);

    initiator_outcome
        .session
        .close()
        .await
        .expect("close the initiator's established session");
    responder_outcome
        .session
        .close()
        .await
        .expect("close the responder's established session");
    assert_trusted(&initiator_profile, &initiator_identity, responder_peer);
    assert_trusted(&responder_profile, &responder_identity, initiator_peer);
}

#[tokio::test]
async fn reconnect_uses_fresh_noise_and_keeps_trust_without_showing_sas() {
    let initiator_profile = TestProfile::new();
    let responder_profile = TestProfile::new();
    let initiator_identity = DeviceIdentity::generate().expect("generate initiator identity");
    let responder_identity = DeviceIdentity::generate().expect("generate responder identity");
    let initiator_peer = initiator_identity.public_identity();
    let responder_peer = responder_identity.public_identity();
    let mut responder_store = open_new(&responder_profile, &responder_identity);
    let hosted_invitation =
        PairingInvitation::create(&mut responder_store, 20_000, Duration::from_secs(60))
            .expect("create invitation");
    let qr = hosted_invitation
        .to_qr_bytes()
        .expect("encode invitation QR");
    let invitation_for_initiator =
        PairingInvitation::from_qr(qr.as_bytes()).expect("parse invitation");
    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect memory nodes");
    let mut approve_i = Approval::new(LocalDecision::Approve);
    let mut approve_r = Approval::new(LocalDecision::Approve);
    let first_session = [0x61; 16];
    let (initiator, responder) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &initiator_identity,
            open_new(&initiator_profile, &initiator_identity),
            short_config(first_session, PairingRole::Initiator, 20_000),
        )
        .expect("construct initial initiator")
        .run(Some(&invitation_for_initiator), &mut approve_i),
        PairingEngine::new(
            transport_r,
            &responder_identity,
            responder_store,
            short_config(first_session, PairingRole::Responder, 20_000),
        )
        .expect("construct initial responder")
        .run(Some(&hosted_invitation), &mut approve_r),
    );
    let mut initiator = initiator.expect("initial initiator pairing");
    let mut responder = responder.expect("initial responder pairing");
    assert!(initiator.sas.is_some());
    assert!(responder.sas.is_some());
    initiator
        .session
        .close()
        .await
        .expect("close first initiator session");
    responder
        .session
        .close()
        .await
        .expect("close first responder session");

    let (transport_i, transport_r) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("connect again");
    let mut no_approval_i = NoApproval::default();
    let mut no_approval_r = NoApproval::default();
    let mut reconnect_i =
        PairingConfig::reconnect([0x62; 16], PairingRole::Initiator, 20_100, responder_peer);
    let mut reconnect_r =
        PairingConfig::reconnect([0x62; 16], PairingRole::Responder, 20_100, initiator_peer);
    reconnect_i.timeout = Duration::from_secs(3);
    reconnect_r.timeout = Duration::from_secs(3);

    let (initiator, responder) = tokio::join!(
        PairingEngine::new(
            transport_i,
            &initiator_identity,
            open_existing(&initiator_profile, &initiator_identity),
            reconnect_i,
        )
        .expect("construct trusted initiator reconnect")
        .run(None, &mut no_approval_i),
        PairingEngine::new(
            transport_r,
            &responder_identity,
            open_existing(&responder_profile, &responder_identity),
            reconnect_r,
        )
        .expect("construct trusted responder reconnect")
        .run(None, &mut no_approval_r),
    );
    let mut initiator = initiator.expect("initiator reconnect succeeds");
    let mut responder = responder.expect("responder reconnect succeeds");
    assert_eq!(initiator.mode, PairingMode::Reconnect);
    assert_eq!(responder.mode, PairingMode::Reconnect);
    assert!(initiator.sas.is_none());
    assert!(responder.sas.is_none());
    assert_eq!(no_approval_i.call_count(), 0);
    assert_eq!(no_approval_r.call_count(), 0);
    assert_trusted(&initiator_profile, &initiator_identity, responder_peer);
    assert_trusted(&responder_profile, &responder_identity, initiator_peer);
    initiator
        .session
        .close()
        .await
        .expect("close reconnect initiator");
    responder
        .session
        .close()
        .await
        .expect("close reconnect responder");
}

#[test]
fn invitation_qr_and_debug_output_do_not_expose_the_bearer_token() {
    let profile = TestProfile::new();
    let identity = DeviceIdentity::generate().expect("generate host identity");
    let mut store = open_new(&profile, &identity);
    let invitation = PairingInvitation::create(&mut store, 30_000, Duration::from_secs(10))
        .expect("create invitation");
    let qr = invitation.to_qr_bytes().expect("encode QR");
    let parsed = PairingInvitation::from_qr(qr.as_bytes()).expect("parse QR");

    let invitation_debug = format!("{invitation:?}");
    let parsed_debug = format!("{parsed:?}");
    let qr_debug = format!("{qr:?}");
    let wire = decode_pairing_invite(qr.as_bytes()).expect("decode QR fixture");
    let token_hex = wire
        .single_use_token
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    assert!(!invitation_debug.contains(&token_hex));
    assert!(!parsed_debug.contains(&token_hex));
    assert!(!qr_debug.contains(&token_hex));
    assert!(qr_debug.contains("redacted"));
}
