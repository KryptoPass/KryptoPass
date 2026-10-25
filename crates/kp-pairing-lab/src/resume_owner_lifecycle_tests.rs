#![allow(
    clippy::expect_used,
    reason = "The in-memory pairing fixture uses expect for setup and assertions."
)]

use super::{AcceptPolicy, OwnedSession, OwnerCommand, OwnerRuntime, ResumeOwnerReady};
use async_trait::async_trait;
use kryptopass_devices::{
    ConnectionState, DeviceDirectory, SessionCloseReason, SessionEvent, SessionHandle,
    start_session,
};
use kryptopass_identity::{DeviceIdentity, SqliteTrustStore};
use kryptopass_pairing::{
    EstablishedSession, LocalApproval, LocalDecision, PairingConfig, PairingEngine,
    PairingInvitation, PairingMode, PairingRole, SasDisplay,
};
use kryptopass_transport::{MemoryTransport, MemoryTransportConfig};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{sync::mpsc, time::timeout};

struct TestProfile(PathBuf);

impl TestProfile {
    fn new() -> Self {
        static NEXT_PROFILE: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let nonce = NEXT_PROFILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kryptopass-resume-owner-lifecycle-{}-{stamp}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated in-memory pairing profile");
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

struct Approve;

#[async_trait]
impl LocalApproval for Approve {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        LocalDecision::Approve
    }
}

#[derive(Default)]
struct UnexpectedApproval {
    calls: usize,
}

#[async_trait]
impl LocalApproval for UnexpectedApproval {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        self.calls += 1;
        LocalDecision::Cancel
    }
}

struct PairedMemorySessions {
    _owner_profile: TestProfile,
    _peer_profile: TestProfile,
    owner_identity: DeviceIdentity,
    peer_identity: DeviceIdentity,
    owner_session: EstablishedSession<MemoryTransport>,
    peer_session: EstablishedSession<MemoryTransport>,
}

async fn pair_two_ephemeral_devices() -> PairedMemorySessions {
    let owner_profile = TestProfile::new();
    let peer_profile = TestProfile::new();
    let owner_identity = DeviceIdentity::generate().expect("generate ephemeral owner identity");
    let peer_identity = DeviceIdentity::generate().expect("generate ephemeral peer identity");

    let mut peer_store = SqliteTrustStore::open_new(peer_profile.path(), peer_identity.device_id())
        .expect("open temporary peer trust database");
    let invitation = PairingInvitation::create(&mut peer_store, 10_000, Duration::from_secs(60))
        .expect("create an invitation in the temporary peer profile");
    let (owner_transport, peer_transport) = MemoryTransport::pair(MemoryTransportConfig {
        send_timeout: None,
        receive_timeout: None,
        ..MemoryTransportConfig::default()
    })
    .expect("connect the two test identities in memory");
    let mut owner_approval = Approve;
    let mut peer_approval = Approve;
    let mut owner_config = PairingConfig::pairing([0x51; 16], PairingRole::Initiator, 10_000);
    owner_config.timeout = Duration::from_secs(3);
    let mut peer_config = PairingConfig::pairing([0x51; 16], PairingRole::Responder, 10_000);
    peer_config.timeout = Duration::from_secs(3);

    let (owner_pairing, peer_pairing) = tokio::join!(
        PairingEngine::new(
            owner_transport,
            &owner_identity,
            SqliteTrustStore::open_new(owner_profile.path(), owner_identity.device_id())
                .expect("open temporary owner trust database"),
            owner_config,
        )
        .expect("construct ephemeral owner pairing")
        .run(Some(&invitation), &mut owner_approval),
        PairingEngine::new(peer_transport, &peer_identity, peer_store, peer_config,)
            .expect("construct ephemeral peer pairing")
            .run(Some(&invitation), &mut peer_approval),
    );
    let owner_pairing = owner_pairing.expect("owner pairing authenticates in memory");
    let peer_pairing = peer_pairing.expect("peer pairing authenticates in memory");

    PairedMemorySessions {
        _owner_profile: owner_profile,
        _peer_profile: peer_profile,
        owner_identity,
        peer_identity,
        owner_session: owner_pairing.session,
        peer_session: peer_pairing.session,
    }
}

async fn wait_until_connected(handle: &SessionHandle) {
    let mut lifecycle = handle.subscribe_lifecycle();
    timeout(Duration::from_secs(3), async {
        loop {
            if matches!(
                *lifecycle.borrow_and_update(),
                Some(SessionEvent::Connected { .. })
            ) {
                break;
            }
            lifecycle
                .changed()
                .await
                .expect("active session lifecycle stays observable");
        }
    })
    .await
    .expect("both authenticated session hellos complete");
}

struct OwnerHarness {
    runtime: OwnerRuntime,
    command_receiver: mpsc::Receiver<OwnerCommand>,
    policy: Arc<Mutex<AcceptPolicy>>,
    snapshots: tokio::sync::watch::Sender<super::ResumeOwnerSnapshot>,
    peer_id: kryptopass_identity::DeviceId,
    generation: u64,
}

fn start_owner_watcher(
    profile: &TestProfile,
    directory: DeviceDirectory,
    peer: kryptopass_identity::PublicIdentity,
    session: SessionHandle,
) -> OwnerHarness {
    let peer_id = peer.device_id;
    let generation = session.generation();
    let ready = ResumeOwnerReady {
        trusted_peers: vec![peer],
        devices: directory.clone(),
        reconnect_port: 0,
    };
    let (command_sender, command_receiver) = mpsc::channel(16);
    let runtime = OwnerRuntime {
        directory_path: profile.path().to_path_buf(),
        host: None,
        devices: Some(directory.clone()),
        ready: Some(ready),
        listener: None,
        relay_workers: HashMap::new(),
        sessions: HashMap::from([(peer_id, OwnedSession { handle: session })]),
        pending_attempts: HashMap::new(),
        next_attempt_id: 1,
        last_snapshot_revision: directory.snapshot().revision,
        last_listener_bind: None,
        command_sender,
        network_settings: None,
        network_settings_error: false,
        network_guard: None,
        route_snapshot_unavailable: false,
        synthetic_controls: HashMap::new(),
        synthetic_transfers: std::collections::BTreeMap::new(),
    };
    let policy = Arc::new(Mutex::new(AcceptPolicy {
        active: HashSet::from([peer_id]),
        ..AcceptPolicy::default()
    }));
    let (snapshots, _snapshot_receiver) =
        tokio::sync::watch::channel(super::ResumeOwnerSnapshot::default());
    runtime.watch_session(
        peer_id,
        generation,
        &runtime
            .sessions
            .get(&peer_id)
            .expect("owner retains the session handle")
            .handle,
    );
    OwnerHarness {
        runtime,
        command_receiver,
        policy,
        snapshots,
        peer_id,
        generation,
    }
}

async fn assert_no_premature_end(harness: &mut OwnerHarness, directory: &DeviceDirectory) {
    assert!(matches!(
        directory
            .snapshot()
            .get(&harness.peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    ));
    assert!(
        timeout(Duration::from_millis(150), harness.command_receiver.recv())
            .await
            .is_err(),
        "Connected must not enqueue SessionEnded"
    );
    assert!(harness.runtime.sessions.contains_key(&harness.peer_id));
}

async fn end_session_and_process_terminal_event(
    harness: &mut OwnerHarness,
    mut peer_session: SessionHandle,
) {
    peer_session
        .disconnect(SessionCloseReason::UserRequested)
        .expect("peer requests a real terminal session event");
    let terminal_command = timeout(Duration::from_secs(3), harness.command_receiver.recv())
        .await
        .expect("terminal lifecycle event reaches the owner")
        .expect("owner command channel remains open");
    assert!(matches!(
        terminal_command,
        OwnerCommand::SessionEnded(peer, generation)
            if peer == harness.peer_id && generation == harness.generation
    ));
    assert!(
        !harness
            .runtime
            .handle_command(terminal_command, &harness.snapshots, &harness.policy)
            .await
    );
    assert!(!harness.runtime.sessions.contains_key(&harness.peer_id));
    assert!(
        !harness
            .policy
            .lock()
            .expect("owner accept policy lock")
            .active
            .contains(&harness.peer_id)
    );
    peer_session.wait_closed().await;
}

#[tokio::test]
async fn connected_lifecycle_does_not_end_session_when_watcher_is_already_subscribed() {
    let paired = pair_two_ephemeral_devices().await;
    let owner_public = paired.owner_identity.public_identity();
    let peer_public = paired.peer_identity.public_identity();
    let owner_directory = DeviceDirectory::new(paired.owner_identity.device_id());
    owner_directory
        .replace_trusted(vec![peer_public])
        .expect("trust only the ephemeral peer identity");
    let peer_directory = DeviceDirectory::new(paired.peer_identity.device_id());
    peer_directory
        .replace_trusted(vec![owner_public])
        .expect("trust only the ephemeral owner identity");
    let owner_session = start_session(
        owner_directory.clone(),
        paired.owner_session,
        paired.owner_identity.device_id(),
        peer_public,
    )
    .expect("start owner-side authenticated session");
    let mut harness = start_owner_watcher(
        &paired._owner_profile,
        owner_directory.clone(),
        peer_public,
        owner_session,
    );

    // The owner hello is queued, but the peer session has not been started yet.
    // Give the spawned watcher time to subscribe before the peer publishes Connected.
    tokio::time::sleep(Duration::from_millis(20)).await;
    let peer_session = start_session(
        peer_directory,
        paired.peer_session,
        paired.peer_identity.device_id(),
        owner_public,
    )
    .expect("start peer-side authenticated session");
    wait_until_connected(
        &harness
            .runtime
            .sessions
            .get(&harness.peer_id)
            .expect("owner retains the session")
            .handle,
    )
    .await;
    wait_until_connected(&peer_session).await;

    assert_no_premature_end(&mut harness, &owner_directory).await;
    end_session_and_process_terminal_event(&mut harness, peer_session).await;
}

#[tokio::test]
async fn connected_lifecycle_already_published_before_subscription_keeps_session_owned() {
    let paired = pair_two_ephemeral_devices().await;
    let owner_public = paired.owner_identity.public_identity();
    let peer_public = paired.peer_identity.public_identity();
    let owner_directory = DeviceDirectory::new(paired.owner_identity.device_id());
    owner_directory
        .replace_trusted(vec![peer_public])
        .expect("trust only the ephemeral peer identity");
    let peer_directory = DeviceDirectory::new(paired.peer_identity.device_id());
    peer_directory
        .replace_trusted(vec![owner_public])
        .expect("trust only the ephemeral owner identity");
    let owner_session = start_session(
        owner_directory.clone(),
        paired.owner_session,
        paired.owner_identity.device_id(),
        peer_public,
    )
    .expect("start owner-side authenticated session");
    let peer_session = start_session(
        peer_directory,
        paired.peer_session,
        paired.peer_identity.device_id(),
        owner_public,
    )
    .expect("start peer-side authenticated session");
    wait_until_connected(&owner_session).await;
    wait_until_connected(&peer_session).await;

    let mut harness = start_owner_watcher(
        &paired._owner_profile,
        owner_directory.clone(),
        peer_public,
        owner_session,
    );
    assert_no_premature_end(&mut harness, &owner_directory).await;
    end_session_and_process_terminal_event(&mut harness, peer_session).await;
}

#[tokio::test]
async fn stale_session_end_from_previous_generation_does_not_remove_authenticated_reconnect() {
    let paired = pair_two_ephemeral_devices().await;
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

    let owner_session = start_session(
        owner_directory.clone(),
        paired.owner_session,
        owner_id,
        peer_public,
    )
    .expect("start the first owner session");
    let old_generation = owner_session.generation();
    let mut harness = start_owner_watcher(
        &paired._owner_profile,
        owner_directory.clone(),
        peer_public,
        owner_session,
    );
    let mut peer_session = start_session(
        peer_directory.clone(),
        paired.peer_session,
        peer_id,
        owner_public,
    )
    .expect("start the first peer session");
    wait_until_connected(
        &harness
            .runtime
            .sessions
            .get(&peer_id)
            .expect("owner retains the first session")
            .handle,
    )
    .await;
    wait_until_connected(&peer_session).await;

    // Capture the old owner's real terminal event but defer processing it while
    // a fresh authenticated session for the same pinned peer becomes active.
    peer_session
        .disconnect(SessionCloseReason::UserRequested)
        .expect("peer requests a real user disconnect");
    let old_terminal_command = timeout(Duration::from_secs(3), harness.command_receiver.recv())
        .await
        .expect("old session terminal event reaches the process owner")
        .expect("owner command channel remains open");
    assert!(matches!(
        &old_terminal_command,
        OwnerCommand::SessionEnded(peer, generation)
            if *peer == peer_id && *generation == old_generation
    ));
    harness
        .runtime
        .sessions
        .get_mut(&peer_id)
        .expect("owner still retains the old session until the event is processed")
        .handle
        .wait_closed()
        .await;
    peer_session.wait_closed().await;
    assert!(matches!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Disconnected)
    ));

    let (owner_transport, peer_transport) = MemoryTransport::pair(MemoryTransportConfig::default())
        .expect("create a fresh carrier for the authenticated reconnect");
    let owner_store = SqliteTrustStore::open_existing(paired._owner_profile.path(), owner_id)
        .expect("reopen the owner's persisted trust profile");
    let peer_store = SqliteTrustStore::open_existing(paired._peer_profile.path(), peer_id)
        .expect("reopen the peer's persisted trust profile");
    assert_eq!(
        owner_store
            .lookup_trusted_peer(&peer_id)
            .expect("owner's exact peer pin survives disconnect"),
        peer_public
    );
    assert_eq!(
        peer_store
            .lookup_trusted_peer(&owner_id)
            .expect("peer's exact owner pin survives disconnect"),
        owner_public
    );
    let mut owner_config =
        PairingConfig::reconnect([0x52; 16], PairingRole::Initiator, 20_100, peer_public);
    owner_config.timeout = Duration::from_secs(3);
    let mut peer_config =
        PairingConfig::reconnect([0x52; 16], PairingRole::Responder, 20_100, owner_public);
    peer_config.timeout = Duration::from_secs(3);
    let mut owner_approval = UnexpectedApproval::default();
    let mut peer_approval = UnexpectedApproval::default();
    let (owner_reconnect, peer_reconnect) = tokio::join!(
        PairingEngine::new(
            owner_transport,
            &paired.owner_identity,
            owner_store,
            owner_config,
        )
        .expect("construct the pinned owner reconnect")
        .run(None, &mut owner_approval),
        PairingEngine::new(
            peer_transport,
            &paired.peer_identity,
            peer_store,
            peer_config,
        )
        .expect("construct the pinned peer reconnect")
        .run(None, &mut peer_approval),
    );
    let owner_reconnect = owner_reconnect.expect("owner authenticates the reconnect");
    let peer_reconnect = peer_reconnect.expect("peer authenticates the reconnect");
    assert_eq!(owner_reconnect.mode, PairingMode::Reconnect);
    assert_eq!(peer_reconnect.mode, PairingMode::Reconnect);
    assert!(owner_reconnect.sas.is_none());
    assert!(peer_reconnect.sas.is_none());
    assert_eq!(owner_approval.calls, 0);
    assert_eq!(peer_approval.calls, 0);

    let owner_reconnected = start_session(
        owner_directory.clone(),
        owner_reconnect.session,
        owner_id,
        peer_public,
    )
    .expect("start the new owner session after authenticated reconnect");
    let new_generation = owner_reconnected.generation();
    assert_ne!(new_generation, old_generation);
    harness
        .runtime
        .watch_session(peer_id, new_generation, &owner_reconnected);
    harness.runtime.sessions.insert(
        peer_id,
        OwnedSession {
            handle: owner_reconnected,
        },
    );
    let mut peer_reconnected = start_session(
        peer_directory,
        peer_reconnect.session,
        peer_id,
        owner_public,
    )
    .expect("start the peer session after authenticated reconnect");
    wait_until_connected(
        &harness
            .runtime
            .sessions
            .get(&peer_id)
            .expect("owner retains the new session")
            .handle,
    )
    .await;
    wait_until_connected(&peer_reconnected).await;

    assert!(matches!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    ));
    assert_eq!(
        harness
            .runtime
            .sessions
            .get(&peer_id)
            .map(|session| session.handle.generation()),
        Some(new_generation),
        "the process owner stores the new authenticated generation"
    );

    assert!(
        !harness
            .runtime
            .handle_command(old_terminal_command, &harness.snapshots, &harness.policy)
            .await,
        "a stale terminal event does not stop the owner loop"
    );
    assert_eq!(
        harness
            .runtime
            .sessions
            .get(&peer_id)
            .map(|session| session.handle.generation()),
        Some(new_generation),
        "the old generation cannot remove the new session from the owner"
    );
    assert!(matches!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    ));
    assert!(
        harness
            .policy
            .lock()
            .expect("owner accept policy lock")
            .active
            .contains(&peer_id),
        "the active peer stays available to the new authenticated session"
    );

    let owner_trust_after = SqliteTrustStore::open_existing(paired._owner_profile.path(), owner_id)
        .expect("reopen owner trust after reconnect");
    let peer_trust_after = SqliteTrustStore::open_existing(paired._peer_profile.path(), peer_id)
        .expect("reopen peer trust after reconnect");
    assert_eq!(
        owner_trust_after
            .lookup_trusted_peer(&peer_id)
            .expect("owner's pin remains readable after reconnect"),
        peer_public
    );
    assert_eq!(
        peer_trust_after
            .lookup_trusted_peer(&owner_id)
            .expect("peer's pin remains readable after reconnect"),
        owner_public
    );

    harness
        .runtime
        .sessions
        .get(&peer_id)
        .expect("owner retains the reconnected session")
        .handle
        .disconnect(SessionCloseReason::UserRequested)
        .expect("close the reconnected owner session after assertions");
    harness
        .runtime
        .sessions
        .get_mut(&peer_id)
        .expect("owner retains the reconnected session during cleanup")
        .handle
        .wait_closed()
        .await;
    peer_reconnected.wait_closed().await;
}
