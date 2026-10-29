//! Bounded Sync rounds over an already authenticated Noise session.
//!
//! The engine works on a [`SyncStore`] and a trust source supplied by the
//! caller. It is event driven: callers pass one decoded sync message at a
//! time and pull at most one outbound message per loop, leaving the
//! authenticated session owner responsible for keepalives, closure,
//! revocation and transport I/O. What each side offers is routed by the keys
//! it syncs with the peer; see `docs/sync/contrato.md`.

use crate::model::{
    Digest, Fingerprint, Head, Id, RecordKind, SYNC_MANIFEST_PAGE_HEADS, SYNC_MAX_BATCH_BYTES,
    SYNC_MAX_OBJECTS, SYNC_MAX_OBJECTS_PER_BATCH, SYNC_MAX_RECIPIENTS, SyncError, TrustSource,
    fresh_id, is_zero_id,
};
use crate::store::{SyncStore, VersionMeta, route, verify_batch};
use crate::wire::{Batch, decode_batch, encode_batch};
use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_protocol::{MessagePayload, proto};
use prost::Message as _;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::fmt;
use std::time::{Duration, Instant};
use tracing::{debug, info, trace, warn};

/// Maximum objects advertised by one endpoint.
pub const SESSION_MAX_KNOWN_OBJECTS: usize = SYNC_MAX_OBJECTS;
/// Maximum requested heads in one receive window.
pub const SESSION_MAX_REQUESTED_HEADS: usize = 256;
/// Maximum opaque fragment payload.
pub const SESSION_CHUNK_BYTES: usize = 48 * 1024;
/// Idle request or transfer timeout.
pub const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Absolute maximum lifetime of one round.
pub const SESSION_ROUND_TIMEOUT: Duration = Duration::from_secs(300);
/// Minimum spacing between continuous rounds on one authenticated session.
pub const SESSION_ROUND_DEBOUNCE: Duration = Duration::from_secs(1);
/// Maximum number of continuous round starts in the rolling window.
pub const SESSION_MAX_ROUNDS_PER_WINDOW: usize = 128;
/// Rolling budget window for continuous round starts.
pub const SESSION_ROUND_START_WINDOW: Duration = Duration::from_secs(60);
/// Maximum interval between checks for writes committed through another store handle.
pub const SESSION_LOCAL_CHANGE_POLL: Duration = Duration::from_millis(500);

const MAX_PENDING_CONTROL: usize = 4;
const MAX_TRACKED_TRANSFERS: usize = SESSION_MAX_KNOWN_OBJECTS;
/// Room for one object's fixed fields when estimating a batch's size.
const OBJECT_OVERHEAD: usize = 4096;

/// A payload-free failure from the authenticated-session Sync engine.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SyncSessionError {
    /// The supplied local identity, peer identity or keys are invalid.
    InvalidBinding,
    /// The authenticated peer is no longer exactly present in the trust source.
    TrustRevoked,
    /// A sync message arrived in an invalid order or failed a protocol check.
    Protocol,
    /// The round exceeded its idle or absolute timeout.
    Timeout,
    /// A bounded queue, object count or message size limit was exceeded.
    Capacity,
    /// A store read or transactional write failed.
    Storage,
    /// An object author is absent from the explicit trust source.
    UnknownAuthor,
    /// The operating system random source failed.
    Randomness,
}

impl fmt::Debug for SyncSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SyncSessionError([redacted])")
    }
}

impl fmt::Display for SyncSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidBinding => "sync session binding is invalid",
            Self::TrustRevoked => "sync peer is no longer trusted",
            Self::Protocol => "sync session protocol failed",
            Self::Timeout => "sync session timed out",
            Self::Capacity => "sync session limit exceeded",
            Self::Storage => "sync store operation failed",
            Self::UnknownAuthor => "sync object author is not trusted",
            Self::Randomness => "sync random source failed",
        })
    }
}

impl std::error::Error for SyncSessionError {}

fn map_sync_error(error: SyncError) -> SyncSessionError {
    match error {
        SyncError::Storage | SyncError::InvalidStore => SyncSessionError::Storage,
        SyncError::UnknownAuthor => SyncSessionError::UnknownAuthor,
        SyncError::Randomness => SyncSessionError::Randomness,
        SyncError::LimitExceeded | SyncError::Allocation => SyncSessionError::Capacity,
        _ => SyncSessionError::Protocol,
    }
}

/// One typed outbound message. Ciphertext fragments are redacted from Debug.
pub enum SyncOutbound {
    /// Announces this side and its keys after SessionHello negotiation.
    Hello(proto::SyncHello),
    /// Publishes the frozen local manifest for this round.
    Manifest(proto::SyncManifest),
    /// Requests missing heads from the peer's frozen snapshot.
    Request(proto::SyncRequest),
    /// Carries one bounded opaque batch fragment.
    Chunk(proto::SyncChunk),
    /// Commits one completed encoded batch.
    Commit(proto::SyncCommit),
    /// Acknowledges a batch only after the store commits it.
    Ack(proto::SyncAck),
}

impl fmt::Debug for SyncOutbound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Hello(_) => "SyncOutbound::Hello([redacted])",
            Self::Manifest(_) => "SyncOutbound::Manifest([redacted])",
            Self::Request(_) => "SyncOutbound::Request([redacted])",
            Self::Chunk(_) => "SyncOutbound::Chunk([redacted])",
            Self::Commit(_) => "SyncOutbound::Commit([redacted])",
            Self::Ack(_) => "SyncOutbound::Ack([redacted])",
        })
    }
}

impl SyncOutbound {
    /// Returns the message kind and payload for the existing Noise session owner.
    #[must_use]
    pub fn into_message(self) -> (proto::MessageType, MessagePayload) {
        match self {
            Self::Hello(payload) => (
                proto::MessageType::SyncHello,
                MessagePayload::SyncHello(payload),
            ),
            Self::Manifest(payload) => (
                proto::MessageType::SyncManifest,
                MessagePayload::SyncManifest(payload),
            ),
            Self::Request(payload) => (
                proto::MessageType::SyncRequest,
                MessagePayload::SyncRequest(payload),
            ),
            Self::Chunk(payload) => (
                proto::MessageType::SyncChunk,
                MessagePayload::SyncChunk(payload),
            ),
            Self::Commit(payload) => (
                proto::MessageType::SyncCommit,
                MessagePayload::SyncCommit(payload),
            ),
            Self::Ack(payload) => (
                proto::MessageType::SyncAck,
                MessagePayload::SyncAck(payload),
            ),
        }
    }
}

/// Coarse in-memory round state for the authenticated peer projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncProgressState {
    /// SyncHello and frozen manifests are still being exchanged.
    Negotiating,
    /// The local endpoint is receiving a requested head window.
    Receiving,
    /// The local endpoint is serving a peer request.
    Serving,
    /// Both directed receive and serve phases have completed.
    Complete,
    /// The round ended on an error or cancellation.
    Failed,
}

/// Payload-free volatile progress for one authenticated peer and one round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncProgress {
    /// Current coarse round state.
    pub state: SyncProgressState,
    /// Objects received from the peer and committed in this round.
    pub received_objects: usize,
    /// Objects the peer acknowledged in this round.
    pub acknowledged_objects: usize,
    /// Number of heads in the local frozen manifest.
    pub local_heads: usize,
    /// Number of heads in the remote frozen manifest, once received.
    pub remote_heads: usize,
}

struct FrozenSnapshot {
    snapshot_id: Id,
    /// Everything stored, routed or not, to notice writes made meanwhile.
    stored_count: usize,
    /// Objects offered to this peer.
    object_count: usize,
    manifest: Vec<Head>,
    heads: BTreeMap<(Id, Option<Id>), Head>,
    versions: BTreeMap<Id, VersionMeta>,
    order: Vec<Id>,
}

/// One object to send: a deletion mark by record, or a version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PlanItem {
    Deletion(Id),
    Version(Id),
}

struct InboundTransfer {
    transfer_id: Id,
    digest: Digest,
    total_size: usize,
    chunk_count: usize,
    next_chunk: usize,
    encoded: Vec<u8>,
}

struct OutboundTransfer {
    transfer_id: Id,
    digest: Digest,
    encoded: Vec<u8>,
    chunk_count: usize,
    next_chunk: usize,
    commit_sent: bool,
    items: Vec<PlanItem>,
    /// Records in the batch, shared with the peer once it acknowledges.
    records: Vec<Id>,
    author_device_ids: Vec<DeviceId>,
}

/// An explicitly bound, event-driven Sync session that runs bounded rounds.
///
/// The caller must create this only after bilateral `SessionHello` validation
/// and retain one instance for the lifetime of that exact authenticated peer.
pub struct SyncSession {
    store: Box<dyn SyncStore + Send>,
    trust: Box<dyn TrustSource + Send>,
    local_identity: PublicIdentity,
    expected_peer: PublicIdentity,
    /// Keys this side syncs with the peer; they route what it is offered.
    keys: Vec<Fingerprint>,
    /// Keys the peer announced it syncs with this side.
    peer_keys: Vec<Fingerprint>,
    started_at: Instant,
    last_activity: Instant,
    continuous_rounds: bool,
    local_hello_sent: bool,
    round_requested: bool,
    round_starts: VecDeque<Instant>,
    round_number: u64,
    baseline_object_count: Option<usize>,
    round_inserted_count: usize,
    remote_hello_received: bool,
    remote_known_object_count: usize,
    remote_manifest_received: bool,
    /// Index of the next streamed manifest page expected from the peer.
    remote_manifest_next_page: u32,
    local_is_smaller: bool,
    local_snapshot: Option<FrozenSnapshot>,
    remote_snapshot_id: Option<Id>,
    remote_manifest: Vec<Head>,
    remote_heads: BTreeMap<(Id, Option<Id>), Head>,
    control_queue: VecDeque<SyncOutbound>,
    receive_started: bool,
    receive_done: bool,
    active_window: Vec<Head>,
    serve_started: bool,
    serve_done: bool,
    request_record_ids: BTreeSet<Id>,
    outbound_plan: VecDeque<PlanItem>,
    outbound_transfer: Option<OutboundTransfer>,
    awaiting_ack: Option<Id>,
    acknowledged_objects: HashSet<PlanItem>,
    inbound_transfer: Option<InboundTransfer>,
    used_transfer_ids: HashSet<Id>,
    issued_transfer_ids: HashSet<Id>,
    inbound_objects: HashSet<(Id, Option<Id>)>,
    received_object_count: usize,
    acknowledged_object_count: usize,
    failed: bool,
    failure: Option<SyncSessionError>,
    complete: bool,
}

impl fmt::Debug for SyncSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncSession")
            .field("state", &self.progress().state)
            .field("counts", &"[redacted]")
            .finish()
    }
}

impl SyncSession {
    /// Starts a round on `store`, offering the peer what is routed to it by
    /// `keys`: the OpenPGP fingerprints this side syncs with that peer.
    pub fn new(
        store: Box<dyn SyncStore + Send>,
        trust: Box<dyn TrustSource + Send>,
        local_device_id: DeviceId,
        expected_peer: PublicIdentity,
        keys: Vec<Fingerprint>,
        now: Instant,
    ) -> Result<Self, SyncSessionError> {
        match Self::new_inner(store, trust, local_device_id, expected_peer, keys, now) {
            Ok(session) => {
                debug!(
                    local_serves_first = session.local_is_smaller,
                    "Sync session created"
                );
                Ok(session)
            }
            Err(error) => {
                warn!(error = %error, "Sync session could not start");
                Err(error)
            }
        }
    }
    fn new_inner(
        store: Box<dyn SyncStore + Send>,
        trust: Box<dyn TrustSource + Send>,
        local_device_id: DeviceId,
        expected_peer: PublicIdentity,
        mut keys: Vec<Fingerprint>,
        now: Instant,
    ) -> Result<Self, SyncSessionError> {
        keys.sort_unstable();
        keys.dedup();
        if local_device_id.as_bytes().iter().all(|byte| *byte == 0)
            || expected_peer.device_id == local_device_id
            || keys.is_empty()
            || keys.len() > SYNC_MAX_RECIPIENTS
            || expected_peer.validate().is_err()
        {
            return Err(SyncSessionError::InvalidBinding);
        }
        let local = trust
            .lookup(local_device_id)
            .map_err(map_sync_error)?
            .ok_or(SyncSessionError::TrustRevoked)?;
        let peer = trust
            .lookup(expected_peer.device_id)
            .map_err(map_sync_error)?
            .ok_or(SyncSessionError::TrustRevoked)?;
        if local.device_id != local_device_id || local.validate().is_err() || peer != expected_peer
        {
            return Err(SyncSessionError::TrustRevoked);
        }
        let mut session = Self {
            store,
            trust,
            local_identity: local,
            expected_peer,
            keys,
            peer_keys: Vec::new(),
            started_at: now,
            last_activity: now,
            continuous_rounds: false,
            local_hello_sent: false,
            round_requested: false,
            round_starts: VecDeque::from([now]),
            round_number: 1,
            baseline_object_count: None,
            round_inserted_count: 0,
            remote_hello_received: false,
            remote_known_object_count: 0,
            remote_manifest_received: false,
            remote_manifest_next_page: 0,
            local_is_smaller: local_device_id.as_bytes() < expected_peer.device_id.as_bytes(),
            local_snapshot: None,
            remote_snapshot_id: None,
            remote_manifest: Vec::new(),
            remote_heads: BTreeMap::new(),
            control_queue: VecDeque::with_capacity(MAX_PENDING_CONTROL),
            receive_started: false,
            receive_done: false,
            active_window: Vec::new(),
            serve_started: false,
            serve_done: false,
            request_record_ids: BTreeSet::new(),
            outbound_plan: VecDeque::new(),
            outbound_transfer: None,
            awaiting_ack: None,
            acknowledged_objects: HashSet::new(),
            inbound_transfer: None,
            used_transfer_ids: HashSet::new(),
            issued_transfer_ids: HashSet::new(),
            inbound_objects: HashSet::new(),
            received_object_count: 0,
            acknowledged_object_count: 0,
            failed: false,
            failure: None,
            complete: false,
        };
        session.queue_snapshot_hello()?;
        Ok(session)
    }

    /// Enables continuous rounds after bilateral authenticated `SessionHello`
    /// negotiation. Call this only before the first SyncHello is exchanged.
    /// Existing construction remains single-round by default.
    pub fn enable_continuous_rounds(&mut self) -> Result<(), SyncSessionError> {
        if self.failed
            || self.complete
            || self.remote_hello_received
            || self.local_hello_sent
            || self.round_starts.len() != 1
        {
            warn!(reason = "invalid_state", "Sync continuous rounds rejected");
            return Err(SyncSessionError::Protocol);
        }
        self.continuous_rounds = true;
        debug!("Sync continuous rounds enabled");
        Ok(())
    }

    /// Returns whether this session negotiated continuous rounds.
    #[must_use]
    pub const fn is_continuous(&self) -> bool {
        self.continuous_rounds
    }

    /// Coalesces an explicit request for one more round.
    /// The caller may check [`Self::has_uncovered_local_changes`] first when
    /// the request comes from an automatic local-change watcher. The request is
    /// accepted while a round is active and starts only after that round completes.
    pub fn request_round(&mut self) -> bool {
        if !self.continuous_rounds || self.failed {
            return false;
        }
        self.round_requested = true;
        debug!(round = self.round_number, "Sync next round requested");
        true
    }

    /// Returns whether objects committed after this round's frozen snapshot
    /// remain uncovered after accounting for inserts received in this round.
    pub fn has_uncovered_local_changes(&self) -> Result<bool, SyncSessionError> {
        let result = self.has_uncovered_local_changes_inner();
        if let Err(error) = &result {
            warn!(error = %error, "Sync local change check failed");
        }
        result
    }

    /// Pulls at most one outbound frame. Chunks and batches are generated lazily.
    pub fn poll_output(&mut self, now: Instant) -> Result<Option<SyncOutbound>, SyncSessionError> {
        match self.poll_output_inner(now) {
            Ok(output) => Ok(output),
            Err(error) => self.fail(error),
        }
    }

    fn poll_output_inner(
        &mut self,
        now: Instant,
    ) -> Result<Option<SyncOutbound>, SyncSessionError> {
        if self.failed {
            return Ok(None);
        }
        if self.complete {
            if self.continuous_rounds {
                self.try_start_requested_round(now)?;
            }
            if self.complete {
                return Ok(None);
            }
        }
        self.check_time(now)?;
        if let Some(message) = self.control_queue.pop_front() {
            if matches!(&message, SyncOutbound::Hello(_)) {
                self.local_hello_sent = true;
            }
            match message {
                SyncOutbound::Request(ref request) if request.wanted.is_empty() => {
                    self.receive_done = true;
                    self.active_window.clear();
                    self.receive_started = true;
                    if !self.local_is_smaller {
                        self.serve_started = true;
                    }
                }
                SyncOutbound::Request(_) => self.receive_started = true,
                _ => {}
            }
            self.last_activity = now;
            self.refresh_completion();
            return Ok(Some(message));
        }

        if let Some(transfer) = self.outbound_transfer.as_ref() {
            self.ensure_authors_trusted(&transfer.author_device_ids)?;
        }
        let Some(transfer) = self.outbound_transfer.as_mut() else {
            return Ok(None);
        };
        if transfer.next_chunk < transfer.chunk_count {
            let index = transfer.next_chunk;
            let start = index
                .checked_mul(SESSION_CHUNK_BYTES)
                .ok_or(SyncSessionError::Capacity)?;
            let end = transfer
                .encoded
                .len()
                .min(start.saturating_add(SESSION_CHUNK_BYTES));
            if start >= end {
                return self.fail(SyncSessionError::Protocol);
            }
            let payload = proto::SyncChunk {
                transfer_id: transfer.transfer_id.to_vec(),
                chunk_index: u32::try_from(index).map_err(|_| SyncSessionError::Capacity)?,
                chunk_count: u32::try_from(transfer.chunk_count)
                    .map_err(|_| SyncSessionError::Capacity)?,
                total_size: transfer.encoded.len() as u64,
                sha256_digest: transfer.digest.to_vec(),
                data: transfer.encoded[start..end].to_vec(),
            };
            transfer.next_chunk += 1;
            trace!(
                index,
                chunks = transfer.chunk_count,
                bytes = payload.data.len(),
                "Sync outbound chunk"
            );
            self.last_activity = now;
            return Ok(Some(SyncOutbound::Chunk(payload)));
        }
        if !transfer.commit_sent {
            let payload = proto::SyncCommit {
                transfer_id: transfer.transfer_id.to_vec(),
                sha256_digest: transfer.digest.to_vec(),
            };
            transfer.commit_sent = true;
            self.awaiting_ack = Some(transfer.transfer_id);
            debug!(
                chunks = transfer.chunk_count,
                bytes = transfer.encoded.len(),
                "Sync outbound transfer complete"
            );
            self.last_activity = now;
            return Ok(Some(SyncOutbound::Commit(payload)));
        }
        Ok(None)
    }

    /// Handles one decoded message received on the already authenticated session.
    pub fn handle_message(
        &mut self,
        message_type: proto::MessageType,
        payload: MessagePayload,
        now: Instant,
    ) -> Result<(), SyncSessionError> {
        self.check_time(now)?;
        if self.complete {
            if self.continuous_rounds && message_type == proto::MessageType::SyncHello {
                self.begin_peer_requested_round(now)?;
            } else {
                return self.fail(SyncSessionError::Protocol);
            }
        } else if self.failed {
            return self.fail(self.failure.unwrap_or(SyncSessionError::Protocol));
        }
        let result = match (message_type, payload) {
            (proto::MessageType::SyncHello, MessagePayload::SyncHello(hello)) => {
                self.on_hello(hello)
            }
            (proto::MessageType::SyncManifest, MessagePayload::SyncManifest(manifest)) => {
                self.on_manifest(manifest)
            }
            (proto::MessageType::SyncRequest, MessagePayload::SyncRequest(request)) => {
                self.on_request(request)
            }
            (proto::MessageType::SyncChunk, MessagePayload::SyncChunk(chunk)) => {
                self.on_chunk(chunk)
            }
            (proto::MessageType::SyncCommit, MessagePayload::SyncCommit(commit)) => {
                self.on_commit(commit)
            }
            (proto::MessageType::SyncAck, MessagePayload::SyncAck(ack)) => self.on_ack(ack),
            _ => Err(SyncSessionError::Protocol),
        };
        match result {
            Ok(()) => {
                self.last_activity = now;
                self.refresh_completion();
                Ok(())
            }
            Err(error) => self.fail(error),
        }
    }

    /// Checks caller-supplied monotonic timeouts between session-owner events.
    pub fn check_time(&mut self, now: Instant) -> Result<(), SyncSessionError> {
        if self.failed {
            return Err(self.failure.unwrap_or(SyncSessionError::Protocol));
        }
        if self.complete {
            return Ok(());
        }
        if now.saturating_duration_since(self.started_at) >= SESSION_ROUND_TIMEOUT
            || now.saturating_duration_since(self.last_activity) >= SESSION_IDLE_TIMEOUT
        {
            return self.fail(SyncSessionError::Timeout);
        }
        if let Err(error) = self.ensure_session_trusted() {
            return self.fail(error);
        }
        Ok(())
    }

    /// Returns the shorter remaining idle or absolute round deadline.
    #[must_use]
    pub fn timeout_after(&self, now: Instant) -> Duration {
        let idle =
            SESSION_IDLE_TIMEOUT.saturating_sub(now.saturating_duration_since(self.last_activity));
        let round =
            SESSION_ROUND_TIMEOUT.saturating_sub(now.saturating_duration_since(self.started_at));
        idle.min(round)
    }

    /// Returns fixed-code volatile progress without peer IDs or object contents.
    #[must_use]
    pub fn progress(&self) -> SyncProgress {
        let state = if self.failed {
            SyncProgressState::Failed
        } else if self.complete {
            SyncProgressState::Complete
        } else if !self.remote_hello_received || !self.remote_manifest_received {
            SyncProgressState::Negotiating
        } else if self.receive_started && !self.receive_done {
            SyncProgressState::Receiving
        } else if self.serve_started && !self.serve_done {
            SyncProgressState::Serving
        } else if self.receive_done {
            SyncProgressState::Receiving
        } else {
            SyncProgressState::Serving
        };
        SyncProgress {
            state,
            received_objects: self.received_object_count,
            acknowledged_objects: self.acknowledged_object_count,
            local_heads: self
                .local_snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.manifest.len()),
            remote_heads: self.remote_manifest.len(),
        }
    }

    /// Payload-free phase flags and counters for debug logging by the connection owner.
    #[must_use]
    pub fn diagnostics(&self, now: Instant) -> String {
        format!(
            "round={} state={:?} local_heads={} remote_heads={} manifest_pages={} \
             hello_sent={} hello_received={} manifest_received={} queued={} \
             receive_done={} serve_done={} awaiting_ack={} inbound_chunks={} \
             outbound_chunks={} received={} acknowledged={} pending_round={} idle_ms={} age_ms={}",
            self.round_number,
            self.progress().state,
            self.local_snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.manifest.len()),
            self.remote_manifest.len(),
            self.remote_manifest_next_page,
            self.local_hello_sent,
            self.remote_hello_received,
            self.remote_manifest_received,
            self.control_queue.len(),
            self.receive_done,
            self.serve_done,
            self.awaiting_ack.is_some(),
            self.inbound_transfer
                .as_ref()
                .map_or(0, |transfer| transfer.next_chunk),
            self.outbound_transfer
                .as_ref()
                .map_or(0, |transfer| transfer.next_chunk),
            self.received_object_count,
            self.acknowledged_object_count,
            self.round_requested,
            now.saturating_duration_since(self.last_activity)
                .as_millis(),
            now.saturating_duration_since(self.started_at).as_millis(),
        )
    }

    /// Indicates that both directed phases have ended successfully.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    /// The keys the peer announced it syncs with this side, once its hello
    /// arrived. A difference with this side's keys is for the UI to show.
    #[must_use]
    pub fn peer_keys(&self) -> &[Fingerprint] {
        &self.peer_keys
    }

    /// Cancels the round and discards every queued or partial transfer buffer.
    pub fn cancel(&mut self) {
        let was_failed = self.failed;
        self.failed = true;
        self.control_queue.clear();
        self.outbound_plan.clear();
        self.outbound_transfer = None;
        self.inbound_transfer = None;
        self.active_window.clear();
        if !was_failed {
            info!(round = self.round_number, "Sync session cancelled");
        }
    }
    fn on_hello(&mut self, hello: proto::SyncHello) -> Result<(), SyncSessionError> {
        if self.remote_hello_received {
            return Err(SyncSessionError::Protocol);
        }
        if hello.device_id.as_slice() != self.expected_peer.device_id.as_bytes()
            || hello.known_object_count > SESSION_MAX_KNOWN_OBJECTS as u64
            || hello.keys.is_empty()
            || hello.keys.len() > SYNC_MAX_RECIPIENTS
        {
            return Err(SyncSessionError::InvalidBinding);
        }
        let mut peer_keys = hello
            .keys
            .into_iter()
            .map(|key| Fingerprint::new(key).map_err(|_| SyncSessionError::Protocol))
            .collect::<Result<Vec<_>, _>>()?;
        peer_keys.sort_unstable();
        peer_keys.dedup();
        // Each side routes with its own keys; a difference is only reported.
        if peer_keys != self.keys {
            debug!("Sync peer syncs a different set of keys");
        }
        self.peer_keys = peer_keys;
        self.remote_hello_received = true;
        self.remote_known_object_count =
            usize::try_from(hello.known_object_count).map_err(|_| SyncSessionError::Capacity)?;
        let snapshot = self
            .local_snapshot
            .as_ref()
            .ok_or(SyncSessionError::Protocol)?;
        let snapshot_id = snapshot.snapshot_id;
        debug!(
            remote_objects = self.remote_known_object_count,
            local_objects = snapshot.object_count,
            local_serves_first = self.local_is_smaller,
            "Sync hello accepted"
        );
        // Stream the manifest in fixed-size pages so no message grows with the
        // store. An empty manifest still sends one empty final page.
        let mut pages = snapshot
            .manifest
            .chunks(SYNC_MANIFEST_PAGE_HEADS)
            .map(|heads| heads.iter().map(wire_head).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        if pages.is_empty() {
            pages.push(Vec::new());
        }
        let last_index = pages.len() - 1;
        for (index, heads) in pages.into_iter().enumerate() {
            let page = proto::SyncManifest {
                snapshot_id: snapshot_id.to_vec(),
                heads,
                page_index: u32::try_from(index).map_err(|_| SyncSessionError::Capacity)?,
                last_page: index == last_index,
            };
            self.queue_manifest_page(page)?;
        }
        Ok(())
    }
    fn queue_snapshot_hello(&mut self) -> Result<(), SyncSessionError> {
        // A write committed meanwhile belongs to the next round. This greeting
        // and its manifest describe the same frozen state, even if the peer is slow.
        let snapshot = self.freeze_local_snapshot()?;
        info!(
            round = self.round_number,
            objects = snapshot.object_count,
            heads = snapshot.manifest.len(),
            local_serves_first = self.local_is_smaller,
            "Sync round started"
        );
        self.baseline_object_count = Some(snapshot.stored_count);
        self.local_snapshot = Some(snapshot);
        self.queue_control(SyncOutbound::Hello(self.local_hello()?))
    }

    fn local_hello(&self) -> Result<proto::SyncHello, SyncSessionError> {
        let known_object_count = self
            .local_snapshot
            .as_ref()
            .ok_or(SyncSessionError::Protocol)?
            .object_count;
        Ok(proto::SyncHello {
            device_id: self.local_identity.device_id.as_bytes().to_vec(),
            known_object_count: known_object_count as u64,
            keys: self
                .keys
                .iter()
                .map(|key| key.as_bytes().to_vec())
                .collect(),
        })
    }

    fn begin_peer_requested_round(&mut self, now: Instant) -> Result<(), SyncSessionError> {
        if !self.continuous_rounds || !self.complete {
            return self.fail(SyncSessionError::Protocol);
        }
        self.trim_round_start_window(now);
        if self.round_starts.len() >= SESSION_MAX_ROUNDS_PER_WINDOW {
            return self.fail(SyncSessionError::Capacity);
        }
        self.ensure_session_trusted()?;
        debug!(
            round = self.round_number.saturating_add(1),
            trigger = "peer",
            "Sync starting next round"
        );
        self.reset_round_state(now);
        self.round_starts.push_back(now);
        self.queue_snapshot_hello()?;
        Ok(())
    }

    fn try_start_requested_round(&mut self, now: Instant) -> Result<bool, SyncSessionError> {
        if !self.round_requested || !self.complete {
            return Ok(false);
        }
        self.trim_round_start_window(now);
        if self.round_starts.len() >= SESSION_MAX_ROUNDS_PER_WINDOW
            || self
                .round_starts
                .back()
                .is_some_and(|last| now.saturating_duration_since(*last) < SESSION_ROUND_DEBOUNCE)
        {
            return Ok(false);
        }
        self.ensure_session_trusted()?;
        debug!(
            round = self.round_number.saturating_add(1),
            trigger = "local",
            "Sync starting next round"
        );
        self.reset_round_state(now);
        self.round_starts.push_back(now);
        self.queue_snapshot_hello()?;
        Ok(true)
    }

    fn trim_round_start_window(&mut self, now: Instant) {
        while self.round_starts.front().is_some_and(|started| {
            now.saturating_duration_since(*started) >= SESSION_ROUND_START_WINDOW
        }) {
            self.round_starts.pop_front();
        }
    }

    fn reset_round_state(&mut self, now: Instant) {
        self.round_number = self.round_number.saturating_add(1);
        self.started_at = now;
        self.last_activity = now;
        self.round_requested = false;
        self.baseline_object_count = None;
        self.round_inserted_count = 0;
        self.remote_hello_received = false;
        self.remote_known_object_count = 0;
        self.remote_manifest_received = false;
        self.remote_manifest_next_page = 0;
        self.local_snapshot = None;
        self.remote_snapshot_id = None;
        self.remote_manifest.clear();
        self.remote_heads.clear();
        self.control_queue.clear();
        self.receive_started = false;
        self.receive_done = false;
        self.active_window.clear();
        self.serve_started = false;
        self.serve_done = false;
        self.request_record_ids.clear();
        self.outbound_plan.clear();
        self.outbound_transfer = None;
        self.awaiting_ack = None;
        self.acknowledged_objects.clear();
        self.inbound_transfer = None;
        self.used_transfer_ids.clear();
        self.issued_transfer_ids.clear();
        self.inbound_objects.clear();
        self.received_object_count = 0;
        self.acknowledged_object_count = 0;
        self.complete = false;
        self.local_hello_sent = false;
    }

    fn has_uncovered_local_changes_inner(&self) -> Result<bool, SyncSessionError> {
        let Some(baseline) = self.baseline_object_count else {
            return Ok(false);
        };
        let covered = baseline
            .checked_add(self.round_inserted_count)
            .ok_or(SyncSessionError::Capacity)?;
        let current = self.store.object_count().map_err(map_sync_error)?;
        if current > SESSION_MAX_KNOWN_OBJECTS {
            return Err(SyncSessionError::Capacity);
        }
        if current < covered {
            return Err(SyncSessionError::Storage);
        }
        Ok(current > covered)
    }
    fn on_manifest(&mut self, manifest: proto::SyncManifest) -> Result<(), SyncSessionError> {
        if !self.remote_hello_received
            || self.remote_manifest_received
            || manifest.snapshot_id.len() != 16
            || manifest.snapshot_id.iter().all(|byte| *byte == 0)
            || manifest.heads.len() > SYNC_MANIFEST_PAGE_HEADS
            || manifest.page_index != self.remote_manifest_next_page
            || (manifest.heads.is_empty() && !(manifest.page_index == 0 && manifest.last_page))
        {
            return Err(SyncSessionError::Protocol);
        }
        trace!(
            page = manifest.page_index,
            heads = manifest.heads.len(),
            last = manifest.last_page,
            "Sync manifest page received"
        );
        let snapshot_id = fixed::<16>(&manifest.snapshot_id)?;
        match self.remote_snapshot_id {
            None => self.remote_snapshot_id = Some(snapshot_id),
            Some(expected) if expected == snapshot_id => {}
            Some(_) => return Err(SyncSessionError::Protocol),
        }
        // Pages accumulate into the remote manifest, bounded by the count the
        // peer announced.
        if self
            .remote_manifest
            .len()
            .checked_add(manifest.heads.len())
            .is_none_or(|total| total > self.remote_known_object_count)
        {
            return Err(SyncSessionError::Capacity);
        }
        self.remote_manifest
            .try_reserve(manifest.heads.len())
            .map_err(|_| SyncSessionError::Capacity)?;
        for head in manifest.heads {
            let parsed = Head {
                record_kind: RecordKind::from_wire(head.record_kind)
                    .ok_or(SyncSessionError::Protocol)?,
                record_id: fixed::<16>(&head.record_id)?,
                version_id: if head.version_id.is_empty() {
                    None
                } else {
                    Some(fixed::<16>(&head.version_id)?)
                },
                object_hash: fixed::<32>(&head.object_hash)?,
            };
            // Heads are strictly increasing across pages, which rejects
            // duplicates and reordering without rescanning earlier pages.
            let in_order = self.remote_manifest.last().is_none_or(|previous| {
                (previous.record_id, previous.version_id) < (parsed.record_id, parsed.version_id)
            });
            if !in_order
                || is_zero_id(&parsed.record_id)
                || parsed.version_id.as_ref().is_some_and(is_zero_id)
                || parsed.object_hash.iter().all(|byte| *byte == 0)
                || self
                    .remote_heads
                    .insert(head_key(&parsed), parsed)
                    .is_some()
            {
                return Err(SyncSessionError::Protocol);
            }
            self.check_no_collision(&parsed)?;
            self.remote_manifest.push(parsed);
        }
        self.remote_manifest_next_page = self
            .remote_manifest_next_page
            .checked_add(1)
            .ok_or(SyncSessionError::Capacity)?;
        if !manifest.last_page {
            return Ok(());
        }
        self.remote_manifest_received = true;
        debug!(
            heads = self.remote_manifest.len(),
            pages = self.remote_manifest_next_page,
            "Sync remote manifest complete"
        );

        if self.local_is_smaller {
            self.serve_started = true;
        } else {
            self.begin_receive_phase()?;
        }
        Ok(())
    }

    /// A version id the peer advertises with other bytes than ours is a
    /// collision. Deletion marks never collide: two devices may delete the
    /// same record with different marks.
    fn check_no_collision(&self, head: &Head) -> Result<(), SyncSessionError> {
        let Some(version_id) = head.version_id else {
            return Ok(());
        };
        if let Some(local) = self.store.version(&version_id).map_err(map_sync_error)? {
            if local.record_id != head.record_id
                || local.hash().map_err(map_sync_error)? != head.object_hash
            {
                return Err(SyncSessionError::Protocol);
            }
        }
        Ok(())
    }

    fn on_request(&mut self, request: proto::SyncRequest) -> Result<(), SyncSessionError> {
        if !self.serve_started
            || self.serve_done
            || self.outbound_transfer.is_some()
            || self.awaiting_ack.is_some()
            || !self.outbound_plan.is_empty()
            || self.inbound_transfer.is_some()
        {
            return Err(SyncSessionError::Protocol);
        }
        let snapshot = self
            .local_snapshot
            .as_ref()
            .ok_or(SyncSessionError::Protocol)?;
        if request.snapshot_id.as_slice() != snapshot.snapshot_id {
            return Err(SyncSessionError::Protocol);
        }
        if request.wanted.len() > SESSION_MAX_REQUESTED_HEADS {
            return Err(SyncSessionError::Capacity);
        }
        if request.wanted.is_empty() {
            self.serve_done = true;
            self.request_record_ids.clear();
            debug!("Sync serve phase complete");
            if self.local_is_smaller {
                self.begin_receive_phase()?;
            }
            self.refresh_completion();
            return Ok(());
        }

        let requested_count = request.wanted.len();
        let mut wanted = BTreeSet::new();
        let mut requested_records = BTreeSet::new();
        for item in request.wanted {
            let record_id = fixed::<16>(&item.record_id)?;
            let version_id = if item.version_id.is_empty() {
                None
            } else {
                Some(fixed::<16>(&item.version_id)?)
            };
            if is_zero_id(&record_id) || !wanted.insert((record_id, version_id)) {
                return Err(SyncSessionError::Protocol);
            }
            // Only what this round's manifest offered may be requested.
            if !snapshot.heads.contains_key(&(record_id, version_id)) {
                return Err(SyncSessionError::Protocol);
            }
            requested_records.insert(record_id);
        }
        let plan = self.closure_plan(snapshot, &wanted)?;
        if plan.is_empty() {
            return Err(SyncSessionError::Protocol);
        }
        debug!(
            requested = requested_count,
            planned = plan.len(),
            "Sync serve request accepted"
        );
        self.outbound_plan = plan.into();
        self.request_record_ids = requested_records;
        self.prepare_next_outbound_batch()?;
        Ok(())
    }

    fn on_chunk(&mut self, chunk: proto::SyncChunk) -> Result<(), SyncSessionError> {
        if !self.receive_started || self.receive_done || self.active_window.is_empty() {
            return Err(SyncSessionError::Protocol);
        }
        let transfer_id = fixed::<16>(&chunk.transfer_id)?;
        let digest = fixed::<32>(&chunk.sha256_digest)?;
        let total_size =
            usize::try_from(chunk.total_size).map_err(|_| SyncSessionError::Capacity)?;
        if is_zero_id(&transfer_id)
            || total_size == 0
            || total_size > SYNC_MAX_BATCH_BYTES
            || digest.iter().all(|byte| *byte == 0)
        {
            return Err(SyncSessionError::Capacity);
        }
        if self.inbound_transfer.is_none() {
            if chunk.chunk_index != 0 || self.used_transfer_ids.contains(&transfer_id) {
                return Err(SyncSessionError::Protocol);
            }
            if self.used_transfer_ids.len() >= MAX_TRACKED_TRANSFERS {
                return Err(SyncSessionError::Capacity);
            }
            let count = total_size
                .checked_add(SESSION_CHUNK_BYTES - 1)
                .ok_or(SyncSessionError::Capacity)?
                / SESSION_CHUNK_BYTES;
            if count == 0 || usize::try_from(chunk.chunk_count).ok() != Some(count) {
                return Err(SyncSessionError::Protocol);
            }
            self.used_transfer_ids.insert(transfer_id);
            self.inbound_transfer = Some(InboundTransfer {
                transfer_id,
                digest,
                total_size,
                chunk_count: count,
                next_chunk: 0,
                encoded: Vec::new(),
            });
        }
        let transfer = self
            .inbound_transfer
            .as_mut()
            .ok_or(SyncSessionError::Protocol)?;
        if transfer.transfer_id != transfer_id
            || transfer.digest != digest
            || transfer.total_size != total_size
            || usize::try_from(chunk.chunk_count).ok() != Some(transfer.chunk_count)
            || usize::try_from(chunk.chunk_index).ok() != Some(transfer.next_chunk)
        {
            return Err(SyncSessionError::Protocol);
        }
        let expected_len = if transfer.next_chunk + 1 == transfer.chunk_count {
            transfer.total_size - transfer.next_chunk * SESSION_CHUNK_BYTES
        } else {
            SESSION_CHUNK_BYTES
        };
        if chunk.data.len() != expected_len
            || transfer
                .encoded
                .len()
                .checked_add(chunk.data.len())
                .is_none_or(|size| size > transfer.total_size)
        {
            return Err(SyncSessionError::Protocol);
        }
        let chunk_index = chunk.chunk_index;
        let chunk_bytes = chunk.data.len();
        let is_first_chunk = transfer.next_chunk == 0;
        let is_last_chunk = transfer.next_chunk.saturating_add(1) == transfer.chunk_count;
        transfer
            .encoded
            .try_reserve(chunk_bytes)
            .map_err(|_| SyncSessionError::Capacity)?;
        transfer.encoded.extend_from_slice(&chunk.data);
        transfer.next_chunk += 1;
        if is_first_chunk {
            debug!(
                chunks = transfer.chunk_count,
                total_bytes = transfer.total_size,
                "Sync inbound transfer started"
            );
        }
        if is_last_chunk {
            debug!(
                chunks = transfer.chunk_count,
                total_bytes = transfer.total_size,
                "Sync inbound transfer complete"
            );
        }
        trace!(
            index = chunk_index,
            bytes = chunk_bytes,
            "Sync inbound chunk accepted"
        );
        Ok(())
    }
    fn on_commit(&mut self, commit: proto::SyncCommit) -> Result<(), SyncSessionError> {
        if !self.receive_started || self.receive_done || self.active_window.is_empty() {
            return Err(SyncSessionError::Protocol);
        }
        let transfer_id = fixed::<16>(&commit.transfer_id)?;
        let digest = fixed::<32>(&commit.sha256_digest)?;
        let transfer = self
            .inbound_transfer
            .take()
            .ok_or(SyncSessionError::Protocol)?;
        if transfer.transfer_id != transfer_id
            || transfer.digest != digest
            || transfer.next_chunk != transfer.chunk_count
            || transfer.encoded.len() != transfer.total_size
            || <[u8; 32]>::from(Sha256::digest(&transfer.encoded)) != transfer.digest
        {
            return Err(SyncSessionError::Protocol);
        }
        let batch = decode_batch(&transfer.encoded).map_err(map_sync_error)?;
        let projected_count = self
            .received_object_count
            .checked_add(batch.objects())
            .ok_or(SyncSessionError::Capacity)?;
        if projected_count > SESSION_MAX_KNOWN_OBJECTS {
            return Err(SyncSessionError::Capacity);
        }
        let mut keys = Vec::with_capacity(batch.objects());
        for deletion in &batch.deletions {
            let key = (deletion.record_id, None);
            if let Some(head) = self.remote_heads.get(&key) {
                if deletion.hash().map_err(map_sync_error)? != head.object_hash {
                    return Err(SyncSessionError::Protocol);
                }
            }
            keys.push((key, deletion.record_id));
        }
        for version in &batch.versions {
            let key = (version.record_id, Some(version.version_id));
            if let Some(head) = self.remote_heads.get(&key) {
                if version.hash().map_err(map_sync_error)? != head.object_hash {
                    return Err(SyncSessionError::Protocol);
                }
            }
            keys.push((key, version.record_id));
        }
        for (key, record) in &keys {
            // Only records of the requested window, and nothing twice per round.
            if !self.request_record_ids.contains(record) || self.inbound_objects.contains(key) {
                return Err(SyncSessionError::Protocol);
            }
        }
        verify_batch(&batch, self.trust.as_ref()).map_err(map_sync_error)?;
        // The store applies the batch atomically; no accepted ACK is queued
        // before it returns.
        let outcome = self
            .store
            .apply(batch, &self.expected_peer.device_id)
            .map_err(map_sync_error)?;
        self.received_object_count = projected_count;
        self.round_inserted_count = self
            .round_inserted_count
            .checked_add(outcome.inserted)
            .ok_or(SyncSessionError::Capacity)?;
        self.inbound_objects
            .extend(keys.into_iter().map(|(key, _)| key));
        debug!(
            inserted = outcome.inserted,
            skipped = outcome.skipped,
            received_total = self.received_object_count,
            "Sync batch committed"
        );
        self.queue_control(SyncOutbound::Ack(proto::SyncAck {
            transfer_id: transfer_id.to_vec(),
            status: proto::AckStatus::Accepted as i32,
        }))?;
        if self.active_window_is_present()? {
            self.queue_next_receive_request()?;
        }
        Ok(())
    }

    fn on_ack(&mut self, ack: proto::SyncAck) -> Result<(), SyncSessionError> {
        let transfer_id = fixed::<16>(&ack.transfer_id)?;
        if self.awaiting_ack != Some(transfer_id) {
            return Err(SyncSessionError::Protocol);
        }
        if proto::AckStatus::try_from(ack.status).ok() != Some(proto::AckStatus::Accepted) {
            return Err(SyncSessionError::Protocol);
        }
        let transfer = self
            .outbound_transfer
            .take()
            .ok_or(SyncSessionError::Protocol)?;
        if transfer.transfer_id != transfer_id || !transfer.commit_sent {
            return Err(SyncSessionError::Protocol);
        }
        self.store
            .mark_shared(&self.expected_peer.device_id, &transfer.records)
            .map_err(map_sync_error)?;
        let acknowledged_count = transfer.items.len();
        for item in transfer.items {
            if self.acknowledged_objects.insert(item) {
                self.acknowledged_object_count = self.acknowledged_object_count.saturating_add(1);
            }
        }
        self.awaiting_ack = None;
        debug!(
            objects = acknowledged_count,
            acknowledged_total = self.acknowledged_object_count,
            "Sync batch acknowledged"
        );
        if !self.outbound_plan.is_empty() {
            self.prepare_next_outbound_batch()?;
        }
        Ok(())
    }

    fn freeze_local_snapshot(&self) -> Result<FrozenSnapshot, SyncSessionError> {
        let index = self.store.index().map_err(map_sync_error)?;
        let stored_count = index.versions.len() + index.deletions.len();
        if stored_count > SESSION_MAX_KNOWN_OBJECTS {
            return Err(SyncSessionError::Capacity);
        }
        let shared = self
            .store
            .shared_records(&self.expected_peer.device_id)
            .map_err(map_sync_error)?;
        let routed = route(index, &self.keys, &shared).map_err(map_sync_error)?;
        let deletions = routed
            .heads
            .iter()
            .filter(|head| head.version_id.is_none())
            .count();
        let object_count = routed.versions.len() + deletions;
        let mut heads = BTreeMap::new();
        for head in &routed.heads {
            if heads.insert(head_key(head), *head).is_some() {
                return Err(SyncSessionError::Storage);
            }
        }
        debug!(
            objects = object_count,
            heads = routed.heads.len(),
            "Sync local snapshot frozen"
        );
        Ok(FrozenSnapshot {
            snapshot_id: fresh_id().map_err(map_sync_error)?,
            stored_count,
            object_count,
            manifest: routed.heads,
            heads,
            versions: routed.versions,
            order: routed.order,
        })
    }

    /// Deletion marks first, then the requested versions with every ancestor
    /// in topological order, without what the peer already acknowledged.
    fn closure_plan(
        &self,
        snapshot: &FrozenSnapshot,
        wanted: &BTreeSet<(Id, Option<Id>)>,
    ) -> Result<Vec<PlanItem>, SyncSessionError> {
        let mut plan = Vec::new();
        let mut closure = HashSet::new();
        let mut stack = Vec::new();
        for (record_id, version_id) in wanted {
            match version_id {
                None => plan.push(PlanItem::Deletion(*record_id)),
                Some(version_id) => stack.push(*version_id),
            }
        }
        while let Some(version_id) = stack.pop() {
            if !closure.insert(version_id) {
                continue;
            }
            let meta = snapshot
                .versions
                .get(&version_id)
                .ok_or(SyncSessionError::Protocol)?;
            stack.extend(meta.parents.iter().copied());
        }
        for version_id in &snapshot.order {
            if closure.contains(version_id) {
                plan.push(PlanItem::Version(*version_id));
            }
        }
        plan.retain(|item| !self.acknowledged_objects.contains(item));
        Ok(plan)
    }

    fn prepare_next_outbound_batch(&mut self) -> Result<(), SyncSessionError> {
        if self.outbound_transfer.is_some() || self.awaiting_ack.is_some() {
            return Err(SyncSessionError::Protocol);
        }
        if self.outbound_plan.is_empty() {
            return Ok(());
        }
        let snapshot = self
            .local_snapshot
            .as_ref()
            .ok_or(SyncSessionError::Protocol)?;
        let mut batch = Batch::default();
        let mut items = Vec::new();
        let mut records = Vec::new();
        let mut authors = Vec::new();
        let mut included = HashSet::new();
        let mut estimated_bytes = 0_usize;
        for item in self.outbound_plan.iter().take(SYNC_MAX_OBJECTS_PER_BATCH) {
            let (size, record, author) = match item {
                PlanItem::Deletion(record_id) => {
                    let deletion = self
                        .store
                        .deletion(record_id)
                        .map_err(map_sync_error)?
                        .ok_or(SyncSessionError::Storage)?;
                    let advertised = snapshot
                        .heads
                        .get(&(*record_id, None))
                        .ok_or(SyncSessionError::Protocol)?;
                    if deletion.hash().map_err(map_sync_error)? != advertised.object_hash {
                        return Err(SyncSessionError::Storage);
                    }
                    let author = self.verified_author(deletion.author)?;
                    deletion
                        .verify(&author)
                        .map_err(|_| SyncSessionError::Storage)?;
                    let size = OBJECT_OVERHEAD;
                    if !fits(estimated_bytes, size, items.is_empty())? {
                        break;
                    }
                    let author = deletion.author;
                    batch.deletions.push(deletion);
                    (size, *record_id, author)
                }
                PlanItem::Version(version_id) => {
                    let meta = snapshot
                        .versions
                        .get(version_id)
                        .ok_or(SyncSessionError::Protocol)?;
                    let version = self
                        .store
                        .version(version_id)
                        .map_err(map_sync_error)?
                        .ok_or(SyncSessionError::Storage)?;
                    if version.record_id != meta.record_id
                        || version.hash().map_err(map_sync_error)? != meta.hash
                    {
                        return Err(SyncSessionError::Storage);
                    }
                    let author = self.verified_author(version.author)?;
                    version
                        .verify(&author)
                        .map_err(|_| SyncSessionError::Storage)?;
                    // A blob a parent already names is with the receiver, or
                    // travels with that parent earlier in this round.
                    let mut blobs = Vec::new();
                    for hash in version.blobs() {
                        let in_parent = version.parents.iter().any(|parent| {
                            snapshot
                                .versions
                                .get(parent)
                                .is_some_and(|parent| parent.blobs.contains(hash))
                        });
                        if in_parent || included.contains(hash) {
                            continue;
                        }
                        let blob = self
                            .store
                            .blob(hash)
                            .map_err(map_sync_error)?
                            .ok_or(SyncSessionError::Storage)?;
                        blobs.push((*hash, blob));
                    }
                    let size = OBJECT_OVERHEAD
                        + version.state.len()
                        + blobs.iter().map(|(_, blob)| blob.len()).sum::<usize>();
                    if !fits(estimated_bytes, size, items.is_empty())? {
                        break;
                    }
                    for (hash, blob) in blobs {
                        included.insert(hash);
                        batch.blobs.push(blob);
                    }
                    let author = version.author;
                    let record = version.record_id;
                    batch.versions.push(version);
                    (size, record, author)
                }
            };
            estimated_bytes += size;
            items.push(*item);
            records.push(record);
            authors.push(author);
        }
        let encoded = encode_batch(&batch).map_err(map_sync_error)?;
        for _ in 0..items.len() {
            let _ = self.outbound_plan.pop_front();
        }
        if self.issued_transfer_ids.len() >= MAX_TRACKED_TRANSFERS {
            return Err(SyncSessionError::Capacity);
        }
        let mut fresh_transfer_id = None;
        for _ in 0..4 {
            let candidate = fresh_id().map_err(map_sync_error)?;
            if self.issued_transfer_ids.insert(candidate) {
                fresh_transfer_id = Some(candidate);
                break;
            }
        }
        let transfer_id = fresh_transfer_id.ok_or(SyncSessionError::Randomness)?;
        let digest: Digest = Sha256::digest(&encoded).into();
        let encoded_len = encoded.len();
        let chunk_count = encoded
            .len()
            .checked_add(SESSION_CHUNK_BYTES - 1)
            .ok_or(SyncSessionError::Capacity)?
            / SESSION_CHUNK_BYTES;
        let object_count = items.len();
        records.sort_unstable();
        records.dedup();
        self.outbound_transfer = Some(OutboundTransfer {
            transfer_id,
            digest,
            encoded,
            chunk_count,
            next_chunk: 0,
            commit_sent: false,
            items,
            records,
            author_device_ids: authors,
        });
        debug!(
            objects = object_count,
            bytes = encoded_len,
            chunks = chunk_count,
            remaining = self.outbound_plan.len(),
            "Sync outbound batch prepared"
        );
        Ok(())
    }

    /// The identity of a stored object's author, to recheck its signature
    /// before serving it. History signed by a device forgotten since then
    /// stays servable; only receiving requires a currently trusted author.
    fn verified_author(&self, author: DeviceId) -> Result<PublicIdentity, SyncSessionError> {
        let identity = self
            .trust
            .lookup_history(author)
            .map_err(map_sync_error)?
            .ok_or(SyncSessionError::UnknownAuthor)?;
        if identity.device_id != author || identity.validate().is_err() {
            return Err(SyncSessionError::Protocol);
        }
        Ok(identity)
    }

    fn begin_receive_phase(&mut self) -> Result<(), SyncSessionError> {
        if self.receive_started || self.receive_done || !self.remote_manifest_received {
            return Err(SyncSessionError::Protocol);
        }
        self.receive_started = true;
        self.queue_next_receive_request()
    }
    fn queue_next_receive_request(&mut self) -> Result<(), SyncSessionError> {
        if self.active_window_is_present()? {
            self.active_window.clear();
        } else if !self.active_window.is_empty() {
            return Err(SyncSessionError::Protocol);
        }
        let mut missing = Vec::new();
        for head in &self.remote_manifest {
            if !self.is_present(head)? {
                missing.push(*head);
            }
        }
        let snapshot_id = self
            .remote_snapshot_id
            .ok_or(SyncSessionError::Protocol)?
            .to_vec();
        if missing.is_empty() {
            self.queue_control(SyncOutbound::Request(proto::SyncRequest {
                snapshot_id,
                wanted: Vec::new(),
            }))?;
            return Ok(());
        }
        let window = missing
            .into_iter()
            .take(SESSION_MAX_REQUESTED_HEADS)
            .collect::<Vec<_>>();
        self.request_record_ids = window.iter().map(|head| head.record_id).collect();
        self.active_window = window.clone();
        self.queue_control(SyncOutbound::Request(proto::SyncRequest {
            snapshot_id,
            wanted: window
                .iter()
                .map(|head| proto::SyncWanted {
                    record_id: head.record_id.to_vec(),
                    version_id: head.version_id.map_or_else(Vec::new, |id| id.to_vec()),
                })
                .collect(),
        }))
    }

    fn active_window_is_present(&self) -> Result<bool, SyncSessionError> {
        if self.active_window.is_empty() {
            return Ok(false);
        }
        for head in &self.active_window {
            if !self.is_present(head)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Whether nothing more is needed for `head`: the record is deleted here
    /// (a version of it would be skipped anyway), or the version is stored
    /// with the advertised bytes.
    fn is_present(&self, head: &Head) -> Result<bool, SyncSessionError> {
        if self
            .store
            .deletion(&head.record_id)
            .map_err(map_sync_error)?
            .is_some()
        {
            return Ok(true);
        }
        let Some(version_id) = head.version_id else {
            return Ok(false);
        };
        match self.store.version(&version_id).map_err(map_sync_error)? {
            Some(version) => {
                if version.record_id != head.record_id
                    || version.hash().map_err(map_sync_error)? != head.object_hash
                {
                    return Err(SyncSessionError::Protocol);
                }
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Manifest pages bypass the small control backlog: all of them are queued
    /// at once, in order, before any request that depends on them.
    fn queue_manifest_page(&mut self, page: proto::SyncManifest) -> Result<(), SyncSessionError> {
        if page.encoded_len() > kryptopass_protocol::MAX_PAYLOAD_SIZE {
            return Err(SyncSessionError::Capacity);
        }
        self.control_queue.push_back(SyncOutbound::Manifest(page));
        Ok(())
    }

    fn queue_control(&mut self, message: SyncOutbound) -> Result<(), SyncSessionError> {
        let encoded_len = match &message {
            SyncOutbound::Hello(payload) => payload.encoded_len(),
            SyncOutbound::Manifest(payload) => payload.encoded_len(),
            SyncOutbound::Request(payload) => payload.encoded_len(),
            SyncOutbound::Chunk(payload) => payload.encoded_len(),
            SyncOutbound::Commit(payload) => payload.encoded_len(),
            SyncOutbound::Ack(payload) => payload.encoded_len(),
        };
        if encoded_len > kryptopass_protocol::MAX_PAYLOAD_SIZE {
            return Err(SyncSessionError::Capacity);
        }
        let pending = self
            .control_queue
            .iter()
            .filter(|queued| !matches!(queued, SyncOutbound::Manifest(_)))
            .count();
        if pending >= MAX_PENDING_CONTROL {
            return Err(SyncSessionError::Capacity);
        }
        self.control_queue.push_back(message);
        Ok(())
    }

    fn ensure_session_trusted(&self) -> Result<(), SyncSessionError> {
        let local = self
            .trust
            .lookup(self.local_identity.device_id)
            .map_err(map_sync_error)?;
        let trusted = self
            .trust
            .lookup(self.expected_peer.device_id)
            .map_err(map_sync_error)?;
        if local == Some(self.local_identity) && trusted == Some(self.expected_peer) {
            Ok(())
        } else {
            Err(SyncSessionError::TrustRevoked)
        }
    }

    fn ensure_authors_trusted(
        &self,
        author_device_ids: &[DeviceId],
    ) -> Result<(), SyncSessionError> {
        for author_device_id in author_device_ids {
            let author = self
                .trust
                .lookup_history(*author_device_id)
                .map_err(map_sync_error)?
                .ok_or(SyncSessionError::UnknownAuthor)?;
            if author.device_id != *author_device_id || author.validate().is_err() {
                return Err(SyncSessionError::Protocol);
            }
        }
        Ok(())
    }

    fn refresh_completion(&mut self) {
        let was_complete = self.complete;
        self.complete = self.receive_done && self.serve_done;
        if self.complete {
            self.control_queue.clear();
            self.outbound_plan.clear();
            self.outbound_transfer = None;
            self.inbound_transfer = None;
            self.active_window.clear();
            if !was_complete {
                info!(
                    round = self.round_number,
                    received = self.received_object_count,
                    acknowledged = self.acknowledged_object_count,
                    local_heads = self
                        .local_snapshot
                        .as_ref()
                        .map_or(0, |snapshot| snapshot.manifest.len()),
                    remote_heads = self.remote_manifest.len(),
                    "Sync round complete"
                );
            }
        }
    }

    fn fail<T>(&mut self, error: SyncSessionError) -> Result<T, SyncSessionError> {
        let first_failure = !self.failed;
        let state = self.progress().state;
        self.failed = true;
        self.failure = Some(error);
        self.control_queue.clear();
        self.outbound_plan.clear();
        self.outbound_transfer = None;
        self.inbound_transfer = None;
        self.active_window.clear();
        if first_failure {
            warn!(
                error = %error,
                round = self.round_number,
                state = ?state,
                received = self.received_object_count,
                acknowledged = self.acknowledged_object_count,
                local_heads = self
                    .local_snapshot
                    .as_ref()
                    .map_or(0, |snapshot| snapshot.manifest.len()),
                remote_heads = self.remote_manifest.len(),
                "Sync round failed"
            );
        }
        Err(error)
    }
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], SyncSessionError> {
    bytes.try_into().map_err(|_| SyncSessionError::Protocol)
}

fn head_key(head: &Head) -> (Id, Option<Id>) {
    (head.record_id, head.version_id)
}

fn wire_head(head: &Head) -> proto::SyncHead {
    proto::SyncHead {
        record_kind: head.record_kind.as_wire(),
        record_id: head.record_id.to_vec(),
        version_id: head.version_id.map_or_else(Vec::new, |id| id.to_vec()),
        object_hash: head.object_hash.to_vec(),
    }
}

/// Whether an object of `size` still fits after `estimated` bytes. The first
/// object of a batch must fit on its own.
fn fits(estimated: usize, size: usize, first: bool) -> Result<bool, SyncSessionError> {
    let total = estimated
        .checked_add(size)
        .ok_or(SyncSessionError::Capacity)?;
    if total <= SYNC_MAX_BATCH_BYTES {
        Ok(true)
    } else if first {
        Err(SyncSessionError::Capacity)
    } else {
        Ok(false)
    }
}
