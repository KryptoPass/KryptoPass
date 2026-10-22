//! One bounded Sync v1 round over an already authenticated Noise session.
//!
//! The engine owns its isolated ledger and trust source. It is deliberately
//! event driven: callers pass one decoded sync message at a time and pull at
//! most one outbound message per loop, leaving the authenticated session owner
//! responsible for keepalives, closure, revocation and transport I/O.

use crate::{
    Digest, Id, Ledger, RevisionHead, RevisionMeta, SYNC_MAX_BATCH_BYTES,
    SYNC_MAX_LEDGER_REVISIONS, SYNC_MAX_MANIFEST_HEADS, SYNC_MAX_REVISIONS_PER_BATCH, SyncError,
    TrustSource, decode_batch, encode_batch, fresh_id, is_zero_id, topological_meta_order,
};
use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_protocol::{MessagePayload, proto};
use prost::Message as _;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::fmt;
use std::time::{Duration, Instant};

/// Maximum metadata count advertised by one endpoint.
pub const SESSION_MAX_KNOWN_REVISIONS: usize = SYNC_MAX_LEDGER_REVISIONS;
/// Maximum requested heads in one receive window.
pub const SESSION_MAX_REQUESTED_HEADS: usize = 256;
/// Maximum opaque fragment payload.
pub const SESSION_CHUNK_BYTES: usize = 48 * 1024;
/// Idle request or transfer timeout.
pub const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Absolute maximum lifetime of one round.
pub const SESSION_ROUND_TIMEOUT: Duration = Duration::from_secs(300);

const MAX_PENDING_CONTROL: usize = 4;
const MAX_TRACKED_TRANSFERS: usize = SESSION_MAX_KNOWN_REVISIONS;

/// A payload-free failure from the authenticated-session Sync engine.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SyncSessionError {
    /// The supplied local identity, peer identity or fingerprint binding is invalid.
    InvalidBinding,
    /// The authenticated peer is no longer exactly present in the trust source.
    TrustRevoked,
    /// A sync message arrived in an invalid order or failed a protocol check.
    Protocol,
    /// The round exceeded its idle or absolute timeout.
    Timeout,
    /// A bounded queue, object count or message size limit was exceeded.
    Capacity,
    /// A ledger read or transactional insert failed.
    Storage,
    /// A revision author is absent from the explicit trust source.
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
            Self::Storage => "sync ledger operation failed",
            Self::UnknownAuthor => "sync revision author is not trusted",
            Self::Randomness => "sync random source failed",
        })
    }
}

impl std::error::Error for SyncSessionError {}

fn map_sync_error(error: SyncError) -> SyncSessionError {
    match error {
        SyncError::Storage | SyncError::InvalidLedger => SyncSessionError::Storage,
        SyncError::UnknownAuthor => SyncSessionError::UnknownAuthor,
        SyncError::Randomness => SyncSessionError::Randomness,
        SyncError::LimitExceeded | SyncError::Allocation => SyncSessionError::Capacity,
        _ => SyncSessionError::Protocol,
    }
}

/// One typed outbound message. Ciphertext fragments are redacted from Debug.
pub enum SyncOutbound {
    /// Announces this explicitly bound ledger after SessionHello negotiation.
    Hello(proto::SyncHello),
    /// Publishes the frozen local manifest for this round.
    Manifest(proto::SyncManifest),
    /// Requests missing heads from the peer's frozen snapshot.
    Request(proto::SyncRequest),
    /// Carries one bounded opaque batch fragment.
    Chunk(proto::SyncChunk),
    /// Commits one completed encoded revision batch.
    Commit(proto::SyncCommit),
    /// Acknowledges a batch only after its ledger transaction commits.
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
    /// Number of valid revision objects committed from the peer in this round.
    pub received_revisions: usize,
    /// Number of revisions accepted by a peer ACK in this round.
    pub acknowledged_revisions: usize,
    /// Number of heads in the local frozen manifest.
    pub local_heads: usize,
    /// Number of heads in the remote frozen manifest, once received.
    pub remote_heads: usize,
}

struct FrozenSnapshot {
    snapshot_id: Id,
    manifest: Vec<RevisionHead>,
    heads_by_revision: BTreeMap<Id, RevisionHead>,
    index: BTreeMap<Id, RevisionMeta>,
    topological_ids: Vec<Id>,
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
    revision_ids: Vec<Id>,
    author_device_ids: Vec<DeviceId>,
}

/// One explicitly bound, event-driven authenticated-session sync round.
///
/// The caller must create this only after bilateral `SessionHello` validation
/// and retain one instance for the lifetime of that exact authenticated peer.
pub struct SyncSession {
    ledger: Ledger,
    trust: Box<dyn TrustSource + Send>,
    expected_peer: PublicIdentity,
    expected_fingerprint: Vec<u8>,
    started_at: Instant,
    last_activity: Instant,
    remote_hello_received: bool,
    remote_manifest_received: bool,
    local_is_smaller: bool,
    local_snapshot: Option<FrozenSnapshot>,
    remote_snapshot_id: Option<Id>,
    remote_manifest: Vec<RevisionHead>,
    remote_heads_by_revision: BTreeMap<Id, RevisionHead>,
    control_queue: VecDeque<SyncOutbound>,
    receive_started: bool,
    receive_done: bool,
    active_window: Vec<RevisionHead>,
    serve_started: bool,
    serve_done: bool,
    request_record_ids: BTreeSet<Id>,
    outbound_plan: VecDeque<Id>,
    outbound_transfer: Option<OutboundTransfer>,
    awaiting_ack: Option<Id>,
    acknowledged_revision_ids: HashSet<Id>,
    inbound_transfer: Option<InboundTransfer>,
    used_transfer_ids: HashSet<Id>,
    issued_transfer_ids: HashSet<Id>,
    inbound_revision_ids: HashSet<Id>,
    received_revision_count: usize,
    acknowledged_revision_count: usize,
    failed: bool,
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
    /// Starts a round with an explicit existing ledger/trust binding.
    ///
    /// The fingerprint must be the caller's already selected 20- or 32-byte
    /// OpenPGP fingerprint. This API never chooses a vault or fingerprint.
    pub fn new(
        ledger: Ledger,
        trust: Box<dyn TrustSource + Send>,
        local_device_id: DeviceId,
        expected_peer: PublicIdentity,
        expected_openpgp_fingerprint: Vec<u8>,
        now: Instant,
    ) -> Result<Self, SyncSessionError> {
        if local_device_id.as_bytes().iter().all(|byte| *byte == 0)
            || expected_peer.device_id == local_device_id
            || !matches!(expected_openpgp_fingerprint.len(), 20 | 32)
            || expected_openpgp_fingerprint.iter().all(|byte| *byte == 0)
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
        let known_revision_count = ledger.revision_count().map_err(map_sync_error)?;
        if known_revision_count > SESSION_MAX_KNOWN_REVISIONS {
            return Err(SyncSessionError::Capacity);
        }

        let hello = proto::SyncHello {
            vault_id: ledger.vault_id().to_vec(),
            device_id: local_device_id.as_bytes().to_vec(),
            known_revision_count: known_revision_count as u64,
            openpgp_fingerprint: expected_openpgp_fingerprint.clone(),
        };
        let mut control_queue = VecDeque::with_capacity(MAX_PENDING_CONTROL);
        control_queue.push_back(SyncOutbound::Hello(hello));
        Ok(Self {
            ledger,
            trust,
            expected_peer,
            expected_fingerprint: expected_openpgp_fingerprint,
            started_at: now,
            last_activity: now,
            remote_hello_received: false,
            remote_manifest_received: false,
            local_is_smaller: local_device_id.as_bytes() < expected_peer.device_id.as_bytes(),
            local_snapshot: None,
            remote_snapshot_id: None,
            remote_manifest: Vec::new(),
            remote_heads_by_revision: BTreeMap::new(),
            control_queue,
            receive_started: false,
            receive_done: false,
            active_window: Vec::new(),
            serve_started: false,
            serve_done: false,
            request_record_ids: BTreeSet::new(),
            outbound_plan: VecDeque::new(),
            outbound_transfer: None,
            awaiting_ack: None,
            acknowledged_revision_ids: HashSet::new(),
            inbound_transfer: None,
            used_transfer_ids: HashSet::new(),
            issued_transfer_ids: HashSet::new(),
            inbound_revision_ids: HashSet::new(),
            received_revision_count: 0,
            acknowledged_revision_count: 0,
            failed: false,
            complete: false,
        })
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
        if self.complete || self.failed {
            return Ok(None);
        }
        self.check_time(now)?;
        if let Some(message) = self.control_queue.pop_front() {
            match message {
                SyncOutbound::Request(ref request) if request.revisions.is_empty() => {
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
        if self.complete || self.failed {
            return self.fail(SyncSessionError::Protocol);
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
            return Err(SyncSessionError::Protocol);
        }
        if self.complete {
            return Ok(());
        }
        if now.saturating_duration_since(self.started_at) >= SESSION_ROUND_TIMEOUT
            || now.saturating_duration_since(self.last_activity) >= SESSION_IDLE_TIMEOUT
        {
            return self.fail(SyncSessionError::Timeout);
        }
        if let Err(error) = self.ensure_peer_trusted() {
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
            received_revisions: self.received_revision_count,
            acknowledged_revisions: self.acknowledged_revision_count,
            local_heads: self
                .local_snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.manifest.len()),
            remote_heads: self.remote_manifest.len(),
        }
    }

    /// Indicates that both directed phases have ended successfully.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    /// Cancels the round and discards every queued or partial transfer buffer.
    pub fn cancel(&mut self) {
        self.failed = true;
        self.control_queue.clear();
        self.outbound_plan.clear();
        self.outbound_transfer = None;
        self.inbound_transfer = None;
        self.active_window.clear();
    }

    fn on_hello(&mut self, hello: proto::SyncHello) -> Result<(), SyncSessionError> {
        if self.remote_hello_received {
            return Err(SyncSessionError::Protocol);
        }
        if hello.vault_id.as_slice() != self.ledger.vault_id()
            || hello.device_id.as_slice() != self.expected_peer.device_id.as_bytes()
            || hello.known_revision_count > SESSION_MAX_KNOWN_REVISIONS as u64
            || hello.openpgp_fingerprint != self.expected_fingerprint
        {
            return Err(SyncSessionError::InvalidBinding);
        }
        self.remote_hello_received = true;
        let snapshot = self.freeze_local_snapshot()?;
        let manifest = proto::SyncManifest {
            snapshot_id: snapshot.snapshot_id.to_vec(),
            heads: snapshot
                .manifest
                .iter()
                .map(|head| proto::RevisionHead {
                    record_id: head.record_id.to_vec(),
                    revision_id: head.revision_id.to_vec(),
                    revision_hash: head.revision_hash.to_vec(),
                })
                .collect(),
        };
        self.local_snapshot = Some(snapshot);
        self.queue_control(SyncOutbound::Manifest(manifest))?;
        Ok(())
    }

    fn on_manifest(&mut self, manifest: proto::SyncManifest) -> Result<(), SyncSessionError> {
        if !self.remote_hello_received
            || self.remote_manifest_received
            || manifest.snapshot_id.len() != 16
            || manifest.snapshot_id.iter().all(|byte| *byte == 0)
            || manifest.heads.len() > SYNC_MAX_MANIFEST_HEADS
        {
            return Err(SyncSessionError::Protocol);
        }
        let mut remote_heads = Vec::new();
        remote_heads
            .try_reserve_exact(manifest.heads.len())
            .map_err(|_| SyncSessionError::Capacity)?;
        let mut by_revision = BTreeMap::new();
        for head in manifest.heads {
            let parsed = RevisionHead {
                record_id: fixed::<16>(&head.record_id)?,
                revision_id: fixed::<16>(&head.revision_id)?,
                revision_hash: fixed::<32>(&head.revision_hash)?,
            };
            if is_zero_id(&parsed.record_id)
                || is_zero_id(&parsed.revision_id)
                || parsed.revision_hash.iter().all(|byte| *byte == 0)
                || by_revision.insert(parsed.revision_id, parsed).is_some()
            {
                return Err(SyncSessionError::Protocol);
            }
            if let Some(local) = self
                .ledger
                .get(parsed.revision_id)
                .map_err(map_sync_error)?
            {
                if local.record_id != parsed.record_id
                    || local.hash().map_err(map_sync_error)? != parsed.revision_hash
                {
                    return Err(SyncSessionError::Protocol);
                }
            }
            remote_heads.push(parsed);
        }
        self.remote_snapshot_id = Some(fixed::<16>(&manifest.snapshot_id)?);
        self.remote_manifest = remote_heads;
        self.remote_heads_by_revision = by_revision;
        self.remote_manifest_received = true;

        if self.local_is_smaller {
            self.serve_started = true;
        } else {
            self.begin_receive_phase()?;
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
        if request.revisions.len() > SESSION_MAX_REQUESTED_HEADS {
            return Err(SyncSessionError::Capacity);
        }
        if request.revisions.is_empty() {
            self.serve_done = true;
            self.request_record_ids.clear();
            if self.local_is_smaller {
                self.begin_receive_phase()?;
            }
            self.refresh_completion();
            return Ok(());
        }

        let mut requested_ids = BTreeSet::new();
        let mut requested_records = BTreeSet::new();
        for requested in request.revisions {
            let record_id = fixed::<16>(&requested.record_id)?;
            let revision_id = fixed::<16>(&requested.revision_id)?;
            if is_zero_id(&record_id)
                || is_zero_id(&revision_id)
                || !requested_ids.insert(revision_id)
            {
                return Err(SyncSessionError::Protocol);
            }
            let advertised = snapshot
                .heads_by_revision
                .get(&revision_id)
                .ok_or(SyncSessionError::Protocol)?;
            if advertised.record_id != record_id {
                return Err(SyncSessionError::Protocol);
            }
            requested_records.insert(record_id);
        }
        let plan = self.ancestor_closure_plan(snapshot, &requested_ids)?;
        if plan.is_empty() {
            return Err(SyncSessionError::Protocol);
        }
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
        transfer
            .encoded
            .try_reserve(chunk.data.len())
            .map_err(|_| SyncSessionError::Capacity)?;
        transfer.encoded.extend_from_slice(&chunk.data);
        transfer.next_chunk += 1;
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
        let (vault_id, revisions) = decode_batch(&transfer.encoded).map_err(map_sync_error)?;
        if vault_id != self.ledger.vault_id()
            || revisions.is_empty()
            || revisions.len() > SYNC_MAX_REVISIONS_PER_BATCH
        {
            return Err(SyncSessionError::Protocol);
        }
        let projected_count = self
            .received_revision_count
            .checked_add(revisions.len())
            .ok_or(SyncSessionError::Capacity)?;
        if projected_count > SESSION_MAX_KNOWN_REVISIONS {
            return Err(SyncSessionError::Capacity);
        }
        let mut batch_ids = HashSet::with_capacity(revisions.len());
        let mut new_ids = Vec::with_capacity(revisions.len());
        for revision in &revisions {
            if !self.request_record_ids.contains(&revision.record_id)
                || !batch_ids.insert(revision.revision_id)
            {
                return Err(SyncSessionError::Protocol);
            }
            if self.inbound_revision_ids.contains(&revision.revision_id) {
                return Err(SyncSessionError::Protocol);
            }
            if let Some(head) = self.remote_heads_by_revision.get(&revision.revision_id) {
                if head.record_id != revision.record_id
                    || revision.hash().map_err(map_sync_error)? != head.revision_hash
                {
                    return Err(SyncSessionError::Protocol);
                }
            }
            new_ids.push(revision.revision_id);
        }
        // The ledger insertion performs signature, author, graph and vault
        // validation atomically. No accepted ACK is queued before it returns.
        self.ledger
            .insert_batch(&revisions, self.trust.as_ref())
            .map_err(map_sync_error)?;
        self.received_revision_count = projected_count;
        self.inbound_revision_ids.extend(new_ids);
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
        for revision_id in transfer.revision_ids {
            if self.acknowledged_revision_ids.insert(revision_id) {
                self.acknowledged_revision_count =
                    self.acknowledged_revision_count.saturating_add(1);
            }
        }
        self.awaiting_ack = None;
        if !self.outbound_plan.is_empty() {
            self.prepare_next_outbound_batch()?;
        }
        Ok(())
    }

    fn freeze_local_snapshot(&self) -> Result<FrozenSnapshot, SyncSessionError> {
        let history = self.ledger.history_index().map_err(map_sync_error)?;
        if history.len() > SESSION_MAX_KNOWN_REVISIONS {
            return Err(SyncSessionError::Capacity);
        }
        let manifest = self.ledger.manifest().map_err(map_sync_error)?;
        if manifest.len() > SYNC_MAX_MANIFEST_HEADS {
            return Err(SyncSessionError::Capacity);
        }
        let mut index = BTreeMap::new();
        for meta in history {
            if is_zero_id(&meta.record_id)
                || is_zero_id(&meta.revision_id)
                || meta.revision_hash.iter().all(|byte| *byte == 0)
                || index.insert(meta.revision_id, meta).is_some()
            {
                return Err(SyncSessionError::Protocol);
            }
        }
        let mut heads_by_revision = BTreeMap::new();
        for head in &manifest {
            if is_zero_id(&head.record_id)
                || is_zero_id(&head.revision_id)
                || head.revision_hash.iter().all(|byte| *byte == 0)
                || heads_by_revision.insert(head.revision_id, *head).is_some()
            {
                return Err(SyncSessionError::Protocol);
            }
            let indexed = index
                .get(&head.revision_id)
                .ok_or(SyncSessionError::Protocol)?;
            if indexed.record_id != head.record_id || indexed.revision_hash != head.revision_hash {
                return Err(SyncSessionError::Protocol);
            }
        }
        let meta_values = index.values().map(copy_meta).collect::<Vec<_>>();
        let topological_ids = topological_meta_order(&meta_values).map_err(map_sync_error)?;
        if topological_ids.len() != index.len() {
            return Err(SyncSessionError::Protocol);
        }
        Ok(FrozenSnapshot {
            snapshot_id: fresh_id().map_err(map_sync_error)?,
            manifest,
            heads_by_revision,
            index,
            topological_ids,
        })
    }

    fn ancestor_closure_plan(
        &self,
        snapshot: &FrozenSnapshot,
        requested_ids: &BTreeSet<Id>,
    ) -> Result<Vec<Id>, SyncSessionError> {
        let mut closure = HashSet::new();
        let mut stack = Vec::new();
        for revision_id in requested_ids {
            stack.push(*revision_id);
        }
        while let Some(revision_id) = stack.pop() {
            if !closure.insert(revision_id) {
                continue;
            }
            if closure.len() > snapshot.index.len() {
                return Err(SyncSessionError::Capacity);
            }
            let meta = snapshot
                .index
                .get(&revision_id)
                .ok_or(SyncSessionError::Protocol)?;
            for parent_id in &meta.parent_revision_ids {
                let parent = snapshot
                    .index
                    .get(parent_id)
                    .ok_or(SyncSessionError::Protocol)?;
                if parent.record_id != meta.record_id {
                    return Err(SyncSessionError::Protocol);
                }
                stack.push(*parent_id);
            }
        }
        let mut ordered = Vec::new();
        for revision_id in &snapshot.topological_ids {
            if closure.contains(revision_id)
                && !self.acknowledged_revision_ids.contains(revision_id)
            {
                ordered.push(*revision_id);
            }
        }
        Ok(ordered)
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
        let mut revisions = Vec::new();
        let mut revision_ids = Vec::new();
        let mut author_device_ids = Vec::new();
        let mut estimated_bytes = 0_usize;
        for revision_id in self.outbound_plan.iter().take(SYNC_MAX_REVISIONS_PER_BATCH) {
            let meta = snapshot
                .index
                .get(revision_id)
                .ok_or(SyncSessionError::Protocol)?;
            let revision = self
                .ledger
                .get(*revision_id)
                .map_err(map_sync_error)?
                .ok_or(SyncSessionError::Storage)?;
            if revision.record_id != meta.record_id
                || revision.hash().map_err(map_sync_error)? != meta.revision_hash
            {
                return Err(SyncSessionError::Protocol);
            }
            let author = self
                .trust
                .lookup(revision.author_device_id)
                .map_err(map_sync_error)?
                .ok_or(SyncSessionError::UnknownAuthor)?;
            if author.device_id != revision.author_device_id
                || author.validate().is_err()
                || revision.verify(&author).is_err()
            {
                return Err(SyncSessionError::Protocol);
            }
            let one_encoded = encode_batch(self.ledger.vault_id(), std::slice::from_ref(&revision))
                .map_err(map_sync_error)?;
            if one_encoded.len() > SYNC_MAX_BATCH_BYTES {
                return Err(SyncSessionError::Capacity);
            }
            if !revisions.is_empty()
                && estimated_bytes
                    .checked_add(one_encoded.len())
                    .is_none_or(|size| size > SYNC_MAX_BATCH_BYTES)
            {
                break;
            }
            estimated_bytes = estimated_bytes
                .checked_add(one_encoded.len())
                .ok_or(SyncSessionError::Capacity)?;
            revision_ids.push(*revision_id);
            author_device_ids.push(revision.author_device_id);
            revisions.push(revision);
        }
        if revisions.is_empty() {
            return Err(SyncSessionError::Capacity);
        }
        let encoded = encode_batch(self.ledger.vault_id(), &revisions).map_err(map_sync_error)?;
        if encoded.is_empty() || encoded.len() > SYNC_MAX_BATCH_BYTES {
            return Err(SyncSessionError::Capacity);
        }
        for _ in 0..revision_ids.len() {
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
        let chunk_count = encoded
            .len()
            .checked_add(SESSION_CHUNK_BYTES - 1)
            .ok_or(SyncSessionError::Capacity)?
            / SESSION_CHUNK_BYTES;
        self.outbound_transfer = Some(OutboundTransfer {
            transfer_id,
            digest,
            encoded,
            chunk_count,
            next_chunk: 0,
            commit_sent: false,
            revision_ids,
            author_device_ids,
        });
        Ok(())
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
            match self.ledger.get(head.revision_id).map_err(map_sync_error)? {
                Some(revision) => {
                    if revision.record_id != head.record_id
                        || revision.hash().map_err(map_sync_error)? != head.revision_hash
                    {
                        return Err(SyncSessionError::Protocol);
                    }
                }
                None => missing.push(*head),
            }
        }
        if missing.is_empty() {
            self.queue_control(SyncOutbound::Request(proto::SyncRequest {
                snapshot_id: self
                    .remote_snapshot_id
                    .ok_or(SyncSessionError::Protocol)?
                    .to_vec(),
                revisions: Vec::new(),
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
            snapshot_id: self
                .remote_snapshot_id
                .ok_or(SyncSessionError::Protocol)?
                .to_vec(),
            revisions: window
                .iter()
                .map(|head| proto::RequestedRevision {
                    record_id: head.record_id.to_vec(),
                    revision_id: head.revision_id.to_vec(),
                })
                .collect(),
        }))
    }

    fn active_window_is_present(&self) -> Result<bool, SyncSessionError> {
        if self.active_window.is_empty() {
            return Ok(false);
        }
        for head in &self.active_window {
            let Some(revision) = self.ledger.get(head.revision_id).map_err(map_sync_error)? else {
                return Ok(false);
            };
            if revision.record_id != head.record_id
                || revision.hash().map_err(map_sync_error)? != head.revision_hash
            {
                return Err(SyncSessionError::Protocol);
            }
        }
        Ok(true)
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
        if self.control_queue.len() >= MAX_PENDING_CONTROL {
            return Err(SyncSessionError::Capacity);
        }
        self.control_queue.push_back(message);
        Ok(())
    }

    fn ensure_peer_trusted(&self) -> Result<(), SyncSessionError> {
        let trusted = self
            .trust
            .lookup(self.expected_peer.device_id)
            .map_err(map_sync_error)?;
        if trusted == Some(self.expected_peer) {
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
                .lookup(*author_device_id)
                .map_err(map_sync_error)?
                .ok_or(SyncSessionError::UnknownAuthor)?;
            if author.device_id != *author_device_id || author.validate().is_err() {
                return Err(SyncSessionError::Protocol);
            }
        }
        Ok(())
    }

    fn refresh_completion(&mut self) {
        self.complete = self.receive_done && self.serve_done;
        if self.complete {
            self.control_queue.clear();
            self.outbound_plan.clear();
            self.outbound_transfer = None;
            self.inbound_transfer = None;
            self.active_window.clear();
        }
    }

    fn fail<T>(&mut self, error: SyncSessionError) -> Result<T, SyncSessionError> {
        self.failed = true;
        self.control_queue.clear();
        self.outbound_plan.clear();
        self.outbound_transfer = None;
        self.inbound_transfer = None;
        self.active_window.clear();
        Err(error)
    }
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], SyncSessionError> {
    bytes.try_into().map_err(|_| SyncSessionError::Protocol)
}

fn copy_meta(meta: &RevisionMeta) -> RevisionMeta {
    RevisionMeta {
        record_id: meta.record_id,
        revision_id: meta.revision_id,
        revision_hash: meta.revision_hash,
        parent_revision_ids: meta.parent_revision_ids.clone(),
        ciphertext_len: meta.ciphertext_len,
    }
}
