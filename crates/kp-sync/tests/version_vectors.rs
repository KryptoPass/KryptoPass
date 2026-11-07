//! Golden vectors of the signed objects, shared with the Kotlin implementation
//! (`kryptopass-android/fixtures/sync/versions-v2.json`). Regenerate with
//! `KP_WRITE_SYNC_VECTORS=1`; a change here is a change of the wire contract.

#![allow(clippy::expect_used, reason = "fixture setup in a test")]

use kryptopass_identity::DeviceIdentity;
use kryptopass_sync::{
    Batch, Clock, Deletion, DeletionDraft, Fingerprint, RecordKind, Version, VersionDraft,
    blob_hash, decode_batch, encode_batch,
};
use serde_json::{Value, json};
use std::path::PathBuf;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn key(fill: u8) -> Fingerprint {
    Fingerprint::new(vec![fill; 20]).expect("valid fingerprint")
}

fn author() -> DeviceIdentity {
    DeviceIdentity::from_seed_material(&[0x11; 32], &[0x22; 32])
}

fn item_version() -> Version {
    Version::sign(
        VersionDraft {
            record_kind: RecordKind::Item,
            record_id: [0xa1; 16],
            version_id: [0xb2; 16],
            parents: vec![[0xc4; 16], [0xc3; 16]],
            clock: Clock {
                millis: 1_790_000_000_123,
                counter: 7,
            },
            recipients: vec![key(0xe2), key(0xe1)],
            state: b"sealed state".to_vec(),
            content_hash: Some(blob_hash(b"sealed content")),
            attachments: vec![blob_hash(b"second"), blob_hash(b"first")],
        },
        &author(),
    )
    .expect("sign item version")
}

fn folder_version() -> Version {
    Version::sign(
        VersionDraft {
            record_kind: RecordKind::Folder,
            record_id: [0xa5; 16],
            version_id: [0xb6; 16],
            parents: Vec::new(),
            clock: Clock {
                millis: u64::MAX,
                counter: u32::MAX,
            },
            recipients: vec![Fingerprint::new(vec![0xee; 32]).expect("valid fingerprint")],
            state: b"sealed folder".to_vec(),
            content_hash: None,
            attachments: Vec::new(),
        },
        &author(),
    )
    .expect("sign folder version")
}

fn deletion() -> Deletion {
    Deletion::sign(
        DeletionDraft {
            record_kind: RecordKind::Item,
            record_id: [0xa7; 16],
            clock: Clock {
                millis: 1_790_000_000_999,
                counter: 0,
            },
            recipients: vec![key(0xe2), key(0xe1)],
        },
        &author(),
    )
    .expect("sign deletion")
}

fn vectors() -> Value {
    let identity = author().public_identity();
    let item = item_version();
    let folder = folder_version();
    let mark = deletion();
    let describe = |canonical: Vec<u8>, signature: &[u8], hash: [u8; 32]| {
        json!({
            "canonical": hex(&canonical),
            "signature": hex(signature),
            "hash": hex(&hash),
        })
    };
    let batch = Batch {
        deletions: vec![mark.clone()],
        versions: vec![item.clone(), folder.clone()],
        blobs: vec![b"sealed content".to_vec()],
    };
    json!({
        "author": {
            "device_id": hex(identity.device_id.as_bytes()),
            "signing_public_key": hex(&identity.signing_public_key),
            "static_public_key": hex(&identity.static_public_key),
        },
        "item": describe(
            item.canonical_bytes().expect("canonical"),
            &item.signature,
            item.hash().expect("hash"),
        ),
        "folder": describe(
            folder.canonical_bytes().expect("canonical"),
            &folder.signature,
            folder.hash().expect("hash"),
        ),
        "deletion": describe(
            mark.canonical_bytes().expect("canonical"),
            &mark.signature,
            mark.hash().expect("hash"),
        ),
        "batch": hex(&encode_batch(&batch).expect("encode")),
    })
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/versions-v2.json")
}

#[test]
fn signed_objects_match_the_shared_vectors() {
    let actual = vectors();
    if std::env::var_os("KP_WRITE_SYNC_VECTORS").is_some() {
        let text = serde_json::to_string_pretty(&actual).expect("encode vectors");
        std::fs::create_dir_all(fixture().parent().expect("parent")).expect("create directory");
        std::fs::write(fixture(), text + "\n").expect("write vectors");
    }
    let stored: Value =
        serde_json::from_slice(&std::fs::read(fixture()).expect("read vectors")).expect("parse");
    assert_eq!(actual, stored);
}

#[test]
fn signing_sorts_lists_and_verification_rejects_any_change() {
    let version = item_version();
    assert!(version.parents.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(version.recipients.windows(2).all(|pair| pair[0] < pair[1]));
    let identity = author().public_identity();
    version.verify(&identity).expect("signature verifies");

    let mut tampered = version.clone();
    tampered.state[0] ^= 1;
    assert!(tampered.verify(&identity).is_err());

    let other = DeviceIdentity::from_seed_material(&[0x33; 32], &[0x44; 32]).public_identity();
    assert!(version.verify(&other).is_err());
}

#[test]
fn a_version_must_match_its_kind() {
    let mut no_content = item_version();
    no_content.content_hash = None;
    assert!(no_content.canonical_bytes().is_err());

    let mut folder_with_content = folder_version();
    folder_with_content.content_hash = Some([1; 32]);
    assert!(folder_with_content.canonical_bytes().is_err());

    let mut self_parent = item_version();
    self_parent.parents = vec![self_parent.version_id];
    assert!(self_parent.canonical_bytes().is_err());
}

#[test]
fn batches_round_trip_and_hostile_input_is_refused() {
    let batch = Batch {
        deletions: vec![deletion()],
        versions: vec![item_version()],
        blobs: vec![b"sealed content".to_vec()],
    };
    let decoded = decode_batch(&encode_batch(&batch).expect("encode")).expect("decode");
    assert_eq!(decoded.objects(), 2);
    assert_eq!(decoded.blobs, batch.blobs);
    assert!(decoded.versions[0] == batch.versions[0]);

    assert!(encode_batch(&Batch::default()).is_err());
    assert!(decode_batch(&[]).is_err());
    assert!(decode_batch(&[0xff; 32]).is_err());
}
