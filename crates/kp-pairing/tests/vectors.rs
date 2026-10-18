#![allow(
    clippy::expect_used,
    reason = "Interoperability vectors are fixtures with explicit local setup failures."
)]

use kryptopass_identity::{InvitationTokenHash, verify_signature};
use kryptopass_pairing::{
    CanonicalContext, NOISE_SUITE, PairingInvitation, PairingMode, PairingRole, build_prologue,
    derive_sas, identity_proof_message, sas_emoji_at, sas_emoji_catalog,
};
use kryptopass_protocol::{encode_pairing_invite, proto};
use serde_json::Value;
use snow::{Builder, HandshakeState, params::NoiseParams};

const PAIRING_VECTOR: &str = include_str!("fixtures/pairing_v1.json");
const MATRIX_EMOJI_CATALOG: &str = include_str!("fixtures/matrix_sas_emoji_v1_16.json");

fn vector() -> Value {
    serde_json::from_str(PAIRING_VECTOR).expect("parse checked-in pairing vector")
}

fn decode_hex(value: &Value) -> Vec<u8> {
    let hex = value.as_str().expect("hex vector is a string");
    assert_eq!(hex.len() % 2, 0, "hex vector has an even number of digits");
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digits = std::str::from_utf8(pair).expect("hex digits are ASCII");
            u8::from_str_radix(digits, 16).expect("hex vector contains valid digits")
        })
        .collect()
}

fn fixed<const N: usize>(value: &Value) -> [u8; N] {
    decode_hex(value)
        .try_into()
        .unwrap_or_else(|bytes: Vec<u8>| panic!("expected {N} bytes, received {}", bytes.len()))
}

fn required<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .unwrap_or_else(|| panic!("missing vector field {key}"))
}

#[test]
fn domain_separated_pairing_prologue_context_proof_and_sas_match_literals() {
    let fixture = vector();
    let inputs = required(&fixture, "inputs");
    let expected = required(&fixture, "expected");
    let session_id = fixed::<16>(required(inputs, "session_id_hex"));
    let invite_id = fixed::<16>(required(inputs, "invite_id_hex"));
    let raw_token = fixed::<32>(required(inputs, "raw_token_hex"));
    let expires_at_ms = required(inputs, "expires_at_ms")
        .as_u64()
        .expect("expiry is an unsigned integer");
    let final_hash = fixed::<32>(required(inputs, "final_noise_hash_hex"));
    let static_i = fixed::<32>(required(inputs, "static_i_hex"));
    let static_r = fixed::<32>(required(inputs, "static_r_hex"));
    let ed_i = fixed::<32>(required(inputs, "ed_i_hex"));
    let ed_r = fixed::<32>(required(inputs, "ed_r_hex"));

    let invite_wire = proto::PairingInvite {
        invite_id: invite_id.to_vec(),
        single_use_token: raw_token.to_vec(),
        expires_at_unix_ms: expires_at_ms,
        protocol_version: 1,
    };
    let invite_qr = encode_pairing_invite(&invite_wire).expect("encode fixed QR vector");
    let invitation = PairingInvitation::from_qr(&invite_qr).expect("parse fixed QR vector");
    assert_eq!(
        *InvitationTokenHash::from_token(&raw_token).as_bytes(),
        fixed::<32>(required(expected, "invitation_token_hash_hex"))
    );

    let prologue = build_prologue(PairingMode::Pairing, session_id, Some(&invitation))
        .expect("construct canonical Noise prologue");
    assert_eq!(
        prologue.as_bytes(),
        decode_hex(required(expected, "noise_prologue_hex"))
    );

    let context = CanonicalContext::new(
        PairingMode::Pairing,
        session_id,
        invite_id,
        *InvitationTokenHash::from_token(&raw_token).as_bytes(),
        expires_at_ms,
        final_hash,
        static_i,
        static_r,
        ed_i,
        ed_r,
    )
    .expect("construct canonical identity context");
    assert_eq!(
        context.as_bytes(),
        decode_hex(required(expected, "canonical_context_hex"))
    );

    let proof_i = identity_proof_message(PairingRole::Initiator, &context);
    let proof_r = identity_proof_message(PairingRole::Responder, &context);
    assert_eq!(
        proof_i,
        decode_hex(required(expected, "initiator_proof_message_hex"))
    );
    assert_eq!(
        proof_r,
        decode_hex(required(expected, "responder_proof_message_hex"))
    );
    assert_ne!(proof_i, proof_r, "role must be part of the proof statement");

    let ed_vector = required(expected, "ed25519_proof_vector");
    let signing_key = fixed::<32>(required(ed_vector, "public_key_hex"));
    let signature = fixed::<64>(required(ed_vector, "signature_hex"));
    verify_signature(&signing_key, &proof_i, &signature)
        .expect("independent Ed25519 proof vector verifies");
    assert!(
        verify_signature(&signing_key, &proof_r, &signature).is_err(),
        "an initiator proof must not verify as the responder role"
    );

    let sas = derive_sas(&context).expect("derive canonical seven-symbol SAS");
    let expected_indices = required(expected, "sas_indices")
        .as_array()
        .expect("SAS indices are a JSON array")
        .iter()
        .map(|item| {
            u8::try_from(item.as_u64().expect("SAS index is unsigned"))
                .expect("SAS index fits in six bits")
        })
        .collect::<Vec<_>>();
    assert_eq!(sas.indices().as_slice(), expected_indices.as_slice());
    assert!(sas.indices().iter().all(|index| *index < 64));
}

#[test]
fn reconnect_has_a_distinct_zero_invitation_prologue_and_context() {
    let fixture = vector();
    let inputs = required(&fixture, "inputs");
    let expected = required(&fixture, "expected");
    let session_id = fixed::<16>(required(inputs, "session_id_hex"));
    let prologue = build_prologue(PairingMode::Reconnect, session_id, None)
        .expect("construct reconnect prologue");
    assert_eq!(
        prologue.as_bytes(),
        decode_hex(required(expected, "reconnect_prologue_hex"))
    );
    assert_ne!(
        prologue.as_bytes(),
        decode_hex(required(expected, "noise_prologue_hex"))
    );
}

#[test]
fn entire_matrix_v1_16_emoji_catalog_matches_the_official_fixture() {
    let source: Value =
        serde_json::from_str(MATRIX_EMOJI_CATALOG).expect("parse checked-in Matrix SAS catalog");
    assert_eq!(
        source.get("source_version").and_then(Value::as_str),
        Some("Matrix v1.16 sas-emoji.json")
    );
    let entries = source
        .get("emojis")
        .and_then(Value::as_array)
        .expect("catalog has an emoji array");
    let catalog = sas_emoji_catalog();
    assert_eq!(entries.len(), 64);
    assert_eq!(catalog.len(), 64);

    for (index, (entry, actual)) in entries.iter().zip(catalog).enumerate() {
        let expected_index = entry
            .get("index")
            .and_then(Value::as_u64)
            .expect("catalog index is unsigned");
        assert_eq!(expected_index, index as u64);
        assert_eq!(actual.index, index as u8);
        assert_eq!(
            entry.get("emoji").and_then(Value::as_str),
            Some(actual.emoji)
        );
        assert_eq!(entry.get("name").and_then(Value::as_str), Some(actual.name));
        assert_eq!(sas_emoji_at(index as u8), Some(actual));
    }
    assert_eq!(sas_emoji_at(64), None);
}

#[test]
fn context_fields_and_mode_are_cryptographically_bound() {
    let fixture = vector();
    let inputs = required(&fixture, "inputs");
    let session_id = fixed::<16>(required(inputs, "session_id_hex"));
    let invite_id = fixed::<16>(required(inputs, "invite_id_hex"));
    let raw_token = fixed::<32>(required(inputs, "raw_token_hex"));
    let token_hash = *InvitationTokenHash::from_token(&raw_token).as_bytes();
    let expires_at = required(inputs, "expires_at_ms")
        .as_u64()
        .expect("expiry is unsigned");
    let final_hash = fixed::<32>(required(inputs, "final_noise_hash_hex"));
    let static_i = fixed::<32>(required(inputs, "static_i_hex"));
    let static_r = fixed::<32>(required(inputs, "static_r_hex"));
    let ed_i = fixed::<32>(required(inputs, "ed_i_hex"));
    let ed_r = fixed::<32>(required(inputs, "ed_r_hex"));
    let base = CanonicalContext::new(
        PairingMode::Pairing,
        session_id,
        invite_id,
        token_hash,
        expires_at,
        final_hash,
        static_i,
        static_r,
        ed_i,
        ed_r,
    )
    .expect("construct base context");
    let mut mutated_session = session_id;
    mutated_session[0] ^= 1;
    let with_new_session = CanonicalContext::new(
        PairingMode::Pairing,
        mutated_session,
        invite_id,
        token_hash,
        expires_at,
        final_hash,
        static_i,
        static_r,
        ed_i,
        ed_r,
    )
    .expect("construct context with another session");
    assert_ne!(base.as_bytes(), with_new_session.as_bytes());

    let reconnect = CanonicalContext::new(
        PairingMode::Reconnect,
        session_id,
        [0; 16],
        [0; 32],
        0,
        final_hash,
        static_i,
        static_r,
        ed_i,
        ed_r,
    )
    .expect("construct reconnect context");
    assert_ne!(base.as_bytes(), reconnect.as_bytes());
}

#[test]
fn deterministic_noise_xx_messages_and_transcript_match_golden_fixture() {
    let fixture = vector();
    let inputs = required(&fixture, "inputs");
    let expected = required(&fixture, "expected");
    let noise = required(&fixture, "noise_xx_test_vector");
    let prologue = decode_hex(required(expected, "noise_prologue_hex"));
    let static_i = decode_hex(required(noise, "static_i_private_hex"));
    let static_r = decode_hex(required(noise, "static_r_private_hex"));
    let ephemeral_i = decode_hex(required(noise, "ephemeral_i_private_hex"));
    let ephemeral_r = decode_hex(required(noise, "ephemeral_r_private_hex"));
    let ed_i = decode_hex(required(noise, "initiator_payload_hex"));
    let ed_r = decode_hex(required(noise, "responder_payload_hex"));
    assert_eq!(static_i.len(), 32);
    assert_eq!(static_r.len(), 32);
    assert_eq!(ephemeral_i.len(), 32);
    assert_eq!(ephemeral_r.len(), 32);
    assert_eq!(ed_i.len(), 32);
    assert_eq!(ed_r.len(), 32);
    let params: NoiseParams = NOISE_SUITE.parse().expect("parse supported Noise suite");

    let mut initiator = Builder::new(params.clone())
        .local_private_key(&static_i)
        .expect("set synthetic initiator static key")
        .prologue(&prologue)
        .expect("set canonical prologue")
        .fixed_ephemeral_key_for_testing_only(&ephemeral_i)
        .build_initiator()
        .expect("build deterministic initiator");
    let mut responder = Builder::new(params)
        .local_private_key(&static_r)
        .expect("set synthetic responder static key")
        .prologue(&prologue)
        .expect("set canonical prologue")
        .fixed_ephemeral_key_for_testing_only(&ephemeral_r)
        .build_responder()
        .expect("build deterministic responder");

    let message_1 = write_noise_message(&mut initiator, &[]);
    assert_eq!(read_noise_message(&mut responder, &message_1), b"");
    let message_2 = write_noise_message(&mut responder, &ed_r);
    assert_eq!(read_noise_message(&mut initiator, &message_2), ed_r);
    let message_3 = write_noise_message(&mut initiator, &ed_i);
    assert_eq!(read_noise_message(&mut responder, &message_3), ed_i);

    let initiator_hash = initiator.get_handshake_hash();
    let responder_hash = responder.get_handshake_hash();
    let static_i_public = responder
        .get_remote_static()
        .expect("responder learned the initiator static key");
    let static_r_public = initiator
        .get_remote_static()
        .expect("initiator learned the responder static key");
    assert_eq!(initiator_hash, responder_hash);
    assert!(initiator.is_handshake_finished());
    assert!(responder.is_handshake_finished());
    assert_eq!(
        static_i_public,
        decode_hex(required(inputs, "static_i_hex"))
    );
    assert_eq!(
        static_r_public,
        decode_hex(required(inputs, "static_r_hex"))
    );
    assert_eq!(ed_i, decode_hex(required(inputs, "ed_i_hex")));
    assert_eq!(ed_r, decode_hex(required(inputs, "ed_r_hex")));
    assert_eq!(
        initiator_hash,
        fixed::<32>(required(inputs, "final_noise_hash_hex"))
    );

    for (key, actual) in [
        ("message_1_hex", message_1),
        ("message_2_hex", message_2),
        ("message_3_hex", message_3),
        ("final_noise_hash_hex", initiator_hash.to_vec()),
    ] {
        let expected_bytes = decode_hex(required(noise, key));
        assert_eq!(actual, expected_bytes, "Noise vector field {key}");
    }

    assert_eq!(
        decode_hex(required(noise, "final_noise_hash_hex")),
        initiator_hash
    );
    assert_eq!(
        decode_hex(required(noise, "static_i_public_hex")),
        static_i_public
    );
    assert_eq!(
        decode_hex(required(noise, "static_r_public_hex")),
        static_r_public
    );

    let context = CanonicalContext::new(
        PairingMode::Pairing,
        fixed::<16>(required(inputs, "session_id_hex")),
        fixed::<16>(required(inputs, "invite_id_hex")),
        *InvitationTokenHash::from_token(&fixed::<32>(required(inputs, "raw_token_hex")))
            .as_bytes(),
        required(inputs, "expires_at_ms")
            .as_u64()
            .expect("expiry is unsigned"),
        fixed::<32>(required(inputs, "final_noise_hash_hex")),
        fixed::<32>(required(inputs, "static_i_hex")),
        fixed::<32>(required(inputs, "static_r_hex")),
        fixed::<32>(required(inputs, "ed_i_hex")),
        fixed::<32>(required(inputs, "ed_r_hex")),
    )
    .expect("construct transcript-bound SAS context");
    let sas = derive_sas(&context).expect("derive SAS from Noise XX transcript");
    let expected_indices = required(noise, "sas_indices")
        .as_array()
        .expect("Noise SAS indices are an array")
        .iter()
        .map(|index| {
            u8::try_from(index.as_u64().expect("Noise SAS index is unsigned"))
                .expect("Noise SAS index fits")
        })
        .collect::<Vec<_>>();
    assert_eq!(sas.indices().as_slice(), expected_indices.as_slice());
    let expected_emojis = required(noise, "sas_emojis")
        .as_array()
        .expect("Noise SAS symbols are an array");
    assert_eq!(expected_emojis.len(), 7);
    for (entry, actual) in expected_emojis.iter().zip(sas.emojis) {
        assert_eq!(
            entry.get("index").and_then(Value::as_u64),
            Some(u64::from(actual.index))
        );
        assert_eq!(
            entry.get("emoji").and_then(Value::as_str),
            Some(actual.emoji)
        );
        assert_eq!(entry.get("name").and_then(Value::as_str), Some(actual.name));
    }
}

fn write_noise_message(state: &mut HandshakeState, payload: &[u8]) -> Vec<u8> {
    let mut output = [0_u8; 1024];
    let written = state
        .write_message(payload, &mut output)
        .expect("write Noise handshake message");
    output[..written].to_vec()
}

fn read_noise_message(state: &mut HandshakeState, frame: &[u8]) -> Vec<u8> {
    let mut output = [0_u8; 1024];
    let read = state
        .read_message(frame, &mut output)
        .expect("read Noise handshake message");
    output[..read].to_vec()
}
