#![allow(clippy::expect_used)] // Fixed test-vector setup should name the failing field.

use kryptopass_identity::DeviceIdentity;
use kryptopass_sync::{Operation, Revision};
use serde_json::Value;

const VECTOR: &str = include_str!("fixtures/sync_revision_v1.json");
const ORDER_VECTOR: &str = include_str!("fixtures/sync_revision_unsigned_parent_order_v1.json");

fn field<'a>(value: &'a Value, name: &str) -> &'a str {
    value[name]
        .as_str()
        .unwrap_or_else(|| panic!("fixture field {name} must be a string"))
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "hex string has an odd length");
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => panic!("fixture contains non-hex data"),
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect()
}

fn fixed<const N: usize>(value: &str) -> [u8; N] {
    decode_hex(value)
        .try_into()
        .unwrap_or_else(|_| panic!("fixture field has the wrong byte length"))
}

#[test]
fn canonical_signature_and_object_digest_match_independent_vector() {
    let vector: Value = serde_json::from_str(VECTOR).expect("valid checked-in JSON fixture");
    let static_seed = fixed::<32>(field(&vector, "static_seed_hex"));
    let signing_seed = fixed::<32>(field(&vector, "signing_seed_hex"));
    let identity = DeviceIdentity::from_seed_material(&static_seed, &signing_seed);
    let public = identity.public_identity();

    assert_eq!(
        public.static_public_key,
        fixed::<32>(field(&vector, "static_public_key_hex"))
    );
    assert_eq!(
        public.signing_public_key,
        fixed::<32>(field(&vector, "signing_public_key_hex"))
    );
    assert_eq!(
        public.device_id.as_bytes(),
        &fixed::<32>(field(&vector, "device_id_hex"))
    );

    let parents = vector["parent_revision_ids_hex"]
        .as_array()
        .expect("parent list")
        .iter()
        .map(|parent| fixed::<16>(parent.as_str().expect("parent ID is a hex string")))
        .collect();
    let revision = Revision::sign(
        fixed::<16>(field(&vector, "vault_id_hex")),
        fixed::<16>(field(&vector, "record_id_hex")),
        fixed::<16>(field(&vector, "revision_id_hex")),
        parents,
        Operation::Write,
        decode_hex(field(&vector, "ciphertext_hex")),
        &identity,
    )
    .expect("fixture revision signs");

    assert!(revision.parent_revision_ids[0] < revision.parent_revision_ids[1]);
    assert_eq!(
        revision.content_hash,
        fixed::<32>(field(&vector, "content_hash_hex"))
    );
    assert_eq!(
        revision.canonical_bytes().expect("canonical bytes"),
        decode_hex(field(&vector, "canonical_hex"))
    );
    assert_eq!(
        revision.signature,
        fixed::<64>(field(&vector, "signature_hex"))
    );
    assert_eq!(
        revision.hash().expect("object hash"),
        fixed::<32>(field(&vector, "object_hash_hex"))
    );
    revision
        .verify(&public)
        .expect("signature verifies against its author");
}

#[test]
fn signatures_bind_every_immutable_field() {
    let vector: Value = serde_json::from_str(VECTOR).expect("valid checked-in JSON fixture");
    let identity = DeviceIdentity::from_seed_material(
        &fixed::<32>(field(&vector, "static_seed_hex")),
        &fixed::<32>(field(&vector, "signing_seed_hex")),
    );
    let parents = vector["parent_revision_ids_hex"]
        .as_array()
        .expect("parent list")
        .iter()
        .map(|parent| fixed::<16>(parent.as_str().expect("parent ID is a hex string")))
        .collect();
    let mut revision = Revision::sign(
        fixed::<16>(field(&vector, "vault_id_hex")),
        fixed::<16>(field(&vector, "record_id_hex")),
        fixed::<16>(field(&vector, "revision_id_hex")),
        parents,
        Operation::Write,
        decode_hex(field(&vector, "ciphertext_hex")),
        &identity,
    )
    .expect("fixture revision signs");
    let public = identity.public_identity();

    revision.ciphertext[0] ^= 1;
    assert!(revision.verify(&public).is_err());
    revision.ciphertext[0] ^= 1;
    revision.record_id[0] ^= 1;
    assert!(revision.verify(&public).is_err());
    revision.record_id[0] ^= 1;
    revision.signature[0] ^= 1;
    assert!(revision.verify(&public).is_err());
}

#[test]
fn parent_ids_use_unsigned_byte_lexicographic_order() {
    let vector: Value = serde_json::from_str(ORDER_VECTOR).expect("valid parent-order fixture");
    let identity = DeviceIdentity::from_seed_material(
        &fixed::<32>(field(&vector, "static_seed_hex")),
        &fixed::<32>(field(&vector, "signing_seed_hex")),
    );
    let parents = vector["parent_revision_ids_input_hex"]
        .as_array()
        .expect("input parent list")
        .iter()
        .map(|parent| fixed::<16>(parent.as_str().expect("parent ID is hex")))
        .collect();
    let revision = Revision::sign(
        fixed::<16>(field(&vector, "vault_id_hex")),
        fixed::<16>(field(&vector, "record_id_hex")),
        fixed::<16>(field(&vector, "revision_id_hex")),
        parents,
        Operation::Write,
        decode_hex(field(&vector, "ciphertext_hex")),
        &identity,
    )
    .expect("revision signs after sorting parents");

    let expected_parents = vector["parent_revision_ids_canonical_hex"]
        .as_array()
        .expect("canonical parent list")
        .iter()
        .map(|parent| fixed::<16>(parent.as_str().expect("parent ID is hex")))
        .collect::<Vec<_>>();
    assert_eq!(revision.parent_revision_ids, expected_parents);
    assert_eq!(
        revision.canonical_bytes().expect("canonical bytes"),
        decode_hex(field(&vector, "canonical_hex"))
    );
    assert_eq!(
        revision.signature,
        fixed::<64>(field(&vector, "signature_hex"))
    );
    assert_eq!(
        revision.hash().expect("object hash"),
        fixed::<32>(field(&vector, "object_hash_hex"))
    );
}
