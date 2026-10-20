#![allow(
    clippy::expect_used,
    reason = "The in-memory authenticated-session fixture uses expect for setup and assertions."
)]

use async_trait::async_trait;
use kryptopass_devices::{
    ConnectionState, DeviceDirectory, SessionCloseReason, SessionEvent, SessionFailure,
    SessionHandle, TrustState, start_session,
};
use kryptopass_identity::{DeviceId, DeviceIdentity, SqliteTrustStore};
use kryptopass_pairing::{
    EstablishedSession, LocalApproval, LocalDecision, PairingConfig, PairingEngine,
    PairingInvitation, PairingRole, SasDisplay,
};
use kryptopass_protocol::Channel;
use kryptopass_transport::{
    ConnectionInfo, MemoryTransport, MemoryTransportConfig, PeerTransport, TransportError,
    TransportFrame,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::{sync::watch, time::Instant};

const EXPECTED_PING_INTERVAL: Duration = Duration::from_secs(3);
const EXPECTED_PING_TIMEOUT: Duration = Duration::from_secs(6);
const EXPECTED_HELLO_TIMEOUT: Duration = Duration::from_secs(30);

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
            "kryptopass-fast-recovery-{}-{stamp}-{nonce}",
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

struct Approve;

#[async_trait]
impl LocalApproval for Approve {
    async fn confirm_sas(&mut self, _sas: &SasDisplay) -> LocalDecision {
        LocalDecision::Approve
    }
}

#[derive(Clone)]
struct Counter(watch::Sender<u64>);

impl Counter {
    fn new() -> (Self, watch::Receiver<u64>) {
        let (sender, receiver) = watch::channel(0);
        (Self(sender), receiver)
    }

    fn current(&self) -> u64 {
        *self.0.borrow()
    }

    fn increment(&self) {
        self.0.send_modify(|value| *value += 1);
    }

    async fn wait_until_at_least(&self, target: u64) {
        let mut receiver = self.0.subscribe();
        wait_for_counter(&mut receiver, target).await;
    }
}

async fn wait_for_counter(receiver: &mut watch::Receiver<u64>, target: u64) {
    loop {
        if *receiver.borrow_and_update() >= target {
            return;
        }
        receiver
            .changed()
            .await
            .expect("test transport counter remains observable");
    }
}

#[derive(Clone)]
struct TransportControl {
    pause_reads: watch::Sender<bool>,
    pause_writes: watch::Sender<bool>,
    sent: Counter,
    received: Counter,
    observations: Counter,
    blocked_reads: Counter,
    blocked_writes: Counter,
}

impl TransportControl {
    fn pause_reads(&self, pause: bool) {
        self.pause_reads.send_replace(pause);
    }

    fn pause_writes(&self, pause: bool) {
        self.pause_writes.send_replace(pause);
    }
}

struct GateMemoryTransport {
    inner: MemoryTransport,
    pause_reads: watch::Receiver<bool>,
    pause_writes: watch::Receiver<bool>,
    control: TransportControl,
}

impl GateMemoryTransport {
    fn pair() -> (Self, Self, TransportControl, TransportControl) {
        let (owner_inner, peer_inner) = MemoryTransport::pair(MemoryTransportConfig {
            send_timeout: None,
            receive_timeout: None,
            ..MemoryTransportConfig::default()
        })
        .expect("create authenticated test transport");
        let (owner, owner_control) = Self::new(owner_inner);
        let (peer, peer_control) = Self::new(peer_inner);
        (owner, peer, owner_control, peer_control)
    }

    fn new(inner: MemoryTransport) -> (Self, TransportControl) {
        let (pause_reads, read_receiver) = watch::channel(false);
        let (pause_writes, write_receiver) = watch::channel(false);
        let (sent, _) = Counter::new();
        let (received, _) = Counter::new();
        let (observations, _) = Counter::new();
        let (blocked_reads, _) = Counter::new();
        let (blocked_writes, _) = Counter::new();
        let control = TransportControl {
            pause_reads,
            pause_writes,
            sent,
            received,
            observations,
            blocked_reads,
            blocked_writes,
        };
        (
            Self {
                inner,
                pause_reads: read_receiver,
                pause_writes: write_receiver,
                control: control.clone(),
            },
            control,
        )
    }

    async fn wait_for_write_gate(&mut self) -> Result<(), TransportError> {
        loop {
            if !*self.pause_writes.borrow_and_update() {
                return Ok(());
            }
            self.control.blocked_writes.increment();
            self.pause_writes
                .changed()
                .await
                .map_err(|_| TransportError::Closed)?;
        }
    }
}

#[async_trait]
impl PeerTransport for GateMemoryTransport {
    async fn connection_info(&mut self) -> ConnectionInfo {
        self.control.observations.increment();
        self.inner.connection_info().await
    }

    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        self.wait_for_write_gate().await?;
        self.inner.send(channel, data).await?;
        self.control.sent.increment();
        Ok(())
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        loop {
            if *self.pause_reads.borrow_and_update() {
                self.control.blocked_reads.increment();
                self.pause_reads
                    .changed()
                    .await
                    .map_err(|_| TransportError::Closed)?;
                continue;
            }
            tokio::select! {
                biased;
                changed = self.pause_reads.changed() => {
                    changed.map_err(|_| TransportError::Closed)?;
                }
                result = self.inner.receive() => {
                    let frame = result?;
                    self.control.received.increment();
                    return Ok(frame);
                }
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

struct PairedMemorySessions {
    _owner_profile: TestProfile,
    _peer_profile: TestProfile,
    owner_identity: DeviceIdentity,
    peer_identity: DeviceIdentity,
    owner_session: EstablishedSession<GateMemoryTransport>,
    peer_session: EstablishedSession<GateMemoryTransport>,
    owner_transport: TransportControl,
    peer_transport: TransportControl,
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
    let (owner_transport, peer_transport, owner_control, peer_control) =
        GateMemoryTransport::pair();
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

    let owner_pairing = owner_pairing.expect("owner Noise pairing authenticates in memory");
    let peer_pairing = peer_pairing.expect("peer Noise pairing authenticates in memory");
    PairedMemorySessions {
        _owner_profile: owner_profile,
        _peer_profile: peer_profile,
        owner_identity,
        peer_identity,
        owner_session: owner_pairing.session,
        peer_session: peer_pairing.session,
        owner_transport: owner_control,
        peer_transport: peer_control,
    }
}

async fn wait_for_lifecycle<F>(handle: &SessionHandle, predicate: F) -> SessionEvent
where
    F: Fn(SessionEvent) -> bool,
{
    let mut lifecycle = handle.subscribe_lifecycle();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(event) = *lifecycle.borrow_and_update()
                && predicate(event)
            {
                return event;
            }
            lifecycle
                .changed()
                .await
                .expect("session lifecycle remains observable until it closes");
        }
    })
    .await
    .expect("expected lifecycle event is published")
}

async fn wait_until_connected(handle: &SessionHandle) {
    wait_for_lifecycle(handle, |event| {
        matches!(event, SessionEvent::Connected { .. })
    })
    .await;
}

async fn settle_session_tasks() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

fn assert_connected(directory: &DeviceDirectory, peer_id: DeviceId) {
    assert!(matches!(
        directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connected)
    ));
}

fn assert_trust_unchanged(directory: &DeviceDirectory, peer_id: DeviceId) {
    let snapshot = directory.snapshot();
    let device = snapshot
        .get(&peer_id)
        .expect("authenticated peer remains present in trusted directory");
    assert_eq!(device.trust, TrustState::Trusted);
    assert_eq!(snapshot.devices.len(), 1);
}

async fn start_connected_pair(paired: PairedMemorySessions) -> ConnectedPair {
    let PairedMemorySessions {
        owner_identity,
        peer_identity,
        owner_session,
        peer_session,
        owner_transport,
        peer_transport,
        ..
    } = paired;
    let owner_device_id = owner_identity.device_id();
    let peer_device_id = peer_identity.device_id();
    let owner_public = owner_identity.public_identity();
    let peer_public = peer_identity.public_identity();
    let owner_directory = DeviceDirectory::new(owner_device_id);
    owner_directory
        .replace_trusted(vec![peer_public])
        .expect("trust only the ephemeral peer identity");
    let peer_directory = DeviceDirectory::new(peer_device_id);
    peer_directory
        .replace_trusted(vec![owner_public])
        .expect("trust only the ephemeral owner identity");
    let owner_session = start_session(
        owner_directory.clone(),
        owner_session,
        owner_device_id,
        peer_public,
    )
    .expect("start owner-side authenticated session");
    let peer_session = start_session(
        peer_directory.clone(),
        peer_session,
        peer_device_id,
        owner_public,
    )
    .expect("start peer-side authenticated session");
    wait_until_connected(&owner_session).await;
    wait_until_connected(&peer_session).await;
    owner_transport.observations.wait_until_at_least(1).await;
    peer_transport.observations.wait_until_at_least(1).await;
    ConnectedPair {
        owner_directory,
        owner_session,
        peer_session,
        owner_transport,
        peer_transport,
    }
}

struct ConnectedPair {
    owner_directory: DeviceDirectory,
    owner_session: SessionHandle,
    peer_session: SessionHandle,
    owner_transport: TransportControl,
    peer_transport: TransportControl,
}

#[tokio::test(start_paused = true)]
async fn missed_authenticated_peer_pong_fails_after_one_short_heartbeat_window() {
    let paired = pair_two_ephemeral_devices().await;
    let peer_id = paired.peer_identity.device_id();
    let mut connected = start_connected_pair(paired).await;
    assert_connected(&connected.owner_directory, peer_id);

    let owner_sent_before_ping = connected.owner_transport.sent.current();
    let owner_elapsed_start = Instant::now();
    connected.peer_transport.pause_reads(true);
    connected.peer_transport.pause_writes(true);
    connected
        .peer_transport
        .blocked_reads
        .wait_until_at_least(1)
        .await;

    tokio::time::advance(EXPECTED_PING_INTERVAL - Duration::from_millis(100)).await;
    settle_session_tasks().await;
    assert_eq!(
        connected.owner_transport.sent.current(),
        owner_sent_before_ping
    );
    assert_connected(&connected.owner_directory, peer_id);

    tokio::time::advance(Duration::from_millis(100)).await;
    connected
        .owner_transport
        .sent
        .wait_until_at_least(owner_sent_before_ping + 1)
        .await;
    connected
        .peer_transport
        .blocked_writes
        .wait_until_at_least(1)
        .await;
    assert_connected(&connected.owner_directory, peer_id);

    tokio::time::advance(EXPECTED_PING_TIMEOUT - Duration::from_millis(100)).await;
    settle_session_tasks().await;
    assert_connected(&connected.owner_directory, peer_id);

    tokio::time::advance(Duration::from_millis(100)).await;
    let event = wait_for_lifecycle(&connected.owner_session, |event| {
        matches!(
            event,
            SessionEvent::Failed {
                failure: SessionFailure::PingTimeout,
                ..
            }
        )
    })
    .await;
    connected.owner_session.wait_closed().await;
    assert!(matches!(
        event,
        SessionEvent::Failed {
            failure: SessionFailure::PingTimeout,
            ..
        }
    ));
    assert_eq!(
        Instant::now().duration_since(owner_elapsed_start),
        EXPECTED_PING_INTERVAL + EXPECTED_PING_TIMEOUT,
        "a lost pong is detected at the 3s ping plus 6s response deadline"
    );
    assert_eq!(
        connected.owner_transport.sent.current(),
        owner_sent_before_ping + 1,
        "the failed heartbeat sends no protocol-error SessionClose to the peer"
    );

    connected.peer_transport.pause_writes(false);
    connected.peer_transport.pause_reads(false);
    let peer_event = wait_for_lifecycle(&connected.peer_session, |event| {
        matches!(
            event,
            SessionEvent::Failed { .. } | SessionEvent::Disconnected { .. }
        )
    })
    .await;
    connected.peer_session.wait_closed().await;
    assert!(
        matches!(
            peer_event,
            SessionEvent::Failed {
                failure: SessionFailure::Transport,
                ..
            }
        ),
        "the peer observes carrier closure rather than a protocol-error close"
    );
    assert_trust_unchanged(&connected.owner_directory, peer_id);
}

#[tokio::test(start_paused = true)]
async fn delayed_valid_pong_within_deadline_keeps_session_connected() {
    let paired = pair_two_ephemeral_devices().await;
    let peer_id = paired.peer_identity.device_id();
    let mut connected = start_connected_pair(paired).await;
    let owner_received_before_ping = connected.owner_transport.received.current();
    let owner_sent_before_ping = connected.owner_transport.sent.current();
    let peer_sent_before_ping = connected.peer_transport.sent.current();
    assert_connected(&connected.owner_directory, peer_id);

    connected.peer_transport.pause_reads(true);
    connected
        .peer_transport
        .blocked_reads
        .wait_until_at_least(1)
        .await;
    tokio::time::advance(EXPECTED_PING_INTERVAL).await;
    connected
        .owner_transport
        .sent
        .wait_until_at_least(owner_sent_before_ping + 1)
        .await;
    connected
        .peer_transport
        .sent
        .wait_until_at_least(peer_sent_before_ping + 1)
        .await;
    settle_session_tasks().await;

    tokio::time::advance(Duration::from_secs(5)).await;
    assert_connected(&connected.owner_directory, peer_id);
    connected.peer_transport.pause_reads(false);
    connected
        .peer_transport
        .sent
        .wait_until_at_least(peer_sent_before_ping + 2)
        .await;
    connected
        .owner_transport
        .received
        .wait_until_at_least(owner_received_before_ping + 2)
        .await;
    settle_session_tasks().await;
    assert_connected(&connected.owner_directory, peer_id);

    tokio::time::advance(Duration::from_secs(1)).await;
    settle_session_tasks().await;
    assert_connected(&connected.owner_directory, peer_id);
    assert_trust_unchanged(&connected.owner_directory, peer_id);

    connected
        .owner_session
        .disconnect(SessionCloseReason::UserRequested)
        .expect("close the healthy test session after verifying delayed pong");
    let _ = wait_for_lifecycle(&connected.owner_session, |event| {
        matches!(event, SessionEvent::Disconnected { .. })
    })
    .await;
    connected.owner_session.wait_closed().await;
    connected.peer_session.wait_closed().await;
}

#[tokio::test(start_paused = true)]
async fn initial_hello_keeps_its_longer_deadline_than_the_pong_timeout() {
    let paired = pair_two_ephemeral_devices().await;
    let peer_id = paired.peer_identity.device_id();
    let peer_public = paired.peer_identity.public_identity();
    let owner_directory = DeviceDirectory::new(paired.owner_identity.device_id());
    owner_directory
        .replace_trusted(vec![peer_public])
        .expect("trust only the ephemeral peer identity");
    let owner_sent_before_hello = paired.owner_transport.sent.current();
    let hello_started = Instant::now();
    let mut owner_session = start_session(
        owner_directory.clone(),
        paired.owner_session,
        paired.owner_identity.device_id(),
        peer_public,
    )
    .expect("start owner session while authenticated peer is slow to send Hello");
    paired
        .owner_transport
        .sent
        .wait_until_at_least(owner_sent_before_hello + 1)
        .await;

    tokio::time::advance(EXPECTED_PING_TIMEOUT).await;
    settle_session_tasks().await;
    let lifecycle = owner_session.subscribe_lifecycle();
    assert!(lifecycle.borrow().is_none());
    assert!(matches!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connecting)
    ));

    tokio::time::advance(
        EXPECTED_HELLO_TIMEOUT - EXPECTED_PING_TIMEOUT - Duration::from_millis(100),
    )
    .await;
    settle_session_tasks().await;
    assert!(lifecycle.borrow().is_none());
    assert!(matches!(
        owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Connecting)
    ));

    tokio::time::advance(Duration::from_millis(100)).await;
    let event = wait_for_lifecycle(&owner_session, |event| {
        matches!(
            event,
            SessionEvent::Failed {
                failure: SessionFailure::HelloTimeout,
                ..
            }
        )
    })
    .await;
    owner_session.wait_closed().await;
    assert!(matches!(
        event,
        SessionEvent::Failed {
            failure: SessionFailure::HelloTimeout,
            ..
        }
    ));
    assert_eq!(
        Instant::now().duration_since(hello_started),
        EXPECTED_HELLO_TIMEOUT
    );
    assert_trust_unchanged(&owner_directory, peer_id);
}

#[tokio::test(start_paused = true)]
async fn user_disconnect_closes_session_without_removing_trust_or_reconnecting() {
    let paired = pair_two_ephemeral_devices().await;
    let peer_id = paired.peer_identity.device_id();
    let mut connected = start_connected_pair(paired).await;
    assert_connected(&connected.owner_directory, peer_id);

    connected
        .owner_session
        .disconnect(SessionCloseReason::UserRequested)
        .expect("queue explicit human disconnect");
    let event = wait_for_lifecycle(&connected.owner_session, |event| {
        matches!(
            event,
            SessionEvent::Disconnected {
                reason: SessionCloseReason::UserRequested,
                ..
            }
        )
    })
    .await;
    connected.owner_session.wait_closed().await;
    assert!(matches!(
        event,
        SessionEvent::Disconnected {
            reason: SessionCloseReason::UserRequested,
            ..
        }
    ));
    assert_trust_unchanged(&connected.owner_directory, peer_id);
    assert!(matches!(
        connected
            .owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Disconnected)
    ));

    tokio::time::advance(Duration::from_secs(60)).await;
    settle_session_tasks().await;
    assert_trust_unchanged(&connected.owner_directory, peer_id);
    assert!(matches!(
        connected
            .owner_directory
            .snapshot()
            .get(&peer_id)
            .map(|device| device.connection),
        Some(ConnectionState::Disconnected)
    ));
    assert!(matches!(
        connected
            .owner_session
            .subscribe_lifecycle()
            .borrow()
            .as_ref(),
        Some(SessionEvent::Disconnected {
            reason: SessionCloseReason::UserRequested,
            ..
        })
    ));
    connected.peer_session.wait_closed().await;
}
