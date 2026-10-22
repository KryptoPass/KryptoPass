#![allow(clippy::expect_used)] // Database and identity fixtures need clear setup failures.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_sync::{
    Id, Ledger, Operation, Revision, RevisionHead, SyncError, TrustSource, reconcile,
};

const VAULT: Id = [0x19; 16];

fn id(value: u8) -> Id {
    [value.max(1); 16]
}

fn identity(seed: u8) -> DeviceIdentity {
    DeviceIdentity::from_seed_material(&[seed; 32], &[seed.wrapping_add(41); 32])
}

#[derive(Default)]
struct TestTrust(HashMap<DeviceId, PublicIdentity>);

impl TestTrust {
    fn add(&mut self, identity: &DeviceIdentity) {
        self.0
            .insert(identity.device_id(), identity.public_identity());
    }
}

impl TrustSource for TestTrust {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok(self.0.get(&device_id).copied())
    }
}

fn head_ids(heads: &[RevisionHead]) -> Vec<Id> {
    heads.iter().map(|head| head.revision_id).collect()
}

fn sorted(mut ids: Vec<Id>) -> Vec<Id> {
    ids.sort_unstable();
    ids
}

struct TempDatabase {
    directory: PathBuf,
    path: PathBuf,
}

impl TempDatabase {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let suffix = NEXT.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "kp-sync-test-{}-{suffix}.sqlite",
            std::process::id()
        ));
        std::fs::create_dir(&directory).expect("create private ledger directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                .expect("make ledger directory private");
        }
        let path = directory.join("vault.sqlite");
        Self { directory, path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn offline_edits_converge_as_conflict_until_an_explicit_merge() {
    let alice = identity(1);
    let bob = identity(2);
    let mut trust = TestTrust::default();
    trust.add(&alice);
    trust.add(&bob);
    let mut left = Ledger::in_memory(VAULT).expect("left isolated ledger");
    let mut right = Ledger::in_memory(VAULT).expect("right isolated ledger");
    let record = id(3);

    let root = left
        .append_local(
            &alice,
            record,
            &[],
            Operation::Write,
            b"ciphertext-root".to_vec(),
            &trust,
        )
        .expect("offline root write");
    right
        .insert_batch(std::slice::from_ref(&root), &trust)
        .expect("copy root to peer");

    let alice_edit = left
        .append_local(
            &alice,
            record,
            &[root.revision_id],
            Operation::Write,
            b"ciphertext-alice-edit".to_vec(),
            &trust,
        )
        .expect("Alice edits offline");
    let bob_edit = right
        .append_local(
            &bob,
            record,
            &[root.revision_id],
            Operation::Write,
            b"ciphertext-bob-edit".to_vec(),
            &trust,
        )
        .expect("Bob edits offline");
    left.insert_batch(std::slice::from_ref(&bob_edit), &trust)
        .expect("exchange Bob's branch");
    right
        .insert_batch(std::slice::from_ref(&alice_edit), &trust)
        .expect("exchange Alice's branch");

    let conflict_heads = sorted(vec![alice_edit.revision_id, bob_edit.revision_id]);
    assert_eq!(
        head_ids(&left.heads(record).expect("left heads")),
        conflict_heads
    );
    assert_eq!(
        head_ids(&right.heads(record).expect("right heads")),
        conflict_heads
    );
    assert!(
        matches!(
            left.append_local(
                &alice,
                record,
                &[root.revision_id],
                Operation::Write,
                b"stale-write".to_vec(),
                &trust,
            ),
            Err(SyncError::HeadConflict)
        ),
        "a stale expected-head set cannot overwrite either branch"
    );

    let merge = left
        .append_local(
            &alice,
            record,
            &conflict_heads,
            Operation::Write,
            b"ciphertext-explicit-merge".to_vec(),
            &trust,
        )
        .expect("explicit merge covers all conflict heads");
    right
        .insert_batch(std::slice::from_ref(&merge), &trust)
        .expect("exchange explicit merge");
    assert_eq!(
        head_ids(&left.heads(record).expect("left merged head")),
        vec![merge.revision_id]
    );
    assert_eq!(
        head_ids(&right.heads(record).expect("right merged head")),
        vec![merge.revision_id]
    );
    assert_eq!(left.revision_count().expect("left revision count"), 4);
    assert_eq!(right.revision_count().expect("right revision count"), 4);
}

#[test]
fn delete_vs_edit_stays_a_conflict_and_ancestor_replay_cannot_resurrect() {
    let alice = identity(3);
    let bob = identity(4);
    let mut trust = TestTrust::default();
    trust.add(&alice);
    trust.add(&bob);
    let mut left = Ledger::in_memory(VAULT).expect("left isolated ledger");
    let mut right = Ledger::in_memory(VAULT).expect("right isolated ledger");
    let record = id(5);

    let root = left
        .append_local(
            &alice,
            record,
            &[],
            Operation::Write,
            b"ciphertext-root".to_vec(),
            &trust,
        )
        .expect("root write");
    right
        .insert_batch(std::slice::from_ref(&root), &trust)
        .expect("copy root");
    let tombstone = left
        .append_local(
            &alice,
            record,
            &[root.revision_id],
            Operation::Delete,
            Vec::new(),
            &trust,
        )
        .expect("offline delete");
    let edit = right
        .append_local(
            &bob,
            record,
            &[root.revision_id],
            Operation::Write,
            b"ciphertext-concurrent-edit".to_vec(),
            &trust,
        )
        .expect("offline edit");
    left.insert_batch(std::slice::from_ref(&edit), &trust)
        .expect("receive edit");
    right
        .insert_batch(std::slice::from_ref(&tombstone), &trust)
        .expect("receive tombstone");

    let conflict_heads = sorted(vec![tombstone.revision_id, edit.revision_id]);
    assert_eq!(
        head_ids(&left.heads(record).expect("left conflict")),
        conflict_heads
    );
    assert_eq!(
        head_ids(&right.heads(record).expect("right conflict")),
        conflict_heads
    );
    assert_eq!(
        left.insert_batch(std::slice::from_ref(&root), &trust)
            .expect("ancestor retry is idempotent")
            .inserted,
        0,
        "replaying an existing ancestor adds nothing"
    );
    assert_eq!(
        head_ids(&left.heads(record).expect("heads after replay")),
        conflict_heads
    );
    assert!(
        left.get(tombstone.revision_id)
            .expect("tombstone lookup")
            .is_some()
    );

    let restore = right
        .append_local(
            &bob,
            record,
            &conflict_heads,
            Operation::Write,
            b"ciphertext-explicit-restore".to_vec(),
            &trust,
        )
        .expect("intentional restore is a descendant of both heads");
    left.insert_batch(std::slice::from_ref(&restore), &trust)
        .expect("receive intentional restore");
    assert_eq!(
        head_ids(&left.heads(record).expect("restored head")),
        vec![restore.revision_id]
    );
    assert_eq!(
        head_ids(&right.heads(record).expect("restored peer head")),
        vec![restore.revision_id]
    );
}

#[test]
fn batch_graph_validation_is_atomic_and_accepts_out_of_order_ancestors() {
    let alice = identity(5);
    let mut trust = TestTrust::default();
    trust.add(&alice);
    let mut ledger = Ledger::in_memory(VAULT).expect("isolated ledger");
    let record = id(6);
    let root = Revision::sign(
        VAULT,
        record,
        id(10),
        Vec::new(),
        Operation::Write,
        b"root".to_vec(),
        &alice,
    )
    .expect("root signs");
    let child = Revision::sign(
        VAULT,
        record,
        id(11),
        vec![root.revision_id],
        Operation::Write,
        b"child".to_vec(),
        &alice,
    )
    .expect("child signs");
    ledger
        .insert_batch(&[child.clone(), root.clone()], &trust)
        .expect("child may precede its ancestor in the batch");
    assert_eq!(
        head_ids(&ledger.heads(record).expect("child head")),
        vec![child.revision_id]
    );

    let missing_parent = Revision::sign(
        VAULT,
        id(12),
        id(12),
        vec![id(99)],
        Operation::Write,
        b"missing".to_vec(),
        &alice,
    )
    .expect("missing-parent object signs");
    assert!(matches!(
        ledger.insert_batch(std::slice::from_ref(&missing_parent), &trust),
        Err(SyncError::MissingParent)
    ));

    let cross_record_parent = Revision::sign(
        VAULT,
        id(13),
        id(13),
        vec![root.revision_id],
        Operation::Write,
        b"cross-record".to_vec(),
        &alice,
    )
    .expect("cross-record object signs");
    assert!(matches!(
        ledger.insert_batch(std::slice::from_ref(&cross_record_parent), &trust),
        Err(SyncError::CrossRecordParent)
    ));

    let self_parent = Revision::sign(
        VAULT,
        id(14),
        id(14),
        vec![id(14)],
        Operation::Write,
        b"self-parent".to_vec(),
        &alice,
    )
    .expect("self-parent object signs");
    assert!(matches!(
        ledger.insert_batch(std::slice::from_ref(&self_parent), &trust),
        Err(SyncError::SelfParent)
    ));

    let cycle_left = Revision::sign(
        VAULT,
        id(15),
        id(15),
        vec![id(16)],
        Operation::Write,
        b"cycle-left".to_vec(),
        &alice,
    )
    .expect("cycle left signs");
    let cycle_right = Revision::sign(
        VAULT,
        id(15),
        id(16),
        vec![id(15)],
        Operation::Write,
        b"cycle-right".to_vec(),
        &alice,
    )
    .expect("cycle right signs");
    assert!(matches!(
        ledger.insert_batch(&[cycle_left, cycle_right], &trust),
        Err(SyncError::Cycle)
    ));
    assert_eq!(ledger.revision_count().expect("unchanged count"), 2);
}

#[test]
fn duplicate_replay_is_idempotent_and_id_collision_rolls_back_the_batch() {
    let alice = identity(6);
    let mut trust = TestTrust::default();
    trust.add(&alice);
    let mut ledger = Ledger::in_memory(VAULT).expect("isolated ledger");
    let original = Revision::sign(
        VAULT,
        id(17),
        id(18),
        Vec::new(),
        Operation::Write,
        b"original".to_vec(),
        &alice,
    )
    .expect("original signs");
    assert_eq!(
        ledger
            .insert_batch(std::slice::from_ref(&original), &trust)
            .expect("initial insert")
            .inserted,
        1
    );
    assert_eq!(
        ledger
            .insert_batch(std::slice::from_ref(&original), &trust)
            .expect("same immutable object replays")
            .inserted,
        0
    );

    let unrelated = Revision::sign(
        VAULT,
        id(19),
        id(20),
        Vec::new(),
        Operation::Write,
        b"must-roll-back".to_vec(),
        &alice,
    )
    .expect("unrelated revision signs");
    let collision = Revision::sign(
        VAULT,
        id(17),
        id(18),
        Vec::new(),
        Operation::Write,
        b"different bytes, same revision ID".to_vec(),
        &alice,
    )
    .expect("colliding object can have its own valid signature");
    assert!(matches!(
        ledger.insert_batch(&[unrelated.clone(), collision], &trust),
        Err(SyncError::RevisionIdCollision)
    ));
    assert_eq!(ledger.revision_count().expect("rolled back count"), 1);
    assert!(
        ledger
            .get(unrelated.revision_id)
            .expect("rolled-back lookup")
            .is_none()
    );
}

#[test]
fn unknown_authors_are_rejected_without_partial_storage() {
    let trusted = identity(7);
    let unknown = identity(8);
    let mut trust = TestTrust::default();
    trust.add(&trusted);
    let mut ledger = Ledger::in_memory(VAULT).expect("isolated ledger");
    let trusted_revision = Revision::sign(
        VAULT,
        id(21),
        id(22),
        Vec::new(),
        Operation::Write,
        b"trusted".to_vec(),
        &trusted,
    )
    .expect("trusted revision signs");
    let unknown_revision = Revision::sign(
        VAULT,
        id(23),
        id(24),
        Vec::new(),
        Operation::Write,
        b"unknown".to_vec(),
        &unknown,
    )
    .expect("unknown revision signs locally");

    assert!(matches!(
        ledger.insert_batch(&[trusted_revision, unknown_revision], &trust),
        Err(SyncError::UnknownAuthor)
    ));
    assert_eq!(ledger.revision_count().expect("unchanged count"), 0);
}

#[test]
fn reconciliation_keeps_first_committed_batch_and_retries_only_missing_revisions() {
    let alice = identity(10);
    let bob = identity(11);
    let mut all_authors = TestTrust::default();
    all_authors.add(&alice);
    all_authors.add(&bob);
    let mut destination_trust = TestTrust::default();
    destination_trust.add(&alice);
    let mut source = Ledger::in_memory(VAULT).expect("source ledger");
    let mut destination = Ledger::in_memory(VAULT).expect("destination ledger");

    let revisions = (1..=33)
        .map(|number| {
            let author = if number == 33 { &bob } else { &alice };
            Revision::sign(
                VAULT,
                id(number + 70),
                id(number),
                Vec::new(),
                Operation::Write,
                vec![number],
                author,
            )
            .expect("revision signs")
        })
        .collect::<Vec<_>>();
    source
        .insert_batch(&revisions[..32], &all_authors)
        .expect("first batch fits source");
    source
        .insert_batch(&revisions[32..], &all_authors)
        .expect("last revision fits source");

    assert!(matches!(
        reconcile(&source, &mut destination, &destination_trust),
        Err(SyncError::UnknownAuthor)
    ));
    assert_eq!(
        destination
            .revision_count()
            .expect("first batch remains committed"),
        32
    );
    assert_eq!(
        destination.manifest().expect("committed first batch").len(),
        32
    );

    destination_trust.add(&bob);
    let retried = reconcile(&source, &mut destination, &destination_trust)
        .expect("retry sends the previously rejected revision");
    assert_eq!(retried.batches_sent, 1);
    assert_eq!(retried.revisions_sent, 1);
    assert_eq!(retried.duplicates, 0);
    assert_eq!(retried.acknowledged, 1);
    assert_eq!(
        destination.revision_count().expect("all rows committed"),
        33
    );

    let second_retry = reconcile(&source, &mut destination, &destination_trust)
        .expect("identical reconciliation is idempotent");
    assert_eq!(second_retry.batches_sent, 0);
    assert_eq!(second_retry.revisions_sent, 0);
    assert_eq!(second_retry.acknowledged, 0);
}

#[test]
fn local_head_compare_and_swap_and_sqlite_reopen_preserve_history() {
    let alice = identity(9);
    let mut trust = TestTrust::default();
    trust.add(&alice);
    let database = TempDatabase::new();
    let record = id(25);
    let root_id;
    {
        let mut ledger =
            Ledger::create(database.path(), VAULT).expect("create isolated SQLite ledger");
        let root = ledger
            .append_local(
                &alice,
                record,
                &[],
                Operation::Write,
                b"persistent-root".to_vec(),
                &trust,
            )
            .expect("append persistent root");
        root_id = root.revision_id;
        let child = ledger
            .append_local(
                &alice,
                record,
                &[root_id],
                Operation::Write,
                b"persistent-child".to_vec(),
                &trust,
            )
            .expect("append persistent child");
        assert!(matches!(
            ledger.append_local(
                &alice,
                record,
                &[],
                Operation::Write,
                b"stale".to_vec(),
                &trust,
            ),
            Err(SyncError::HeadConflict)
        ));
        assert_eq!(ledger.revision_count().expect("two rows"), 2);
        assert_eq!(
            head_ids(&ledger.heads(record).expect("child is the only head")),
            vec![child.revision_id]
        );
    }

    let reopened = Ledger::open_existing(database.path(), VAULT, &trust)
        .expect("open existing isolated ledger");
    assert_eq!(reopened.revision_count().expect("persisted rows"), 2);
    assert!(reopened.get(root_id).expect("persisted root").is_some());
    assert_eq!(
        head_ids(&reopened.heads(record).expect("persisted head")),
        head_ids(
            &reopened
                .manifest()
                .expect("persisted manifest")
                .into_iter()
                .filter(|head| head.record_id == record)
                .collect::<Vec<_>>()
        )
    );
}
