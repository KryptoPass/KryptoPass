#![allow(clippy::expect_used)] // Boundary fixtures should report the failing setup step.

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_sync::{
    Id, Ledger, Operation, Revision, SYNC_MAX_BATCH_BYTES, SYNC_MAX_CIPHERTEXT_BYTES,
    SYNC_MAX_HEADS, SYNC_MAX_LEDGER_CIPHERTEXT_BYTES, SYNC_MAX_LEDGER_REVISIONS, SYNC_MAX_PARENTS,
    SYNC_MAX_REVISIONS_PER_BATCH, SyncError, TrustSource, encode_batch,
};

const VAULT: Id = [0x28; 16];

fn numeric_id(value: u32) -> Id {
    let mut id = [0; 16];
    id[12..].copy_from_slice(&value.to_be_bytes());
    id
}

fn identity() -> DeviceIdentity {
    DeviceIdentity::from_seed_material(&[0x57; 32], &[0x68; 32])
}

struct Trust(PublicIdentity);

impl TrustSource for Trust {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok((self.0.device_id == device_id).then_some(self.0))
    }
}

fn revision(
    identity: &DeviceIdentity,
    record_number: u32,
    revision_number: u32,
    parents: Vec<Id>,
    ciphertext: Vec<u8>,
) -> Result<Revision, SyncError> {
    Revision::sign(
        VAULT,
        numeric_id(record_number),
        numeric_id(revision_number),
        parents,
        Operation::Write,
        ciphertext,
        identity,
    )
}

#[test]
fn parent_head_and_ciphertext_limits_reject_without_partial_writes() {
    let identity = identity();
    let trust = Trust(identity.public_identity());
    let too_many_parents = (1..=SYNC_MAX_PARENTS as u32 + 1)
        .map(numeric_id)
        .collect::<Vec<_>>();
    assert!(matches!(
        revision(&identity, 1, 2, too_many_parents, vec![1]),
        Err(SyncError::LimitExceeded)
    ));

    let mut ledger = Ledger::in_memory(VAULT).expect("isolated ledger");
    let stale_heads = (1..=SYNC_MAX_HEADS as u32 + 1)
        .map(numeric_id)
        .collect::<Vec<_>>();
    assert!(matches!(
        ledger.append_local(
            &identity,
            numeric_id(3),
            &stale_heads,
            Operation::Write,
            vec![1],
            &trust,
        ),
        Err(SyncError::LimitExceeded)
    ));

    let many_heads = (1..=SYNC_MAX_HEADS as u32 + 1)
        .map(|index| {
            revision(&identity, 4, index + 100, Vec::new(), vec![index as u8]).expect("root signs")
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        ledger.insert_batch(&many_heads, &trust),
        Err(SyncError::LimitExceeded)
    ));
    assert_eq!(ledger.revision_count().expect("transaction rolled back"), 0);

    assert!(matches!(
        revision(
            &identity,
            5,
            6,
            Vec::new(),
            vec![0x81; SYNC_MAX_CIPHERTEXT_BYTES + 1],
        ),
        Err(SyncError::LimitExceeded)
    ));
}

#[test]
fn batch_revision_count_and_encoded_byte_limits_are_enforced() {
    let identity = identity();
    let too_many = (1..=SYNC_MAX_REVISIONS_PER_BATCH as u32 + 1)
        .map(|index| {
            revision(&identity, index, index + 100, Vec::new(), vec![index as u8])
                .expect("revision signs")
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        encode_batch(VAULT, &too_many),
        Err(SyncError::LimitExceeded)
    ));

    let large_revisions = (1..=4)
        .map(|index| {
            revision(
                &identity,
                index,
                index + 100,
                Vec::new(),
                vec![index as u8; SYNC_MAX_CIPHERTEXT_BYTES],
            )
            .expect("maximum-sized revision signs")
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        encode_batch(VAULT, &large_revisions),
        Err(SyncError::LimitExceeded)
    ));
    assert_eq!(SYNC_MAX_BATCH_BYTES, 8 * 1024 * 1024);
}

#[test]
#[ignore = "inserts 10,000 signed rows; run through the opt-in sync stress target"]
fn ledger_revision_cap_accepts_ten_thousand_independent_heads_and_rejects_one_more() {
    let identity = identity();
    let trust = Trust(identity.public_identity());
    let mut ledger = Ledger::in_memory(VAULT).expect("isolated ledger");
    let mut inserted = 0_u32;

    while inserted < SYNC_MAX_LEDGER_REVISIONS as u32 {
        let batch_size = (SYNC_MAX_LEDGER_REVISIONS as u32 - inserted).min(32);
        let mut batch = Vec::with_capacity(batch_size as usize);
        for offset in 0..batch_size {
            let number = inserted + offset + 1;
            let current = revision(
                &identity,
                number,
                number + SYNC_MAX_LEDGER_REVISIONS as u32,
                Vec::new(),
                vec![number as u8],
            )
            .expect("independent head signs");
            batch.push(current);
        }
        ledger
            .insert_batch(&batch, &trust)
            .expect("batch of independent heads fits remaining storage capacity");
        inserted += batch_size;
    }

    assert_eq!(
        ledger.revision_count().expect("maximum row count"),
        SYNC_MAX_LEDGER_REVISIONS
    );
    assert_eq!(
        ledger
            .manifest()
            .expect("maximum stored head manifest")
            .len(),
        SYNC_MAX_LEDGER_REVISIONS
    );
    let overflow = revision(
        &identity,
        inserted + 1,
        inserted + SYNC_MAX_LEDGER_REVISIONS as u32 + 1,
        Vec::new(),
        vec![1],
    )
    .expect("overflow candidate signs");
    assert!(matches!(
        ledger.insert_batch(std::slice::from_ref(&overflow), &trust),
        Err(SyncError::LimitExceeded)
    ));
    assert_eq!(
        ledger.revision_count().expect("overflow rolled back"),
        SYNC_MAX_LEDGER_REVISIONS
    );
    assert_eq!(
        ledger
            .manifest()
            .expect("overflow leaves every stored head intact")
            .len(),
        SYNC_MAX_LEDGER_REVISIONS
    );
}

#[test]
fn ledger_revision_manifest_and_total_ciphertext_caps_are_enforced() {
    let identity = identity();
    let trust = Trust(identity.public_identity());
    // Keep the documented history cap visible without inserting all 10,000
    // signed rows in this routine test; sustained capacity stress is too slow
    // for the focused sync check.
    assert_eq!(SYNC_MAX_LEDGER_REVISIONS, 10_000);

    // A realistic vault can exceed the former 1024-head manifest cap. Build
    // independent roots in small batches; the global storage cap is 10,000.
    const REALISTIC_HEAD_COUNT: u32 = 1_114;
    let mut heads_ledger = Ledger::in_memory(VAULT).expect("realistic manifest ledger");
    let mut roots = Vec::new();
    for number in 1..=REALISTIC_HEAD_COUNT {
        roots.push(
            revision(
                &identity,
                number,
                number + 20_000,
                Vec::new(),
                vec![number as u8],
            )
            .expect("independent root signs"),
        );
        if roots.len() == SYNC_MAX_REVISIONS_PER_BATCH {
            heads_ledger
                .insert_batch(&roots, &trust)
                .expect("bounded head batch");
            roots.clear();
        }
    }
    if !roots.is_empty() {
        heads_ledger
            .insert_batch(&roots, &trust)
            .expect("last head batch");
    }
    assert_eq!(
        heads_ledger
            .manifest()
            .expect("manifest includes every realistic root")
            .len(),
        REALISTIC_HEAD_COUNT as usize
    );

    let mut ledger = Ledger::in_memory(VAULT).expect("ciphertext capacity ledger");
    let megabyte = 1024 * 1024;
    let ciphertext_batch = (1..=3)
        .map(|index| {
            revision(
                &identity,
                index,
                index + 50_000,
                Vec::new(),
                vec![index as u8; megabyte],
            )
            .expect("megabyte revision signs")
        })
        .collect::<Vec<_>>();
    let full_batches = SYNC_MAX_LEDGER_CIPHERTEXT_BYTES / (3 * megabyte);
    for batch_index in 0..full_batches {
        let batch = ciphertext_batch
            .iter()
            .enumerate()
            .map(|(position, template)| {
                let record_number = u32::try_from(batch_index * 3 + position + 1)
                    .expect("small deterministic record number");
                revision(
                    &identity,
                    record_number,
                    record_number + 50_000,
                    Vec::new(),
                    template.ciphertext.clone(),
                )
                .expect("large revision signs")
            })
            .collect::<Vec<_>>();
        ledger
            .insert_batch(&batch, &trust)
            .expect("three megabytes fit the remaining ledger capacity");
    }
    let remainder = SYNC_MAX_LEDGER_CIPHERTEXT_BYTES % (3 * megabyte);
    assert_eq!(remainder, megabyte);
    let last = revision(
        &identity,
        u32::try_from(full_batches * 3 + 1).expect("small ID"),
        u32::try_from(full_batches * 3 + 50_001).expect("small ID"),
        Vec::new(),
        vec![0x7f; remainder],
    )
    .expect("final exact-capacity revision signs");
    ledger
        .insert_batch(std::slice::from_ref(&last), &trust)
        .expect("exact ciphertext ledger cap is accepted");
    assert_eq!(
        ledger.revision_count().expect("exact ciphertext rows"),
        full_batches * 3 + 1
    );

    let overflow = revision(
        &identity,
        u32::try_from(full_batches * 3 + 2).expect("small ID"),
        u32::try_from(full_batches * 3 + 50_002).expect("small ID"),
        Vec::new(),
        vec![0x80],
    )
    .expect("one-byte overflow candidate signs");
    assert!(matches!(
        ledger.insert_batch(&[overflow], &trust),
        Err(SyncError::LimitExceeded)
    ));
    assert_eq!(
        ledger
            .revision_count()
            .expect("ciphertext overflow rolled back"),
        full_batches * 3 + 1
    );
}
