#![allow(clippy::expect_used)] // Deterministic fixture setup should name the failing operation.

use kryptopass_identity::DeviceIdentity;
use kryptopass_protocol::proto::{SyncRevision as WireRevision, SyncRevisionBatch as WireBatch};
use kryptopass_sync::{Id, Operation, Revision, SyncError, decode_batch, encode_batch};
use proptest::prelude::*;
use prost::Message;
use sha2::{Digest, Sha256};

fn identity() -> DeviceIdentity {
    DeviceIdentity::from_seed_material(&[0x41; 32], &[0x73; 32])
}

fn sample_revision(revision_number: u8) -> Revision {
    let author = identity();
    let vault_id = [0x11; 16];
    let mut record_id = [0x22; 16];
    record_id[0] = revision_number.max(1);
    let mut revision_id = [0x33; 16];
    revision_id[0] = revision_number.max(1);
    Revision::sign(
        vault_id,
        record_id,
        revision_id,
        Vec::new(),
        Operation::Write,
        vec![revision_number.max(1), 0xa5, 0x5a],
        &author,
    )
    .expect("sample revision signs")
}

fn wire_revision(parents: Vec<Vec<u8>>) -> WireRevision {
    let ciphertext = vec![0x91];
    WireRevision {
        vault_id: vec![0x11; 16],
        record_id: vec![0x22; 16],
        revision_id: vec![0x33; 16],
        parent_revision_ids: parents,
        author_device_id: vec![0x44; 32],
        operation: 1,
        content_hash: Sha256::digest(&ciphertext).to_vec(),
        ciphertext,
        signature: vec![0; 64],
    }
}

fn append_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn append_bytes_field(field: u64, value: &[u8], output: &mut Vec<u8>) {
    append_varint((field << 3) | 2, output);
    append_varint(value.len() as u64, output);
    output.extend_from_slice(value);
}

fn append_varint_field(field: u64, value: u64, output: &mut Vec<u8>) {
    append_varint(field << 3, output);
    append_varint(value, output);
}

fn raw_batch_with_revision(revision: &[u8]) -> Vec<u8> {
    let mut batch = Vec::new();
    append_bytes_field(1, &[0x11; 16], &mut batch);
    append_bytes_field(2, revision, &mut batch);
    batch
}

#[test]
fn protobuf_batch_round_trip_preserves_immutable_revision_bytes() {
    let vault_id: Id = [0x11; 16];
    let revision = sample_revision(1);
    let encoded =
        encode_batch(vault_id, std::slice::from_ref(&revision)).expect("sample revision encodes");
    let (decoded_vault, decoded) = decode_batch(&encoded).expect("encoded batch decodes");

    assert_eq!(decoded_vault, vault_id);
    assert_eq!(decoded, vec![revision]);
    assert_eq!(
        decoded[0].canonical_bytes(),
        sample_revision(1).canonical_bytes()
    );
}

#[test]
fn delete_revision_round_trips_with_empty_ciphertext() {
    let author = identity();
    let delete = Revision::sign(
        [0x11; 16],
        [0x22; 16],
        [0x33; 16],
        Vec::new(),
        Operation::Delete,
        Vec::new(),
        &author,
    )
    .expect("delete revision signs with the empty-content hash");
    let encoded =
        encode_batch([0x11; 16], std::slice::from_ref(&delete)).expect("delete revision encodes");
    let (vault_id, decoded) = decode_batch(&encoded).expect("delete revision decodes");

    assert_eq!(vault_id, [0x11; 16]);
    assert_eq!(decoded, vec![delete]);
    assert!(decoded[0].ciphertext.is_empty());
    assert_eq!(decoded[0].operation, Operation::Delete);
}

#[test]
fn structurally_malformed_batch_bytes_are_rejected() {
    for bytes in [&[0_u8][..], &[0x0a, 0xff][..], &[0x12, 0x80][..]] {
        assert_eq!(decode_batch(bytes), Err(SyncError::Decode));
    }
}

#[test]
fn decoder_rejects_unsorted_and_over_limit_parent_lists() {
    let unsorted = WireBatch {
        vault_id: vec![0x11; 16],
        revisions: vec![wire_revision(vec![vec![2; 16], vec![1; 16]])],
    }
    .encode_to_vec();
    assert_eq!(decode_batch(&unsorted), Err(SyncError::Decode));

    let too_many = WireBatch {
        vault_id: vec![0x11; 16],
        revisions: vec![wire_revision(
            (1..=17).map(|value| vec![value; 16]).collect(),
        )],
    }
    .encode_to_vec();
    assert_eq!(decode_batch(&too_many), Err(SyncError::LimitExceeded));
}

#[test]
fn decoder_rejects_revision_count_and_encoded_size_caps() {
    let too_many_revisions = WireBatch {
        vault_id: vec![0x11; 16],
        revisions: (0_u8..33)
            .map(|value| {
                let mut revision = wire_revision(Vec::new());
                revision.revision_id = vec![value + 1; 16];
                revision
            })
            .collect(),
    }
    .encode_to_vec();
    assert_eq!(
        decode_batch(&too_many_revisions),
        Err(SyncError::LimitExceeded)
    );

    assert_eq!(
        decode_batch(&vec![0; 8 * 1024 * 1024 + 1]),
        Err(SyncError::LimitExceeded)
    );
}

#[test]
fn decoder_rejects_duplicate_protobuf_singular_fields_and_wrapped_enum_values() {
    let mut duplicate_vault = encode_batch([0x11; 16], &[]).expect("empty batch encodes");
    append_bytes_field(1, &[0x12; 16], &mut duplicate_vault);
    assert_eq!(decode_batch(&duplicate_vault), Err(SyncError::Decode));

    let mut duplicate_ciphertext = wire_revision(Vec::new()).encode_to_vec();
    append_bytes_field(7, b"second ciphertext field", &mut duplicate_ciphertext);
    let duplicate_ciphertext = raw_batch_with_revision(&duplicate_ciphertext);
    assert_eq!(decode_batch(&duplicate_ciphertext), Err(SyncError::Decode));

    let ciphertext = [0x91];
    let mut raw_revision = Vec::new();
    append_bytes_field(1, &[0x11; 16], &mut raw_revision);
    append_bytes_field(2, &[0x22; 16], &mut raw_revision);
    append_bytes_field(3, &[0x33; 16], &mut raw_revision);
    append_bytes_field(5, &[0x44; 32], &mut raw_revision);
    append_varint_field(6, 1 + (1_u64 << 32), &mut raw_revision);
    append_bytes_field(7, &ciphertext, &mut raw_revision);
    append_bytes_field(8, &Sha256::digest(ciphertext), &mut raw_revision);
    append_bytes_field(9, &[0; 64], &mut raw_revision);
    let wrapped_write_enum = raw_batch_with_revision(&raw_revision);
    assert_eq!(decode_batch(&wrapped_write_enum), Err(SyncError::Decode));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, .. ProptestConfig::default() })]

    #[test]
    fn arbitrary_wire_bytes_never_panic_and_any_decoded_revision_is_canonical(
        bytes in prop::collection::vec(any::<u8>(), 0..4096)
    ) {
        if let Ok((_, revisions)) = decode_batch(&bytes) {
            for revision in revisions {
                prop_assert!(revision.canonical_bytes().is_ok());
            }
        }
    }

    #[test]
    fn bounded_mutations_of_valid_batches_never_panic(
        position in 0_usize..64,
        value in any::<u8>()
    ) {
        let revision = sample_revision(1);
        let mut encoded = encode_batch([0x11; 16], std::slice::from_ref(&revision))
            .expect("sample revision encodes");
        if !encoded.is_empty() {
            let index = position % encoded.len();
            encoded[index] = value;
        }
        let _ = decode_batch(&encoded);
    }
}
