//! Whole Sync rounds between two real stores.

#![allow(clippy::expect_used, reason = "fixture setup in a test")]

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_store::SqliteStore;
use kryptopass_sync::{
    Batch, Clock, Deletion, DeletionDraft, Fingerprint, Id, RecordKind, SyncError, SyncSession,
    SyncStore, TrustSource, Version, VersionDraft, blob_hash,
};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
struct Peers(Vec<PublicIdentity>);

impl TrustSource for Peers {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok(self
            .0
            .iter()
            .find(|peer| peer.device_id == device_id)
            .copied())
    }
}

struct Node {
    identity: Arc<DeviceIdentity>,
    path: PathBuf,
    store: SqliteStore,
}

struct Lab {
    directory: PathBuf,
    a: Node,
    b: Node,
    trust: Peers,
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn node(directory: &Path, name: &str, seed: u8) -> Node {
    let identity = Arc::new(DeviceIdentity::from_seed_material(
        &[seed; 32],
        &[seed + 1; 32],
    ));
    let path = directory.join(name);
    let store = SqliteStore::open(&path, Arc::clone(&identity)).expect("open store");
    Node {
        identity,
        path,
        store,
    }
}

fn lab(name: &str) -> Lab {
    let directory =
        std::env::temp_dir().join(format!("kp-sync-rounds-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create directory");
    // The store refuses a directory other accounts can read.
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
        .expect("restrict directory");
    let a = node(&directory, "a.sqlite", 0x10);
    let b = node(&directory, "b.sqlite", 0x20);
    let trust = Peers(vec![
        a.identity.public_identity(),
        b.identity.public_identity(),
    ]);
    Lab {
        directory,
        a,
        b,
        trust,
    }
}

fn key(fill: u8) -> Fingerprint {
    Fingerprint::new(vec![fill; 20]).expect("valid fingerprint")
}

fn version(
    author: &DeviceIdentity,
    record: u8,
    id: u8,
    parents: &[Id],
    keys: &[u8],
) -> (Version, Vec<u8>) {
    let content = format!("sealed content {id}").into_bytes();
    let signed = Version::sign(
        VersionDraft {
            record_kind: RecordKind::Item,
            record_id: [record; 16],
            version_id: [id; 16],
            parents: parents.to_vec(),
            clock: Clock {
                millis: 1_000 + u64::from(id),
                counter: 0,
            },
            recipients: keys.iter().map(|fill| key(*fill)).collect(),
            state: vec![id; 32],
            content_hash: Some(blob_hash(&content)),
            attachments: Vec::new(),
        },
        author,
    )
    .expect("sign version");
    (signed, content)
}

/// Stores `objects` as if a third device had sent them, so no peer is marked as shared.
fn seed(node: &mut Node, objects: Vec<(Version, Vec<u8>)>) {
    let (versions, blobs) = objects.into_iter().unzip();
    node.store
        .apply(
            Batch {
                deletions: Vec::new(),
                versions,
                blobs,
            },
            &DeviceId::from_bytes([0x99; 32]),
        )
        .expect("seed store");
}

/// Runs one round to completion, handing each message to the other side.
fn round(lab: &Lab, a_keys: &[u8], b_keys: &[u8]) {
    let open = |node: &Node, peer: &Node, keys: &[u8]| {
        SyncSession::new(
            Box::new(
                SqliteStore::open_existing(&node.path, Arc::clone(&node.identity)).expect("open"),
            ),
            Box::new(lab.trust.clone()),
            node.identity.device_id(),
            peer.identity.public_identity(),
            keys.iter().map(|fill| key(*fill)).collect(),
            Instant::now(),
        )
        .expect("start session")
    };
    let mut first = open(&lab.a, &lab.b, a_keys);
    let mut second = open(&lab.b, &lab.a, b_keys);
    for _ in 0..10_000 {
        let now = Instant::now();
        let mut moved = false;
        while let Some(message) = first.poll_output(now).expect("first output") {
            let (kind, payload) = message.into_message();
            second
                .handle_message(kind, payload, now)
                .expect("second accepts");
            moved = true;
        }
        while let Some(message) = second.poll_output(now).expect("second output") {
            let (kind, payload) = message.into_message();
            first
                .handle_message(kind, payload, now)
                .expect("first accepts");
            moved = true;
        }
        if !moved {
            return;
        }
    }
    panic!("the round did not settle");
}

#[test]
fn versions_travel_with_their_ancestors_and_blobs_in_both_directions() {
    let mut lab = lab("both");
    let author = Arc::clone(&lab.a.identity);
    let first = version(&author, 1, 1, &[], &[0xe1]);
    let second = version(&author, 1, 2, &[[1; 16]], &[0xe1]);
    seed(&mut lab.a, vec![first, second]);
    let other = Arc::clone(&lab.b.identity);
    seed(&mut lab.b, vec![version(&other, 2, 3, &[], &[0xe1])]);

    round(&lab, &[0xe1], &[0xe1]);

    assert_eq!(lab.a.store.object_count().expect("count"), 3);
    assert_eq!(lab.b.store.object_count().expect("count"), 3);
    let received = lab
        .b
        .store
        .version(&[2; 16])
        .expect("read")
        .expect("ancestor arrived");
    assert_eq!(received.parents, vec![[1; 16]]);
    let hash = received.content_hash.expect("item content");
    assert!(lab.b.store.blob(&hash).expect("read").is_some());
    received
        .verify(&author.public_identity())
        .expect("arrives byte for byte, signature included");
}

#[test]
fn only_records_of_the_keys_a_peer_syncs_are_offered() {
    let mut lab = lab("routing");
    let author = Arc::clone(&lab.a.identity);
    seed(
        &mut lab.a,
        vec![
            version(&author, 1, 1, &[], &[0xe1]),
            version(&author, 2, 2, &[], &[0xe2]),
        ],
    );

    round(&lab, &[0xe1], &[0xe1]);

    assert!(lab.b.store.version(&[1; 16]).expect("read").is_some());
    assert!(lab.b.store.version(&[2; 16]).expect("read").is_none());
}

#[test]
fn a_record_keeps_going_to_a_peer_after_leaving_its_keys() {
    let mut lab = lab("access-loss");
    let author = Arc::clone(&lab.a.identity);
    seed(&mut lab.a, vec![version(&author, 1, 1, &[], &[0xe1])]);
    round(&lab, &[0xe1], &[0xe1]);

    seed(
        &mut lab.a,
        vec![version(&author, 1, 2, &[[1; 16]], &[0xe2])],
    );
    round(&lab, &[0xe1], &[0xe1]);

    assert!(
        lab.b.store.version(&[2; 16]).expect("read").is_some(),
        "the peer learns it lost access instead of keeping the old version"
    );
}

#[test]
fn diverging_edits_stay_side_by_side() {
    let mut lab = lab("conflict");
    let author = Arc::clone(&lab.a.identity);
    let other = Arc::clone(&lab.b.identity);
    let base = version(&author, 1, 1, &[], &[0xe1]);
    seed(
        &mut lab.a,
        vec![base.clone(), version(&author, 1, 2, &[[1; 16]], &[0xe1])],
    );
    seed(
        &mut lab.b,
        vec![base, version(&other, 1, 3, &[[1; 16]], &[0xe1])],
    );

    round(&lab, &[0xe1], &[0xe1]);

    for store in [&lab.a.store, &lab.b.store] {
        assert!(store.version(&[2; 16]).expect("read").is_some());
        assert!(store.version(&[3; 16]).expect("read").is_some());
    }
}

#[test]
fn a_deletion_mark_removes_everything_and_stops_old_versions() {
    let mut lab = lab("deletion");
    let author = Arc::clone(&lab.a.identity);
    let old = version(&author, 1, 1, &[], &[0xe1]);
    seed(&mut lab.a, vec![old.clone()]);
    seed(&mut lab.b, vec![old.clone()]);
    let mark = Deletion::sign(
        DeletionDraft {
            record_kind: RecordKind::Item,
            record_id: [1; 16],
            clock: Clock {
                millis: 9_000,
                counter: 0,
            },
            recipients: vec![key(0xe1)],
        },
        &author,
    )
    .expect("sign deletion");
    lab.a
        .store
        .apply(
            Batch {
                deletions: vec![mark],
                ..Batch::default()
            },
            &DeviceId::from_bytes([0x99; 32]),
        )
        .expect("delete");

    round(&lab, &[0xe1], &[0xe1]);

    let hash = old.0.content_hash.expect("item content");
    assert!(lab.b.store.version(&[1; 16]).expect("read").is_none());
    assert!(lab.b.store.deletion(&[1; 16]).expect("read").is_some());
    assert!(lab.b.store.blob(&hash).expect("read").is_none());

    seed(&mut lab.b, vec![old]);
    assert!(
        lab.b.store.version(&[1; 16]).expect("read").is_none(),
        "an old version cannot bring it back"
    );
}

#[test]
fn an_author_nobody_trusts_is_refused() {
    let lab = lab("untrusted");
    let stranger = DeviceIdentity::from_seed_material(&[0x70; 32], &[0x71; 32]);
    let (signed, content) = version(&stranger, 1, 2, &[], &[0xe1]);
    let batch = Batch {
        deletions: Vec::new(),
        versions: vec![signed],
        blobs: vec![content],
    };
    assert!(
        kryptopass_sync::verify_batch(&batch, &lab.trust).err() == Some(SyncError::UnknownAuthor)
    );
}
