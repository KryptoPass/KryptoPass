#![allow(clippy::expect_used)] // Test setup names each trust, storage, and protocol boundary.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use kryptopass_devices::{
    ConnectionState, DeviceDirectory, SessionCloseReason, SessionEvent, SessionHandle,
    SyncSessionBinding, start_session_with_sync,
};
use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity, SqliteTrustStore};
use kryptopass_pairing::{
    EstablishedSession, LocalApproval, LocalDecision, PairingConfig, PairingEngine,
    PairingInvitation, PairingRole, SasDisplay,
};
use kryptopass_protocol::Channel;
use kryptopass_sync::{Id, Ledger, Operation, SyncError, SyncProgressState, TrustSource};
use kryptopass_transport::{
    ConnectionInfo, MemoryTransport, MemoryTransportConfig, PeerTransport, TransportError,
    TransportFrame,
};
use tokio::time::timeout;

const VAULT: Id = [0x39; 16];
const OPENPGP_FINGERPRINT: [u8; 20] = [0x42; 20];
const REVISION_CIPHERTEXT_BYTES: usize = 2 * 1024 * 1024;
const DATA_FRAME_DELAY: Duration = Duration::from_millis(100);

struct TestProfile(PathBuf);

impl TestProfile {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kryptopass-sync-owner-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated ephemeral device profile");
        #[cfg(unix)]
        fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .expect("restrict temporary profile to its owner");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestProfile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Default)]
struct TestTrust(HashMap<DeviceId, PublicIdentity>);

impl TestTrust {
    fn from_persisted_pairing(
        profile: &TestProfile,
        local: &DeviceIdentity,
        peer: &DeviceIdentity,
    ) -> Self {
        let store = SqliteTrustStore::open_existing(profile.path(), local.device_id())
            .expect("reopen trust persisted by authenticated pairing");
        let stored_peer = store
            .lookup_trusted_peer(&peer.device_id())
            .expect("read persisted trusted peer");
        assert_eq!(stored_peer, peer.public_identity());
        Self(HashMap::from([
            (local.device_id(), local.public_identity()),
            (peer.device_id(), stored_peer),
        ]))
    }
}

impl TrustSource for TestTrust {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok(self.0.get(&device_id).copied())
    }
}

#[derive(Clone, Default)]
struct SentChannels(Arc<Mutex<Vec<(Channel, Instant)>>>);

impl SentChannels {
    fn record(&self, channel: Channel) {
        self.0
            .lock()
            .expect("test channel trace is not poisoned")
            .push((channel, Instant::now()));
    }

    fn snapshot(&self) -> Vec<(Channel, Instant)> {
        self.0
            .lock()
            .expect("test channel trace is not poisoned")
            .clone()
    }

    fn count(&self, channel: Channel) -> usize {
        self.snapshot()
            .iter()
            .filter(|(sent, _)| *sent == channel)
            .count()
    }

    fn clear(&self) {
        self.0
            .lock()
            .expect("test channel trace is not poisoned")
            .clear();
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

    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        if channel == Channel::Data {
            tokio::time::sleep(DATA_FRAME_DELAY).await;
        }
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

struct PairedDevices {
    owner_profile: TestProfile,
    peer_profile: TestProfile,
    owner_identity: DeviceIdentity,
    peer_identity: DeviceIdentity,
    owner_session: EstablishedSession<RecordingTransport>,
    peer_session: EstablishedSession<RecordingTransport>,
    owner_sent: SentChannels,
    peer_sent: SentChannels,
}

async fn pair_ephemeral_devices() -> PairedDevices {
    let owner_profile = TestProfile::new();
    let peer_profile = TestProfile::new();
    let owner_identity = DeviceIdentity::generate().expect("generate ephemeral owner identity");
    let peer_identity = DeviceIdentity::generate().expect("generate ephemeral peer identity");

    let mut peer_store = SqliteTrustStore::open_new(peer_profile.path(), peer_identity.device_id())
        .expect("create temporary peer trust database");
    let invitation = PairingInvitation::create(&mut peer_store, 10_000, Duration::from_secs(60))
        .expect("create a temporary invitation");
    let (owner_inner, peer_inner) = MemoryTransport::pair(MemoryTransportConfig::default())
        .expect("create the existing authenticated in-memory carrier");
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

    PairedDevices {
        owner_profile,
        peer_profile,
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
    timeout(Duration::from_secs(5), async {
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

async fn wait_for_sync_complete(owner: &SessionHandle, peer: &SessionHandle) {
    timeout(Duration::from_secs(20), async {
        loop {
            let owner_progress = owner.sync_progress();
            let peer_progress = peer.sync_progress();
            assert_ne!(
                owner_progress.map(|progress| progress.state),
                Some(SyncProgressState::Failed),
                "owner session must not fail its Sync round"
            );
            assert_ne!(
                peer_progress.map(|progress| progress.state),
                Some(SyncProgressState::Failed),
                "peer session must not fail its Sync round"
            );
            if owner_progress.is_some_and(|progress| progress.state == SyncProgressState::Complete)
                && peer_progress
                    .is_some_and(|progress| progress.state == SyncProgressState::Complete)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both Sync rounds should commit and complete");
}

async fn wait_for_more_control_frames(owner: &SentChannels, peer: &SentChannels) {
    let owner_before = owner.count(Channel::Control);
    let peer_before = peer.count(Channel::Control);
    timeout(Duration::from_secs(5), async {
        loop {
            if owner.count(Channel::Control) > owner_before
                && peer.count(Channel::Control) > peer_before
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("post-sync keepalive control frames should continue on both owners");
}

async fn wait_for_received_revision(handle: &SessionHandle, ledger: &Ledger, expected: usize) {
    timeout(Duration::from_secs(10), async {
        loop {
            let progress = handle.sync_progress();
            assert_ne!(
                progress.map(|value| value.state),
                Some(SyncProgressState::Failed),
                "continuous owner must not fail while starting its follow-up round"
            );
            if ledger
                .revision_count()
                .expect("read the isolated observer ledger")
                == expected
                && progress.is_some_and(|value| {
                    value.state == SyncProgressState::Complete && value.received_revisions > 0
                })
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the same authenticated owner should commit the follow-up revision");
}

#[tokio::test]
async fn authenticated_owner_commits_bidirectional_sync_on_data_while_keepalive_runs() {
    let paired = pair_ephemeral_devices().await;
    let PairedDevices {
        owner_profile,
        peer_profile,
        owner_identity,
        peer_identity,
        owner_session,
        peer_session,
        owner_sent,
        peer_sent,
    } = paired;
    let owner_id = owner_identity.device_id();
    let peer_id = peer_identity.device_id();
    let owner_sends_first_large_batch = owner_id < peer_id;
    let owner_public = owner_identity.public_identity();
    let peer_public = peer_identity.public_identity();
    let owner_trust =
        TestTrust::from_persisted_pairing(&owner_profile, &owner_identity, &peer_identity);
    let peer_trust =
        TestTrust::from_persisted_pairing(&peer_profile, &peer_identity, &owner_identity);
    let owner_ledger_path = owner_profile.path().join("sync-ledger.sqlite");
    let peer_ledger_path = peer_profile.path().join("sync-ledger.sqlite");
    let mut owner_ledger = Ledger::create(&owner_ledger_path, VAULT).expect("owner private ledger");
    let mut peer_ledger = Ledger::create(&peer_ledger_path, VAULT).expect("peer private ledger");
    owner_ledger
        .append_local(
            &owner_identity,
            [0x61; 16],
            &[],
            Operation::Write,
            vec![
                0x71;
                if owner_sends_first_large_batch {
                    REVISION_CIPHERTEXT_BYTES
                } else {
                    32
                }
            ],
            &owner_trust,
        )
        .expect("owner signs its isolated offline revision");
    peer_ledger
        .append_local(
            &peer_identity,
            [0x62; 16],
            &[],
            Operation::Write,
            vec![
                0x72;
                if owner_sends_first_large_batch {
                    32
                } else {
                    REVISION_CIPHERTEXT_BYTES
                }
            ],
            &peer_trust,
        )
        .expect("peer signs its isolated offline revision");

    let owner_directory = DeviceDirectory::new(owner_id);
    owner_directory
        .replace_trusted(vec![peer_public])
        .expect("owner read model contains only paired peer");
    let peer_directory = DeviceDirectory::new(peer_id);
    peer_directory
        .replace_trusted(vec![owner_public])
        .expect("peer read model contains only paired owner");
    let owner_handle = start_session_with_sync(
        owner_directory.clone(),
        owner_session,
        owner_id,
        peer_public,
        SyncSessionBinding::new(
            owner_ledger,
            Box::new(owner_trust.clone()),
            OPENPGP_FINGERPRINT.to_vec(),
        ),
    )
    .expect("start owner with an explicit private-ledger binding");
    let peer_handle = start_session_with_sync(
        peer_directory.clone(),
        peer_session,
        peer_id,
        owner_public,
        SyncSessionBinding::new(
            peer_ledger,
            Box::new(peer_trust.clone()),
            OPENPGP_FINGERPRINT.to_vec(),
        ),
    )
    .expect("start peer with an explicit private-ledger binding");

    wait_until_connected(&owner_handle).await;
    wait_until_connected(&peer_handle).await;
    wait_for_sync_complete(&owner_handle, &peer_handle).await;

    let owner_progress = owner_handle
        .sync_progress()
        .expect("owner exposes volatile Sync progress");
    let peer_progress = peer_handle
        .sync_progress()
        .expect("peer exposes volatile Sync progress");
    assert_eq!(owner_progress.received_revisions, 1);
    assert_eq!(owner_progress.acknowledged_revisions, 1);
    assert_eq!(peer_progress.received_revisions, 1);
    assert_eq!(peer_progress.acknowledged_revisions, 1);

    let owner_data: Vec<_> = owner_sent
        .snapshot()
        .into_iter()
        .filter(|(channel, _)| *channel == Channel::Data)
        .collect();
    let peer_data: Vec<_> = peer_sent
        .snapshot()
        .into_iter()
        .filter(|(channel, _)| *channel == Channel::Data)
        .collect();
    assert!(
        !owner_data.is_empty(),
        "owner's revision used the DATA lane"
    );
    assert!(!peer_data.is_empty(), "peer's revision used the DATA lane");
    let large_sender = if owner_sends_first_large_batch {
        &owner_data
    } else {
        &peer_data
    };
    assert!(
        large_sender.len() > 1,
        "large revision is fragmented on DATA"
    );
    let large_start = large_sender[0].1;
    let large_end = large_sender.last().expect("large DATA fragments").1;
    assert!(
        large_end.duration_since(large_start) >= Duration::from_secs(3),
        "DATA transfer remains active across a keepalive interval"
    );
    let large_sender_trace = if owner_sends_first_large_batch {
        &owner_sent
    } else {
        &peer_sent
    };
    assert!(
        large_sender_trace
            .snapshot()
            .iter()
            .any(|(channel, sent_at)| {
                *channel == Channel::Control && *sent_at > large_start && *sent_at < large_end
            }),
        "the authenticated owner sends a keepalive control while DATA is in flight"
    );
    assert!(matches!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    ));
    assert!(matches!(
        peer_directory
            .snapshot()
            .get(&owner_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    ));

    let reopened_owner = Ledger::open_existing(&owner_ledger_path, VAULT, &owner_trust)
        .expect("reopen the owner ledger after remote commit");
    let reopened_peer = Ledger::open_existing(&peer_ledger_path, VAULT, &peer_trust)
        .expect("reopen the peer ledger after remote commit");
    assert_eq!(
        reopened_owner
            .revision_count()
            .expect("durable owner count"),
        2
    );
    assert_eq!(
        reopened_peer.revision_count().expect("durable peer count"),
        2
    );

    wait_for_more_control_frames(&owner_sent, &peer_sent).await;
    assert!(matches!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    ));
    assert!(matches!(
        peer_directory
            .snapshot()
            .get(&owner_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    ));
}

#[tokio::test]
async fn automatic_rounds_reuse_noise_session_without_echo_and_close_gracefully() {
    let paired = pair_ephemeral_devices().await;
    let PairedDevices {
        owner_profile,
        peer_profile,
        owner_identity,
        peer_identity,
        owner_session,
        peer_session,
        owner_sent,
        peer_sent,
    } = paired;
    let owner_id = owner_identity.device_id();
    let peer_id = peer_identity.device_id();
    let owner_public = owner_identity.public_identity();
    let peer_public = peer_identity.public_identity();
    let owner_trust =
        TestTrust::from_persisted_pairing(&owner_profile, &owner_identity, &peer_identity);
    let peer_trust =
        TestTrust::from_persisted_pairing(&peer_profile, &peer_identity, &owner_identity);
    let owner_ledger_path = owner_profile.path().join("continuous-ledger.sqlite");
    let peer_ledger_path = peer_profile.path().join("continuous-ledger.sqlite");
    let owner_ledger = Ledger::create(&owner_ledger_path, VAULT)
        .expect("create the isolated owner ciphertext ledger");
    let peer_ledger = Ledger::create(&peer_ledger_path, VAULT)
        .expect("create the isolated peer ciphertext ledger");
    let mut owner_writer = Ledger::open_existing(&owner_ledger_path, VAULT, &owner_trust)
        .expect("open a separate local writer handle");
    let peer_observer = Ledger::open_existing(&peer_ledger_path, VAULT, &peer_trust)
        .expect("open a separate peer observer handle");

    let owner_directory = DeviceDirectory::new(owner_id);
    owner_directory
        .replace_trusted(vec![peer_public])
        .expect("owner read model contains only paired peer");
    let peer_directory = DeviceDirectory::new(peer_id);
    peer_directory
        .replace_trusted(vec![owner_public])
        .expect("peer read model contains only paired owner");
    let mut owner_handle = start_session_with_sync(
        owner_directory.clone(),
        owner_session,
        owner_id,
        peer_public,
        SyncSessionBinding::new(
            owner_ledger,
            Box::new(owner_trust.clone()),
            OPENPGP_FINGERPRINT.to_vec(),
        )
        .with_automatic_rounds(),
    )
    .expect("start owner with explicitly enabled automatic rounds");
    let mut peer_handle = start_session_with_sync(
        peer_directory.clone(),
        peer_session,
        peer_id,
        owner_public,
        SyncSessionBinding::new(
            peer_ledger,
            Box::new(peer_trust.clone()),
            OPENPGP_FINGERPRINT.to_vec(),
        )
        .with_automatic_rounds(),
    )
    .expect("start peer with explicitly enabled automatic rounds");

    wait_until_connected(&owner_handle).await;
    wait_until_connected(&peer_handle).await;
    wait_for_sync_complete(&owner_handle, &peer_handle).await;
    wait_for_more_control_frames(&owner_sent, &peer_sent).await;

    owner_writer
        .append_local(
            &owner_identity,
            [0x63; 16],
            &[],
            Operation::Write,
            b"synthetic ciphertext committed after the first round".to_vec(),
            &owner_trust,
        )
        .expect("append through the second handle while the Noise owner remains active");
    wait_for_received_revision(&peer_handle, &peer_observer, 1).await;
    assert_eq!(
        owner_handle
            .sync_progress()
            .expect("owner follow-up progress")
            .acknowledged_revisions,
        1
    );

    owner_sent.clear();
    peer_sent.clear();
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(
        owner_handle.sync_progress().map(|progress| progress.state),
        Some(SyncProgressState::Complete),
        "the sender remains complete without another local write"
    );
    assert_eq!(
        peer_handle.sync_progress().map(|progress| progress.state),
        Some(SyncProgressState::Complete),
        "a received revision must not ping-pong into an empty next round"
    );
    assert_eq!(owner_sent.count(Channel::Control), 0);
    assert_eq!(peer_sent.count(Channel::Control), 0);
    assert_eq!(
        peer_observer
            .revision_count()
            .expect("durable peer revision count"),
        1
    );

    wait_for_more_control_frames(&owner_sent, &peer_sent).await;
    assert_eq!(
        owner_handle.sync_progress().map(|progress| progress.state),
        Some(SyncProgressState::Complete)
    );
    assert_eq!(
        peer_handle.sync_progress().map(|progress| progress.state),
        Some(SyncProgressState::Complete)
    );

    owner_handle
        .disconnect(SessionCloseReason::UserRequested)
        .expect("request a graceful authenticated close");
    timeout(Duration::from_secs(5), owner_handle.wait_closed())
        .await
        .expect("owner finishes graceful close");
    timeout(Duration::from_secs(5), peer_handle.wait_closed())
        .await
        .expect("peer observes graceful close");
    assert_eq!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Disconnected)
    );
    assert_eq!(
        peer_directory
            .snapshot()
            .get(&owner_id)
            .map(|device| device.connection),
        Some(ConnectionState::Disconnected)
    );
}
