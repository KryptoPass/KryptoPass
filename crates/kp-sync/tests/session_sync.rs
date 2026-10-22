#![allow(clippy::expect_used)] // Event-driven test steps name their protocol boundary.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_protocol::{MessagePayload, proto};
use kryptopass_sync::{
    Id, Ledger, Operation, SyncError, SyncProgressState, SyncSession, SyncSessionError, TrustSource,
};
use rusqlite::Connection;

const VAULT: Id = [0x39; 16];
const FINGERPRINT: [u8; 20] = [0x42; 20];

#[derive(Clone)]
struct TestTrust(HashMap<DeviceId, PublicIdentity>);

impl TestTrust {
    fn for_pair(small: &DeviceIdentity, large: &DeviceIdentity) -> Self {
        Self(HashMap::from([
            (small.device_id(), small.public_identity()),
            (large.device_id(), large.public_identity()),
        ]))
    }
}

impl TrustSource for TestTrust {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok(self.0.get(&device_id).copied())
    }
}

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let suffix = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kp-sync-session-{label}-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create isolated session test directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("make session test directory private");
        }
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Sessions {
    small: SyncSession,
    large: SyncSession,
    now: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FrameKind {
    Hello,
    Manifest {
        snapshot_id: Vec<u8>,
    },
    Request {
        empty: bool,
        snapshot_id: Vec<u8>,
    },
    Chunk {
        transfer_id: Vec<u8>,
        index: u32,
        chunk_count: u32,
        digest: Vec<u8>,
        data_len: usize,
    },
    Commit {
        transfer_id: Vec<u8>,
        digest: Vec<u8>,
    },
    Ack {
        transfer_id: Vec<u8>,
        status: i32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Frame {
    from_small: bool,
    kind: FrameKind,
}

fn identities_in_device_order() -> (DeviceIdentity, DeviceIdentity) {
    let first = DeviceIdentity::from_seed_material(&[0x31; 32], &[0x32; 32]);
    let second = DeviceIdentity::from_seed_material(&[0x41; 32], &[0x42; 32]);
    if first.device_id() < second.device_id() {
        (first, second)
    } else {
        (second, first)
    }
}

fn in_memory_sessions(
    small: &DeviceIdentity,
    large: &DeviceIdentity,
    small_ledger: Ledger,
    large_ledger: Ledger,
) -> Sessions {
    let small_trust = TestTrust::for_pair(small, large);
    let large_trust = TestTrust::for_pair(small, large);
    let now = Instant::now();
    let small_session = SyncSession::new(
        small_ledger,
        Box::new(small_trust),
        small.device_id(),
        large.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("explicitly bound small Sync engine");
    let large_session = SyncSession::new(
        large_ledger,
        Box::new(large_trust),
        large.device_id(),
        small.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("explicitly bound large Sync engine");
    Sessions {
        small: small_session,
        large: large_session,
        now,
    }
}

fn summary(from_small: bool, _message_type: proto::MessageType, payload: &MessagePayload) -> Frame {
    let kind = match payload {
        MessagePayload::SyncHello(_) => FrameKind::Hello,
        MessagePayload::SyncManifest(manifest) => FrameKind::Manifest {
            snapshot_id: manifest.snapshot_id.clone(),
        },
        MessagePayload::SyncRequest(request) => FrameKind::Request {
            empty: request.revisions.is_empty(),
            snapshot_id: request.snapshot_id.clone(),
        },
        MessagePayload::SyncChunk(chunk) => FrameKind::Chunk {
            transfer_id: chunk.transfer_id.clone(),
            index: chunk.chunk_index,
            chunk_count: chunk.chunk_count,
            digest: chunk.sha256_digest.clone(),
            data_len: chunk.data.len(),
        },
        MessagePayload::SyncCommit(commit) => FrameKind::Commit {
            transfer_id: commit.transfer_id.clone(),
            digest: commit.sha256_digest.clone(),
        },
        MessagePayload::SyncAck(ack) => FrameKind::Ack {
            transfer_id: ack.transfer_id.clone(),
            status: ack.status,
        },
        _ => panic!("the Sync owner emitted an unrelated protocol message"),
    };
    Frame { from_small, kind }
}

fn run_to_completion(sessions: &mut Sessions) -> Vec<Frame> {
    let mut trace = Vec::new();
    for _ in 0..20_000 {
        if sessions.small.is_complete() && sessions.large.is_complete() {
            return trace;
        }
        let mut advanced = false;
        if let Some(outbound) = sessions
            .small
            .poll_output(sessions.now)
            .expect("small Sync owner polls one frame")
        {
            let (message_type, payload) = outbound.into_message();
            trace.push(summary(true, message_type, &payload));
            sessions
                .large
                .handle_message(message_type, payload, sessions.now)
                .expect("large Sync owner accepts the routed frame");
            advanced = true;
        }
        if let Some(outbound) = sessions
            .large
            .poll_output(sessions.now)
            .expect("large Sync owner polls one frame")
        {
            let (message_type, payload) = outbound.into_message();
            trace.push(summary(false, message_type, &payload));
            sessions
                .small
                .handle_message(message_type, payload, sessions.now)
                .expect("small Sync owner accepts the routed frame");
            advanced = true;
        }
        assert!(
            advanced,
            "both peers should exchange one frame until completion"
        );
    }
    panic!("bounded Sync round did not complete");
}

#[test]
fn empty_ledgers_complete_with_empty_requests_and_no_transfer_ack() {
    let (small_identity, large_identity) = identities_in_device_order();
    let mut sessions = in_memory_sessions(
        &small_identity,
        &large_identity,
        Ledger::in_memory(VAULT).expect("small isolated ledger"),
        Ledger::in_memory(VAULT).expect("large isolated ledger"),
    );

    let trace = run_to_completion(&mut sessions);

    assert_eq!(sessions.small.progress().state, SyncProgressState::Complete);
    assert_eq!(sessions.large.progress().state, SyncProgressState::Complete);
    assert_eq!(sessions.small.progress().received_revisions, 0);
    assert_eq!(sessions.large.progress().received_revisions, 0);
    let small_snapshot = trace
        .iter()
        .find_map(|frame| match &frame.kind {
            FrameKind::Manifest { snapshot_id } if frame.from_small => Some(snapshot_id),
            _ => None,
        })
        .expect("small manifest has a fresh snapshot");
    let large_snapshot = trace
        .iter()
        .find_map(|frame| match &frame.kind {
            FrameKind::Manifest { snapshot_id } if !frame.from_small => Some(snapshot_id),
            _ => None,
        })
        .expect("large manifest has a fresh snapshot");
    let empty_requests: Vec<_> = trace
        .iter()
        .filter_map(|frame| match &frame.kind {
            FrameKind::Request {
                empty: true,
                snapshot_id,
            } => Some((frame.from_small, snapshot_id)),
            _ => None,
        })
        .collect();
    assert_eq!(empty_requests.len(), 2);
    assert!(
        empty_requests
            .iter()
            .any(|(from_small, snapshot)| *from_small && *snapshot == large_snapshot)
    );
    assert!(
        empty_requests
            .iter()
            .any(|(from_small, snapshot)| !*from_small && *snapshot == small_snapshot)
    );
    assert!(trace.iter().all(|frame| !matches!(
        &frame.kind,
        FrameKind::Chunk { .. } | FrameKind::Commit { .. } | FrameKind::Ack { .. }
    )));
}

#[test]
fn remote_manifest_may_arrive_before_queued_local_manifest_is_polled() {
    let (small_identity, large_identity) = identities_in_device_order();
    let mut sessions = in_memory_sessions(
        &small_identity,
        &large_identity,
        Ledger::in_memory(VAULT).expect("small isolated ledger"),
        Ledger::in_memory(VAULT).expect("large isolated ledger"),
    );
    let now = sessions.now;

    let small_hello = sessions
        .small
        .poll_output(now)
        .expect("small hello poll")
        .expect("small SyncHello is queued");
    let large_hello = sessions
        .large
        .poll_output(now)
        .expect("large hello poll")
        .expect("large SyncHello is queued");
    let (small_type, small_payload) = small_hello.into_message();
    let (large_type, large_payload) = large_hello.into_message();
    assert!(small_type == proto::MessageType::SyncHello);
    assert!(large_type == proto::MessageType::SyncHello);
    sessions
        .small
        .handle_message(large_type, large_payload, now)
        .expect("small accepts the authenticated peer hello");
    sessions
        .large
        .handle_message(small_type, small_payload, now)
        .expect("large accepts the authenticated peer hello");

    let remote_manifest = sessions
        .large
        .poll_output(now)
        .expect("large manifest poll")
        .expect("large queued its frozen manifest");
    let (message_type, payload) = remote_manifest.into_message();
    assert!(message_type == proto::MessageType::SyncManifest);
    sessions
        .small
        .handle_message(message_type, payload, now)
        .expect("remote manifest can arrive before draining local output");

    let local_manifest = sessions
        .small
        .poll_output(now)
        .expect("small manifest poll")
        .expect("small local manifest remains queued behind its hello");
    let (message_type, payload) = local_manifest.into_message();
    assert!(message_type == proto::MessageType::SyncManifest);
    sessions
        .large
        .handle_message(message_type, payload, now)
        .expect("large accepts the queued small manifest");

    let large_request = sessions
        .large
        .poll_output(now)
        .expect("large empty request poll")
        .expect("larger endpoint requests receive completion");
    let (message_type, payload) = large_request.into_message();
    assert!(message_type == proto::MessageType::SyncRequest);
    assert!(
        matches!(&payload, MessagePayload::SyncRequest(request) if request.revisions.is_empty())
    );
    sessions
        .small
        .handle_message(message_type, payload, now)
        .expect("small observes the peer receive phase completion");

    let small_request = sessions
        .small
        .poll_output(now)
        .expect("small request poll")
        .expect("small requests the peer's empty snapshot");
    let (message_type, payload) = small_request.into_message();
    assert!(message_type == proto::MessageType::SyncRequest);
    assert!(
        matches!(&payload, MessagePayload::SyncRequest(request) if request.revisions.is_empty())
    );
    sessions
        .large
        .handle_message(message_type, payload, now)
        .expect("large observes the small receive phase completion");
    assert!(sessions.small.is_complete());
    assert!(sessions.large.is_complete());
}

#[test]
fn smaller_device_serves_first_and_transfers_the_full_missing_ancestor_closure() {
    let (small_identity, large_identity) = identities_in_device_order();
    let trust = TestTrust::for_pair(&small_identity, &large_identity);
    let mut small_ledger = Ledger::in_memory(VAULT).expect("small isolated ledger");
    let large_ledger = Ledger::in_memory(VAULT).expect("large isolated ledger");
    let mut parent = Vec::new();
    for index in 0..5_u8 {
        let revision = small_ledger
            .append_local(
                &small_identity,
                [0x71; 16],
                &parent,
                Operation::Write,
                vec![index.saturating_add(1); 32],
                &trust,
            )
            .expect("append a signed causal revision");
        parent.clear();
        parent.push(revision.revision_id);
    }
    let mut sessions =
        in_memory_sessions(&small_identity, &large_identity, small_ledger, large_ledger);

    let trace = run_to_completion(&mut sessions);

    assert_eq!(sessions.small.progress().acknowledged_revisions, 5);
    assert_eq!(sessions.large.progress().received_revisions, 5);
    let first_chunk = trace
        .iter()
        .find(|frame| matches!(&frame.kind, FrameKind::Chunk { .. }))
        .expect("missing ancestor closure uses data frames");
    assert!(first_chunk.from_small, "the smaller device serves first");
    let first_request = trace
        .iter()
        .find(|frame| matches!(&frame.kind, FrameKind::Request { empty: false, .. }))
        .expect("larger device requests the smaller snapshot first");
    assert!(!first_request.from_small);
}

#[test]
fn fragmented_multibatch_transfer_waits_for_accepted_commit_ack() {
    let (small_identity, large_identity) = identities_in_device_order();
    let trust = TestTrust::for_pair(&small_identity, &large_identity);
    let mut small_ledger = Ledger::in_memory(VAULT).expect("small isolated ledger");
    let large_ledger = Ledger::in_memory(VAULT).expect("large isolated ledger");
    let mut parents = Vec::new();
    for index in 0..40_u8 {
        let ciphertext = if index == 0 {
            vec![0x5a; 110_000]
        } else {
            vec![index; 64]
        };
        let revision = small_ledger
            .append_local(
                &small_identity,
                [0x72; 16],
                &parents,
                Operation::Write,
                ciphertext,
                &trust,
            )
            .expect("append revision into isolated source ledger");
        parents.clear();
        parents.push(revision.revision_id);
    }
    let mut sessions =
        in_memory_sessions(&small_identity, &large_identity, small_ledger, large_ledger);

    let trace = run_to_completion(&mut sessions);

    assert_eq!(sessions.large.progress().received_revisions, 40);
    assert_eq!(sessions.small.progress().acknowledged_revisions, 40);
    let chunks: Vec<_> = trace
        .iter()
        .filter_map(|frame| match &frame.kind {
            FrameKind::Chunk {
                transfer_id,
                index,
                chunk_count,
                digest,
                data_len,
            } => Some((transfer_id, *index, *chunk_count, digest, *data_len)),
            _ => None,
        })
        .collect();
    assert!(
        chunks.len() > 3,
        "the first encoded batch must be fragmented"
    );
    let transfer_ids: std::collections::BTreeSet<_> = chunks
        .iter()
        .map(|(id, _, _, _, _)| id.as_slice())
        .collect();
    assert_eq!(transfer_ids.len(), 2, "40 revisions use a 32+8 batch plan");
    for transfer_id in &transfer_ids {
        let fragments: Vec<_> = chunks
            .iter()
            .filter(|(id, _, _, _, _)| id.as_slice() == *transfer_id)
            .collect();
        let count = fragments[0].2 as usize;
        assert_eq!(fragments.len(), count);
        assert_eq!(
            fragments
                .iter()
                .map(|fragment| fragment.1)
                .collect::<Vec<_>>(),
            (0..count as u32).collect::<Vec<_>>()
        );
        assert!(
            fragments
                .iter()
                .all(|fragment| fragment.3 == fragments[0].3)
        );
        assert!(
            fragments[..count - 1]
                .iter()
                .all(|fragment| fragment.4 == 48 * 1024)
        );
        assert!((1..=48 * 1024).contains(&fragments[count - 1].4));
    }
    let commits: Vec<_> = trace
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| match &frame.kind {
            FrameKind::Commit {
                transfer_id,
                digest,
            } => Some((index, transfer_id, digest)),
            _ => None,
        })
        .collect();
    let ack_positions: Vec<_> = trace
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| match &frame.kind {
            FrameKind::Ack { status, .. } => {
                assert_eq!(*status, proto::AckStatus::Accepted as i32);
                Some(index)
            }
            _ => None,
        })
        .collect();
    assert_eq!(commits.len(), 2);
    assert_eq!(ack_positions.len(), 2);
    assert!(commits[0].0 < ack_positions[0] && ack_positions[0] < commits[1].0);
    let acknowledgements: Vec<_> = trace
        .iter()
        .filter_map(|frame| match &frame.kind {
            FrameKind::Ack {
                transfer_id,
                status,
            } => Some((transfer_id, status)),
            _ => None,
        })
        .collect();
    for ((_, transfer_id, digest), (ack_transfer_id, _)) in commits.iter().zip(&acknowledgements) {
        assert_eq!(*transfer_id, *ack_transfer_id);
        let chunk_digest = chunks
            .iter()
            .find(|(chunk_transfer_id, ..)| chunk_transfer_id.as_slice() == transfer_id.as_slice())
            .map(|chunk| chunk.3)
            .expect("each committed batch has fragments");
        assert_eq!(*digest, chunk_digest);
    }
    let first_transfer_id = chunks[0].0.as_slice();
    let second_transfer_id = chunks
        .iter()
        .find(|(transfer_id, _, _, _, _)| transfer_id.as_slice() != first_transfer_id)
        .expect("second batch has a fresh transfer ID")
        .0
        .as_slice();
    let second_transfer_first_chunk = trace
        .iter()
        .position(|frame| match &frame.kind {
            FrameKind::Chunk { transfer_id, .. } => transfer_id.as_slice() == second_transfer_id,
            _ => false,
        })
        .expect("second batch starts after its accepted prior ack");
    assert!(ack_positions[0] < second_transfer_first_chunk);
}

#[test]
fn parallel_offline_edits_survive_as_conflict_heads_on_both_ledgers() {
    let (small_identity, large_identity) = identities_in_device_order();
    let trust = TestTrust::for_pair(&small_identity, &large_identity);
    let directory = TempDirectory::new("conflicts");
    let small_path = directory.file("small.sqlite");
    let large_path = directory.file("large.sqlite");
    let mut small_ledger = Ledger::create(&small_path, VAULT).expect("private small ledger");
    let mut large_ledger = Ledger::create(&large_path, VAULT).expect("private large ledger");
    let root = small_ledger
        .append_local(
            &small_identity,
            [0x73; 16],
            &[],
            Operation::Write,
            b"shared root".to_vec(),
            &trust,
        )
        .expect("small device creates common root");
    large_ledger
        .insert_batch(std::slice::from_ref(&root), &trust)
        .expect("large device receives common root before going offline");
    let small_edit = small_ledger
        .append_local(
            &small_identity,
            [0x73; 16],
            &[root.revision_id],
            Operation::Write,
            b"small offline edit".to_vec(),
            &trust,
        )
        .expect("small device edits offline");
    let large_edit = large_ledger
        .append_local(
            &large_identity,
            [0x73; 16],
            &[root.revision_id],
            Operation::Write,
            b"large offline edit".to_vec(),
            &trust,
        )
        .expect("large device edits offline");
    small_ledger
        .insert_batch(std::slice::from_ref(&large_edit), &trust)
        .expect("small device imports the other branch");
    large_ledger
        .insert_batch(std::slice::from_ref(&small_edit), &trust)
        .expect("large device imports the other branch");
    let mut sessions =
        in_memory_sessions(&small_identity, &large_identity, small_ledger, large_ledger);

    run_to_completion(&mut sessions);
    drop(sessions);

    let expected = [small_edit.revision_id, large_edit.revision_id];
    for path in [&small_path, &large_path] {
        let ledger = Ledger::open_existing(path, VAULT, &trust).expect("reopen committed history");
        let heads = ledger
            .heads([0x73; 16])
            .expect("read offline conflict heads");
        assert_eq!(heads.len(), 2);
        let actual: std::collections::BTreeSet<_> =
            heads.iter().map(|head| head.revision_id).collect();
        let expected: std::collections::BTreeSet<_> = expected.iter().copied().collect();
        assert_eq!(actual, expected);
        assert_eq!(ledger.revision_count().expect("durable revision count"), 3);
    }
}

#[test]
fn mismatched_vault_fingerprint_or_device_rejects_the_peer_hello() {
    let (small_identity, large_identity) = identities_in_device_order();
    let trust = TestTrust::for_pair(&small_identity, &large_identity);
    let now = Instant::now();

    let mut small = SyncSession::new(
        Ledger::in_memory(VAULT).expect("small vault"),
        Box::new(trust.clone()),
        small_identity.device_id(),
        large_identity.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("small bound engine");
    let mut wrong_vault = SyncSession::new(
        Ledger::in_memory([0x3a; 16]).expect("different isolated vault"),
        Box::new(trust.clone()),
        large_identity.device_id(),
        small_identity.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("other bound engine");
    wrong_vault
        .poll_output(now)
        .expect("local hello is sent before validating its peer");
    let hello = small
        .poll_output(now)
        .expect("hello output")
        .expect("queued hello");
    let (message_type, payload) = hello.into_message();
    assert_eq!(
        wrong_vault.handle_message(message_type, payload, now),
        Err(SyncSessionError::InvalidBinding)
    );
    assert_eq!(wrong_vault.progress().state, SyncProgressState::Failed);

    let mut wrong_fingerprint = SyncSession::new(
        Ledger::in_memory(VAULT).expect("peer vault"),
        Box::new(trust.clone()),
        large_identity.device_id(),
        small_identity.public_identity(),
        [0x44; 32].to_vec(),
        now,
    )
    .expect("fingerprint is a valid configured length");
    wrong_fingerprint
        .poll_output(now)
        .expect("local hello is sent before validating its peer");
    let mut small = SyncSession::new(
        Ledger::in_memory(VAULT).expect("small vault"),
        Box::new(trust.clone()),
        small_identity.device_id(),
        large_identity.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("small bound engine");
    let (message_type, payload) = small
        .poll_output(now)
        .expect("hello output")
        .expect("queued hello")
        .into_message();
    assert_eq!(
        wrong_fingerprint.handle_message(message_type, payload, now),
        Err(SyncSessionError::InvalidBinding)
    );

    let mut wrong_device = SyncSession::new(
        Ledger::in_memory(VAULT).expect("peer vault"),
        Box::new(trust),
        large_identity.device_id(),
        small_identity.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("bound peer identity");
    wrong_device
        .poll_output(now)
        .expect("local hello is sent before validating its peer");
    let malformed_claim = proto::SyncHello {
        vault_id: VAULT.to_vec(),
        device_id: [0x7f; 32].to_vec(),
        known_revision_count: 0,
        openpgp_fingerprint: FINGERPRINT.to_vec(),
    };
    assert_eq!(
        wrong_device.handle_message(
            proto::MessageType::SyncHello,
            MessagePayload::SyncHello(malformed_claim),
            now,
        ),
        Err(SyncSessionError::InvalidBinding)
    );
}

#[test]
fn replayed_hello_and_unsolicited_chunk_fail_closed() {
    let (small_identity, large_identity) = identities_in_device_order();
    let trust = TestTrust::for_pair(&small_identity, &large_identity);
    let now = Instant::now();
    let mut receiver = SyncSession::new(
        Ledger::in_memory(VAULT).expect("isolated ledger"),
        Box::new(trust.clone()),
        large_identity.device_id(),
        small_identity.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("receiver binding");
    receiver.poll_output(now).expect("local hello output");
    let remote_hello = proto::SyncHello {
        vault_id: VAULT.to_vec(),
        device_id: small_identity.device_id().as_bytes().to_vec(),
        known_revision_count: 0,
        openpgp_fingerprint: FINGERPRINT.to_vec(),
    };
    receiver
        .handle_message(
            proto::MessageType::SyncHello,
            MessagePayload::SyncHello(remote_hello.clone()),
            now,
        )
        .expect("first peer hello binds");
    assert_eq!(
        receiver.handle_message(
            proto::MessageType::SyncHello,
            MessagePayload::SyncHello(remote_hello),
            now,
        ),
        Err(SyncSessionError::Protocol)
    );

    let mut unsolicited = SyncSession::new(
        Ledger::in_memory(VAULT).expect("another isolated ledger"),
        Box::new(trust),
        large_identity.device_id(),
        small_identity.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("receiver binding");
    unsolicited.poll_output(now).expect("local hello output");
    assert_eq!(
        unsolicited.handle_message(
            proto::MessageType::SyncChunk,
            MessagePayload::SyncChunk(proto::SyncChunk {
                transfer_id: [0x22; 16].to_vec(),
                chunk_index: 0,
                chunk_count: 1,
                total_size: 1,
                sha256_digest: [0x23; 32].to_vec(),
                data: vec![0x24],
            }),
            now,
        ),
        Err(SyncSessionError::Protocol)
    );
}

#[test]
fn invalid_chunk_digest_is_rejected_before_storage_or_ack() {
    let (small_identity, large_identity) = identities_in_device_order();
    let trust = TestTrust::for_pair(&small_identity, &large_identity);
    let mut small_ledger = Ledger::in_memory(VAULT).expect("small isolated ledger");
    let large_ledger = Ledger::in_memory(VAULT).expect("large isolated ledger");
    small_ledger
        .append_local(
            &small_identity,
            [0x74; 16],
            &[],
            Operation::Write,
            b"synthetic bad digest".to_vec(),
            &trust,
        )
        .expect("source revision");
    let mut sessions =
        in_memory_sessions(&small_identity, &large_identity, small_ledger, large_ledger);
    let now = sessions.now;
    let mut trace = Vec::new();
    let mut corrupted = false;

    for _ in 0..500 {
        if sessions.large.progress().state == SyncProgressState::Failed {
            break;
        }
        let mut advanced = false;
        if let Some(outbound) = sessions.small.poll_output(now).expect("small output") {
            let (message_type, mut payload) = outbound.into_message();
            if let MessagePayload::SyncChunk(ref mut chunk) = payload {
                if !corrupted {
                    chunk.data[0] ^= 0x01;
                    corrupted = true;
                }
            }
            trace.push(summary(true, message_type, &payload));
            let result = sessions.large.handle_message(message_type, payload, now);
            if let Err(error) = result {
                assert_eq!(error, SyncSessionError::Protocol);
                break;
            }
            advanced = true;
        }
        if let Some(outbound) = sessions.large.poll_output(now).expect("large output") {
            let (message_type, payload) = outbound.into_message();
            trace.push(summary(false, message_type, &payload));
            let result = sessions.small.handle_message(message_type, payload, now);
            if let Err(error) = result {
                assert_eq!(error, SyncSessionError::Protocol);
                break;
            }
            advanced = true;
        }
        assert!(advanced, "bad digest test should reach the commit");
    }

    assert!(corrupted, "the test must mutate an in-flight fragment");
    assert_eq!(sessions.large.progress().state, SyncProgressState::Failed);
    assert_eq!(sessions.large.progress().received_revisions, 0);
    assert!(
        trace
            .iter()
            .all(|frame| !matches!(&frame.kind, FrameKind::Ack { .. }))
    );
}

#[test]
fn storage_failure_sends_no_successful_ack() {
    let (small_identity, large_identity) = identities_in_device_order();
    let trust = TestTrust::for_pair(&small_identity, &large_identity);
    let mut source = Ledger::in_memory(VAULT).expect("small isolated ledger");
    source
        .append_local(
            &small_identity,
            [0x75; 16],
            &[],
            Operation::Write,
            b"storage failure fixture".to_vec(),
            &trust,
        )
        .expect("source signed revision");
    let directory = TempDirectory::new("storage-failure");
    let destination_path = directory.file("destination.sqlite");
    let destination = Ledger::create(&destination_path, VAULT).expect("private receiver ledger");
    let fault_connection = Connection::open(&destination_path).expect("open test fault injector");
    fault_connection
        .execute_batch(
            "CREATE TRIGGER fail_sync_insert BEFORE INSERT ON revisions \
             BEGIN SELECT RAISE(ABORT, 'injected session test storage failure'); END;",
        )
        .expect("install a SQLite insert fault for this isolated fixture");
    drop(fault_connection);
    let mut sessions = in_memory_sessions(&small_identity, &large_identity, source, destination);
    let now = sessions.now;
    let mut trace = Vec::new();
    let mut received_storage_error = false;

    for _ in 0..1_000 {
        if received_storage_error {
            break;
        }
        let mut advanced = false;
        if let Some(outbound) = sessions.small.poll_output(now).expect("small output") {
            let (message_type, payload) = outbound.into_message();
            trace.push(summary(true, message_type, &payload));
            match sessions.large.handle_message(message_type, payload, now) {
                Ok(()) => advanced = true,
                Err(error) => {
                    assert_eq!(error, SyncSessionError::Storage);
                    received_storage_error = true;
                }
            }
        }
        if !received_storage_error {
            if let Some(outbound) = sessions.large.poll_output(now).expect("large output") {
                let (message_type, payload) = outbound.into_message();
                trace.push(summary(false, message_type, &payload));
                sessions
                    .small
                    .handle_message(message_type, payload, now)
                    .expect("small peer accepts receiver control frame");
                advanced = true;
            }
        }
        assert!(
            advanced || received_storage_error,
            "storage test reaches the insert"
        );
    }

    assert!(received_storage_error);
    assert_eq!(sessions.large.progress().state, SyncProgressState::Failed);
    assert_eq!(sessions.large.progress().received_revisions, 0);
    assert!(
        trace
            .iter()
            .all(|frame| !matches!(&frame.kind, FrameKind::Ack { .. }))
    );
}

#[test]
fn caller_clock_enforces_timeout_and_cancel_discards_queued_messages() {
    let (small_identity, large_identity) = identities_in_device_order();
    let trust = TestTrust::for_pair(&small_identity, &large_identity);
    let now = Instant::now();
    let mut timeout = SyncSession::new(
        Ledger::in_memory(VAULT).expect("isolated ledger"),
        Box::new(trust.clone()),
        small_identity.device_id(),
        large_identity.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("bound engine");
    timeout
        .poll_output(now)
        .expect("local hello is sent before waiting");
    assert_eq!(
        timeout.check_time(now + Duration::from_secs(30)),
        Err(SyncSessionError::Timeout)
    );
    assert_eq!(timeout.progress().state, SyncProgressState::Failed);

    let mut canceled = SyncSession::new(
        Ledger::in_memory(VAULT).expect("another isolated ledger"),
        Box::new(trust),
        small_identity.device_id(),
        large_identity.public_identity(),
        FINGERPRINT.to_vec(),
        now,
    )
    .expect("bound engine");
    canceled.cancel();
    assert_eq!(canceled.progress().state, SyncProgressState::Failed);
    assert!(
        canceled
            .poll_output(now)
            .expect("canceled engine poll")
            .is_none()
    );
}
