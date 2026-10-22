#![allow(clippy::expect_used)] // Test fixtures use expect to attach clear setup-failure context.

use std::time::Duration;

use kryptopass_protocol::{
    Channel, Fragment, FragmentAssembler, FragmentLimits, HandshakeRole,
    MAX_BUFFERED_TRANSFER_BYTES, MAX_FRAGMENT_COUNT, MAX_FRAGMENT_SIZE, MAX_FRAME_SIZE,
    MAX_PAYLOAD_SIZE, MAX_TRANSFER_SIZE, NoiseHandshakeOrder, PROTOCOL_VERSION, ProtocolError,
    SessionCodec, decode_pairing_invite, encode_pairing_invite, proto,
};
use prost::Message;
use sha2::{Digest, Sha256};

const SESSION: [u8; 16] = [0x5a; 16];

fn bytes(length: usize, value: u8) -> Vec<u8> {
    vec![value; length]
}

fn unique_id(length: usize, value: u16) -> Vec<u8> {
    let mut id = vec![0; length];
    id[0..2].copy_from_slice(&value.to_be_bytes());
    id
}

fn frame(
    session_id: &[u8],
    message_id: u64,
    message_type: i32,
    payload: &[u8],
    protocol_version: u32,
) -> Vec<u8> {
    proto::Envelope {
        protocol_version,
        session_id: session_id.to_vec(),
        message_id,
        message_type,
        payload: payload.to_vec(),
    }
    .encode_to_vec()
}

fn decode_as<M: Message>(
    message_type: proto::MessageType,
    message: &M,
) -> Result<kryptopass_protocol::DecodedMessage, ProtocolError> {
    let payload = message.encode_to_vec();
    let wire = frame(&SESSION, 1, message_type as i32, &payload, PROTOCOL_VERSION);
    SessionCodec::new(SESSION).decode(&wire)
}

fn decode_hex(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if value.len() % 2 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "hex vector has an odd length",
        )
        .into());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| -> Result<u8, Box<dyn std::error::Error>> {
            let hex = std::str::from_utf8(pair)?;
            Ok(u8::from_str_radix(hex, 16)?)
        })
        .collect()
}

fn hex_bytes(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn synthetic_transfer_vectors() -> Vec<(proto::MessageType, Vec<u8>, String, Channel)> {
    use proto::MessageType as Type;

    let transfer_id: Vec<u8> = (1..=16).collect();
    let fingerprint = bytes(20, 0x41);
    let digest = bytes(32, 0x42);
    let challenge_digest = bytes(32, 0x44);
    let id_hex = hex_bytes(&transfer_id);
    let fingerprint_hex = hex_bytes(&fingerprint);
    let digest_hex = hex_bytes(&digest);
    let challenge_digest_hex = hex_bytes(&challenge_digest);

    let offer = proto::SyntheticKeyOffer {
        transfer_id: transfer_id.clone(),
        openpgp_fingerprint: fingerprint.clone(),
        total_size: 3,
        sha256_digest: digest.clone(),
        encrypted_challenge: vec![0x43],
    };
    let decision = proto::SyntheticKeyDecision {
        transfer_id: transfer_id.clone(),
        accepted: true,
    };
    let chunk = proto::SyntheticKeyChunk {
        transfer_id: transfer_id.clone(),
        chunk_index: 0,
        chunk_count: 1,
        total_size: 1,
        sha256_digest: digest.clone(),
        data: vec![0x43],
    };
    let complete = proto::SyntheticKeyComplete {
        transfer_id: transfer_id.clone(),
    };
    let receipt = proto::SyntheticKeyReceipt {
        transfer_id: transfer_id.clone(),
        openpgp_fingerprint: fingerprint,
        challenge_sha256: challenge_digest,
    };
    let abort = proto::SyntheticKeyAbort {
        transfer_id,
        code: proto::AbortCode::UserCancelled as i32,
    };

    vec![
        (
            Type::SyntheticKeyOffer,
            offer.encode_to_vec(),
            format!("0a10{id_hex}1214{fingerprint_hex}18032220{digest_hex}2a0143"),
            Channel::Control,
        ),
        (
            Type::SyntheticKeyDecision,
            decision.encode_to_vec(),
            format!("0a10{id_hex}1001"),
            Channel::Control,
        ),
        (
            Type::SyntheticKeyChunk,
            chunk.encode_to_vec(),
            format!("0a10{id_hex}180120012a20{digest_hex}320143"),
            Channel::Data,
        ),
        (
            Type::SyntheticKeyComplete,
            complete.encode_to_vec(),
            format!("0a10{id_hex}"),
            Channel::Control,
        ),
        (
            Type::SyntheticKeyReceipt,
            receipt.encode_to_vec(),
            format!("0a10{id_hex}1214{fingerprint_hex}1a20{challenge_digest_hex}"),
            Channel::Control,
        ),
        (
            Type::SyntheticKeyAbort,
            abort.encode_to_vec(),
            format!("0a10{id_hex}1001"),
            Channel::Control,
        ),
    ]
}

fn valid_messages() -> Vec<(proto::MessageType, Vec<u8>, Channel)> {
    use proto::MessageType as Type;

    let hello = proto::PairingHello {
        invite_id: bytes(16, 3),
    };
    let noise = proto::NoiseHandshake {
        step: 1,
        handshake_message: vec![4],
    };
    let identity = proto::PairingIdentity {
        device_id: bytes(32, 5),
        signing_public_key: bytes(32, 6),
        transcript_hash: bytes(32, 7),
        proof: bytes(64, 8),
    };
    let confirmation = proto::PairingConfirmation {
        transcript_hash: bytes(32, 9),
        accepted: true,
    };
    let complete = proto::PairingComplete {
        transcript_hash: bytes(32, 10),
    };
    let abort = proto::PairingAbort { code: 1 };
    let sync_hello = proto::SyncHello {
        vault_id: bytes(16, 11),
        device_id: bytes(32, 12),
        known_revision_count: 1,
        openpgp_fingerprint: vec![],
    };
    let manifest = proto::SyncManifest {
        snapshot_id: bytes(16, 13),
        heads: vec![],
    };
    let request = proto::SyncRequest {
        snapshot_id: bytes(16, 14),
        revisions: vec![],
    };
    let sync_chunk = proto::SyncChunk {
        transfer_id: bytes(16, 15),
        chunk_index: 0,
        chunk_count: 1,
        total_size: 1,
        sha256_digest: bytes(32, 16),
        data: vec![17],
    };
    let commit = proto::SyncCommit {
        transfer_id: bytes(16, 18),
        sha256_digest: bytes(32, 19),
    };
    let ack = proto::SyncAck {
        transfer_id: bytes(16, 20),
        status: 1,
    };
    let conflict = proto::SyncConflict {
        record_id: bytes(16, 21),
        revision_ids: vec![bytes(16, 22), bytes(16, 23)],
    };
    let offer = proto::KeyTransferOffer {
        transfer_id: bytes(16, 24),
        openpgp_fingerprint: bytes(20, 25),
        total_size: 1,
    };
    let key_chunk = proto::KeyTransferChunk {
        transfer_id: bytes(16, 26),
        chunk_index: 0,
        chunk_count: 1,
        total_size: 1,
        sha256_digest: bytes(32, 27),
        data: vec![28],
    };
    let key_complete = proto::KeyTransferComplete {
        transfer_id: bytes(16, 29),
        openpgp_fingerprint: bytes(32, 30),
    };
    let key_abort = proto::KeyTransferAbort {
        transfer_id: bytes(16, 31),
        code: 1,
    };

    vec![
        (Type::PairingHello, hello.encode_to_vec(), Channel::Control),
        (
            Type::NoiseHandshake,
            noise.encode_to_vec(),
            Channel::Control,
        ),
        (
            Type::PairingIdentity,
            identity.encode_to_vec(),
            Channel::Control,
        ),
        (
            Type::PairingConfirmation,
            confirmation.encode_to_vec(),
            Channel::Control,
        ),
        (
            Type::PairingComplete,
            complete.encode_to_vec(),
            Channel::Control,
        ),
        (Type::PairingAbort, abort.encode_to_vec(), Channel::Control),
        (
            Type::SyncHello,
            sync_hello.encode_to_vec(),
            Channel::Control,
        ),
        (
            Type::SyncManifest,
            manifest.encode_to_vec(),
            Channel::Control,
        ),
        (Type::SyncRequest, request.encode_to_vec(), Channel::Control),
        (Type::SyncChunk, sync_chunk.encode_to_vec(), Channel::Data),
        (Type::SyncCommit, commit.encode_to_vec(), Channel::Control),
        (Type::SyncAck, ack.encode_to_vec(), Channel::Control),
        (
            Type::SyncConflict,
            conflict.encode_to_vec(),
            Channel::Control,
        ),
        (
            Type::KeyTransferOffer,
            offer.encode_to_vec(),
            Channel::Control,
        ),
        (
            Type::KeyTransferChunk,
            key_chunk.encode_to_vec(),
            Channel::Data,
        ),
        (
            Type::KeyTransferComplete,
            key_complete.encode_to_vec(),
            Channel::Control,
        ),
        (
            Type::KeyTransferAbort,
            key_abort.encode_to_vec(),
            Channel::Control,
        ),
    ]
}

fn push_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn append_unknown_bytes_field(output: &mut Vec<u8>, field_number: u64, data: &[u8]) {
    push_varint(output, (field_number << 3) | 2);
    push_varint(output, data.len() as u64);
    output.extend_from_slice(data);
}

fn padded_to(mut value: Vec<u8>, target: usize, field_number: u64) -> Vec<u8> {
    assert!(target > value.len());
    let mut padding_len = target - value.len() - 5;
    loop {
        let mut field_prefix = Vec::new();
        push_varint(&mut field_prefix, (field_number << 3) | 2);
        push_varint(&mut field_prefix, padding_len as u64);
        let actual = value.len() + field_prefix.len() + padding_len;
        if actual == target {
            value.extend(field_prefix);
            value.resize(target, 0);
            return value;
        }
        padding_len = padding_len.saturating_add(target.saturating_sub(actual));
        if actual > target {
            panic!("could not pad protobuf message to exact target size");
        }
    }
}

#[test]
fn every_v1_message_family_decodes_to_its_declared_channel() {
    let cases = valid_messages();
    assert_eq!(cases.len(), 17);

    for (message_type, payload, expected_channel) in cases {
        let wire = frame(&SESSION, 1, message_type as i32, &payload, PROTOCOL_VERSION);
        let decoded = SessionCodec::new(SESSION)
            .decode(&wire)
            .unwrap_or_else(|error| {
                panic!("message type {} failed: {error:?}", message_type as i32)
            });
        assert_eq!(decoded.message_type as i32, message_type as i32);
        assert_eq!(decoded.channel, expected_channel);
        assert_eq!(decoded.message_id, 1);
    }
}

#[test]
fn pairing_invites_use_the_bounded_out_of_band_codec_only() {
    let invite = proto::PairingInvite {
        invite_id: bytes(16, 1),
        single_use_token: bytes(32, 2),
        expires_at_unix_ms: 1,
        protocol_version: PROTOCOL_VERSION,
    };
    let encoded = encode_pairing_invite(&invite).expect("valid QR invitation");
    let decoded = decode_pairing_invite(&encoded).expect("valid invite");
    assert_eq!(decoded.invite_id, invite.invite_id);
    assert_eq!(decoded.single_use_token, invite.single_use_token);
    assert_eq!(decoded.expires_at_unix_ms, invite.expires_at_unix_ms);

    let exact_limit = padded_to(invite.encode_to_vec(), 512, 100);
    let decoded = decode_pairing_invite(&exact_limit).expect("512-byte invite");
    assert_eq!(decoded.invite_id, invite.invite_id);
    let mut oversized = exact_limit;
    oversized.push(0);
    assert!(matches!(
        decode_pairing_invite(&oversized),
        Err(ProtocolError::PayloadTooLarge)
    ));
    let mut incompatible = invite.clone();
    incompatible.protocol_version = PROTOCOL_VERSION + 1;
    assert!(matches!(
        encode_pairing_invite(&incompatible),
        Err(ProtocolError::UnsupportedVersion { actual: 2 })
    ));

    assert!(matches!(
        SessionCodec::new(SESSION).encode(proto::MessageType::PairingInvite, &invite),
        Err(ProtocolError::OutOfBandMessage)
    ));
    let in_session = frame(
        &SESSION,
        1,
        proto::MessageType::PairingInvite as i32,
        &invite.encode_to_vec(),
        PROTOCOL_VERSION,
    );
    assert!(matches!(
        SessionCodec::new(SESSION).decode(&in_session),
        Err(ProtocolError::OutOfBandMessage)
    ));
}

#[test]
fn codec_assigns_consecutive_ids_and_round_trips_typed_messages() {
    let message = proto::SyncHello {
        vault_id: bytes(16, 1),
        device_id: bytes(32, 2),
        known_revision_count: 0,
        openpgp_fingerprint: vec![],
    };
    let mut sender = SessionCodec::new(SESSION);
    let first = sender
        .encode(proto::MessageType::SyncHello, &message)
        .expect("first encode should succeed");
    let second = sender
        .encode(proto::MessageType::SyncHello, &message)
        .expect("second encode should succeed");
    let first_envelope = proto::Envelope::decode(first.as_slice()).expect("valid envelope");
    let second_envelope = proto::Envelope::decode(second.as_slice()).expect("valid envelope");
    assert_eq!(first_envelope.message_id, 1);
    assert_eq!(second_envelope.message_id, 2);

    let mut receiver = SessionCodec::new(SESSION);
    assert_eq!(
        receiver.decode(&first).expect("first message").message_id,
        1
    );
    assert_eq!(
        receiver.decode(&second).expect("second message").message_id,
        2
    );
}

#[test]
fn noise_xx_order_is_role_aware_and_rejected_steps_do_not_advance_state() {
    let mut initiator = NoiseHandshakeOrder::new(HandshakeRole::Initiator);
    let mut responder = NoiseHandshakeOrder::new(HandshakeRole::Responder);
    assert_eq!(initiator.next_outgoing(), Some(1));
    assert_eq!(initiator.next_incoming(), None);
    assert_eq!(responder.next_incoming(), Some(1));
    assert_eq!(responder.next_outgoing(), None);
    assert!(!initiator.is_complete());
    assert!(!responder.is_complete());

    assert!(matches!(
        initiator.record_outgoing(3),
        Err(ProtocolError::UnexpectedNoiseStep {
            expected: 1,
            actual: 3
        })
    ));
    assert!(matches!(
        responder.record_incoming(3),
        Err(ProtocolError::UnexpectedNoiseStep {
            expected: 1,
            actual: 3
        })
    ));
    assert!(matches!(
        initiator.record_incoming(2),
        Err(ProtocolError::UnexpectedNoiseStep {
            expected: 0,
            actual: 2
        })
    ));
    assert!(matches!(
        responder.record_outgoing(2),
        Err(ProtocolError::UnexpectedNoiseStep {
            expected: 0,
            actual: 2
        })
    ));
    assert!(matches!(
        initiator.record_incoming(1),
        Err(ProtocolError::UnexpectedNoiseStep {
            expected: 0,
            actual: 1
        })
    ));
    assert!(matches!(
        responder.record_outgoing(1),
        Err(ProtocolError::UnexpectedNoiseStep {
            expected: 0,
            actual: 1
        })
    ));
    assert_eq!(initiator.next_outgoing(), Some(1));
    assert_eq!(responder.next_incoming(), Some(1));

    initiator.record_outgoing(1).expect("initiator sends 1");
    assert_eq!(initiator.next_outgoing(), None);
    assert_eq!(initiator.next_incoming(), Some(2));
    assert!(matches!(
        initiator.record_outgoing(3),
        Err(ProtocolError::UnexpectedNoiseStep {
            expected: 0,
            actual: 3
        })
    ));
    assert_eq!(initiator.next_incoming(), Some(2));
    responder.record_incoming(1).expect("responder receives 1");
    assert_eq!(responder.next_incoming(), None);
    assert_eq!(responder.next_outgoing(), Some(2));
    assert!(matches!(
        responder.record_incoming(3),
        Err(ProtocolError::UnexpectedNoiseStep {
            expected: 0,
            actual: 3
        })
    ));
    assert_eq!(responder.next_outgoing(), Some(2));
    responder.record_outgoing(2).expect("responder sends 2");
    assert_eq!(responder.next_outgoing(), None);
    assert_eq!(responder.next_incoming(), Some(3));
    initiator.record_incoming(2).expect("initiator receives 2");
    assert_eq!(initiator.next_incoming(), None);
    assert_eq!(initiator.next_outgoing(), Some(3));
    initiator.record_outgoing(3).expect("initiator sends 3");
    responder.record_incoming(3).expect("responder receives 3");
    assert!(initiator.is_complete());
    assert!(responder.is_complete());
    assert!(matches!(
        initiator.record_outgoing(4),
        Err(ProtocolError::UnexpectedNoiseStep {
            expected: 0,
            actual: 4
        })
    ));
}

#[test]
fn frame_limit_is_checked_before_malformed_contents_are_parsed() {
    let oversized = vec![0xff; MAX_FRAME_SIZE + 1];
    assert!(matches!(
        SessionCodec::new(SESSION).decode(&oversized),
        Err(ProtocolError::FrameTooLarge)
    ));

    let hello = proto::SyncHello {
        vault_id: bytes(16, 1),
        device_id: bytes(32, 2),
        known_revision_count: 0,
        openpgp_fingerprint: vec![],
    };
    let valid_payload = hello.encode_to_vec();
    let valid_at_limit = frame(
        &SESSION,
        1,
        proto::MessageType::SyncHello as i32,
        &valid_payload,
        PROTOCOL_VERSION,
    );
    let exact = padded_to(valid_at_limit, MAX_FRAME_SIZE, 100);
    assert_eq!(exact.len(), MAX_FRAME_SIZE);
    assert!(SessionCodec::new(SESSION).decode(&exact).is_ok());

    let mut one_over = exact;
    one_over.push(0);
    assert!(matches!(
        SessionCodec::new(SESSION).decode(&one_over),
        Err(ProtocolError::FrameTooLarge)
    ));
}

#[test]
fn payload_limit_accepts_the_boundary_and_rejects_one_byte_over() {
    let hello = proto::SyncHello {
        vault_id: bytes(16, 1),
        device_id: bytes(32, 2),
        known_revision_count: 0,
        openpgp_fingerprint: vec![],
    };
    let payload_at_limit = padded_to(hello.encode_to_vec(), MAX_PAYLOAD_SIZE, 100);
    let exact = frame(
        &SESSION,
        1,
        proto::MessageType::SyncHello as i32,
        &payload_at_limit,
        PROTOCOL_VERSION,
    );
    assert!(SessionCodec::new(SESSION).decode(&exact).is_ok());

    let payload_over_limit = padded_to(hello.encode_to_vec(), MAX_PAYLOAD_SIZE + 1, 100);
    let over = frame(
        &SESSION,
        1,
        proto::MessageType::SyncHello as i32,
        &payload_over_limit,
        PROTOCOL_VERSION,
    );
    assert!(matches!(
        SessionCodec::new(SESSION).decode(&over),
        Err(ProtocolError::PayloadTooLarge)
    ));
}

#[test]
fn version_session_and_message_id_failures_do_not_consume_the_next_id() {
    let hello = proto::SyncHello {
        vault_id: bytes(16, 1),
        device_id: bytes(32, 2),
        known_revision_count: 0,
        openpgp_fingerprint: vec![],
    }
    .encode_to_vec();
    let valid = frame(
        &SESSION,
        1,
        proto::MessageType::SyncHello as i32,
        &hello,
        PROTOCOL_VERSION,
    );
    let mut receiver = SessionCodec::new(SESSION);

    let unsupported = frame(
        &SESSION,
        1,
        proto::MessageType::SyncHello as i32,
        &hello,
        PROTOCOL_VERSION + 1,
    );
    assert!(matches!(
        receiver.decode(&unsupported),
        Err(ProtocolError::UnsupportedVersion { actual: 2 })
    ));
    assert!(matches!(
        receiver.decode(&frame(
            &SESSION,
            0,
            proto::MessageType::SyncHello as i32,
            &hello,
            PROTOCOL_VERSION,
        )),
        Err(ProtocolError::InvalidMessageId)
    ));
    assert!(matches!(
        receiver.decode(&frame(
            &[0x33; 16],
            1,
            proto::MessageType::SyncHello as i32,
            &hello,
            PROTOCOL_VERSION,
        )),
        Err(ProtocolError::SessionMismatch)
    ));
    assert!(receiver.decode(&valid).is_ok());
    assert!(matches!(
        receiver.decode(&valid),
        Err(ProtocolError::Replay)
    ));
}

#[test]
fn rejects_zero_or_malformed_session_ids_unknown_types_and_wrong_channels() {
    let hello = proto::SyncHello {
        vault_id: bytes(16, 1),
        device_id: bytes(32, 2),
        known_revision_count: 0,
        openpgp_fingerprint: vec![],
    }
    .encode_to_vec();
    let valid = frame(
        &SESSION,
        1,
        proto::MessageType::SyncHello as i32,
        &hello,
        PROTOCOL_VERSION,
    );
    assert!(matches!(
        SessionCodec::new([0; 16]).decode(&valid),
        Err(ProtocolError::InvalidSessionId)
    ));
    assert!(matches!(
        SessionCodec::new(SESSION).decode(&frame(
            &[1; 15],
            1,
            proto::MessageType::SyncHello as i32,
            &hello,
            PROTOCOL_VERSION,
        )),
        Err(ProtocolError::InvalidSessionId)
    ));
    assert!(matches!(
        SessionCodec::new(SESSION).decode(&frame(&SESSION, 1, 0x7fff, &[], PROTOCOL_VERSION,)),
        Err(ProtocolError::UnknownMessageType { value: 0x7fff })
    ));

    let chunk = proto::SyncChunk {
        transfer_id: bytes(16, 3),
        chunk_index: 0,
        chunk_count: 1,
        total_size: 1,
        sha256_digest: bytes(32, 4),
        data: vec![5],
    };
    let chunk_wire = frame(
        &SESSION,
        1,
        proto::MessageType::SyncChunk as i32,
        &chunk.encode_to_vec(),
        PROTOCOL_VERSION,
    );
    assert!(matches!(
        SessionCodec::new(SESSION).decode_on(Channel::Control, &chunk_wire),
        Err(ProtocolError::WrongChannel {
            expected: Channel::Data,
            actual: Channel::Control,
        })
    ));
    assert!(
        SessionCodec::new(SESSION)
            .decode_on(Channel::Data, &chunk_wire)
            .is_ok()
    );
}

#[test]
fn out_of_order_and_malformed_payloads_do_not_advance_the_sequence() {
    let valid_payload = proto::SyncHello {
        vault_id: bytes(16, 1),
        device_id: bytes(32, 2),
        known_revision_count: 0,
        openpgp_fingerprint: vec![],
    }
    .encode_to_vec();
    let one = frame(
        &SESSION,
        1,
        proto::MessageType::SyncHello as i32,
        &valid_payload,
        PROTOCOL_VERSION,
    );
    let two = frame(
        &SESSION,
        2,
        proto::MessageType::SyncHello as i32,
        &valid_payload,
        PROTOCOL_VERSION,
    );
    let mut receiver = SessionCodec::new(SESSION);
    assert!(matches!(
        receiver.decode(&two),
        Err(ProtocolError::OutOfOrder)
    ));

    let malformed_payload = frame(
        &SESSION,
        1,
        proto::MessageType::SyncHello as i32,
        &[0x0a, 0x80],
        PROTOCOL_VERSION,
    );
    assert!(matches!(
        receiver.decode(&malformed_payload),
        Err(ProtocolError::MalformedPayload { .. })
    ));
    assert!(receiver.decode(&one).is_ok());
}

#[test]
fn synchronization_and_key_transfer_field_limits_are_enforced() {
    use proto::MessageType as Type;

    let mut sync_hello = proto::SyncHello {
        vault_id: bytes(16, 1),
        device_id: bytes(32, 2),
        known_revision_count: 1_000_000,
        openpgp_fingerprint: vec![],
    };
    assert!(decode_as(Type::SyncHello, &sync_hello).is_ok());
    sync_hello.known_revision_count = 1_000_001;
    assert!(matches!(
        decode_as(Type::SyncHello, &sync_hello),
        Err(ProtocolError::InvalidField {
            field: "known_revision_count"
        })
    ));

    let requested = |index: u16| proto::RequestedRevision {
        record_id: unique_id(16, index),
        revision_id: unique_id(16, index + 1_000),
    };
    let mut request = proto::SyncRequest {
        snapshot_id: bytes(16, 1),
        revisions: (1..=256).map(requested).collect(),
    };
    assert!(decode_as(Type::SyncRequest, &request).is_ok());
    request.revisions.push(requested(257));
    assert!(matches!(
        decode_as(Type::SyncRequest, &request),
        Err(ProtocolError::FragmentLimitExceeded)
    ));

    let mut conflict = proto::SyncConflict {
        record_id: bytes(16, 1),
        revision_ids: (1..=16).map(|i| bytes(16, i)).collect(),
    };
    assert!(decode_as(Type::SyncConflict, &conflict).is_ok());
    conflict.revision_ids.push(bytes(16, 17));
    assert!(matches!(
        decode_as(Type::SyncConflict, &conflict),
        Err(ProtocolError::FragmentLimitExceeded)
    ));

    let mut chunk = proto::SyncChunk {
        transfer_id: bytes(16, 1),
        chunk_index: 0,
        chunk_count: 1,
        total_size: MAX_FRAGMENT_SIZE as u64,
        sha256_digest: bytes(32, 2),
        data: bytes(MAX_FRAGMENT_SIZE, 3),
    };
    assert!(decode_as(Type::SyncChunk, &chunk).is_ok());
    chunk.data.push(4);
    chunk.total_size += 1;
    assert!(matches!(
        decode_as(Type::SyncChunk, &chunk),
        Err(ProtocolError::FragmentLimitExceeded)
    ));

    let mut malformed_chunk = proto::SyncChunk {
        transfer_id: bytes(16, 1),
        chunk_index: 0,
        chunk_count: 1,
        total_size: 1,
        sha256_digest: bytes(32, 2),
        data: vec![3],
    };
    malformed_chunk.chunk_index = 1;
    assert!(matches!(
        decode_as(Type::SyncChunk, &malformed_chunk),
        Err(ProtocolError::InvalidField {
            field: "chunk_index"
        })
    ));
    malformed_chunk.chunk_index = 0;
    malformed_chunk.chunk_count = 0;
    assert!(matches!(
        decode_as(Type::SyncChunk, &malformed_chunk),
        Err(ProtocolError::InvalidField {
            field: "chunk_index"
        })
    ));
    malformed_chunk.chunk_count = MAX_FRAGMENT_COUNT as u32 + 1;
    assert!(matches!(
        decode_as(Type::SyncChunk, &malformed_chunk),
        Err(ProtocolError::InvalidField {
            field: "chunk_index"
        })
    ));
    malformed_chunk.chunk_count = 1;
    malformed_chunk.sha256_digest.pop();
    assert!(matches!(
        decode_as(Type::SyncChunk, &malformed_chunk),
        Err(ProtocolError::InvalidField {
            field: "sha256_digest"
        })
    ));
    malformed_chunk.sha256_digest = bytes(32, 2);
    malformed_chunk.data.clear();
    assert!(matches!(
        decode_as(Type::SyncChunk, &malformed_chunk),
        Err(ProtocolError::InvalidField { field: "data" })
    ));
    malformed_chunk.data = vec![3];
    malformed_chunk.total_size = 0;
    assert!(matches!(
        decode_as(Type::SyncChunk, &malformed_chunk),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));
    malformed_chunk.total_size = 1;
    malformed_chunk.data.push(4);
    assert!(matches!(
        decode_as(Type::SyncChunk, &malformed_chunk),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));

    let mut max_transfer = proto::SyncChunk {
        transfer_id: bytes(16, 3),
        chunk_index: 0,
        chunk_count: MAX_FRAGMENT_COUNT as u32,
        total_size: MAX_TRANSFER_SIZE as u64,
        sha256_digest: bytes(32, 4),
        data: vec![5],
    };
    assert!(decode_as(Type::SyncChunk, &max_transfer).is_ok());
    max_transfer.total_size += 1;
    assert!(matches!(
        decode_as(Type::SyncChunk, &max_transfer),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));

    let impossible_count = proto::SyncChunk {
        transfer_id: bytes(16, 8),
        chunk_index: 0,
        chunk_count: MAX_FRAGMENT_COUNT as u32,
        total_size: 1,
        sha256_digest: bytes(32, 9),
        data: vec![10],
    };
    assert!(matches!(
        decode_as(Type::SyncChunk, &impossible_count),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));

    let mut offer = proto::KeyTransferOffer {
        transfer_id: bytes(16, 6),
        openpgp_fingerprint: bytes(20, 7),
        total_size: MAX_TRANSFER_SIZE as u64,
    };
    assert!(decode_as(Type::KeyTransferOffer, &offer).is_ok());
    offer.openpgp_fingerprint = bytes(32, 8);
    assert!(decode_as(Type::KeyTransferOffer, &offer).is_ok());
    offer.openpgp_fingerprint = bytes(21, 8);
    assert!(matches!(
        decode_as(Type::KeyTransferOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "openpgp_fingerprint"
        })
    ));
    offer.openpgp_fingerprint = bytes(20, 8);
    offer.total_size = 0;
    assert!(matches!(
        decode_as(Type::KeyTransferOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));
    offer.total_size = MAX_TRANSFER_SIZE as u64 + 1;
    assert!(matches!(
        decode_as(Type::KeyTransferOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));

    let malformed_key_chunk = proto::KeyTransferChunk {
        transfer_id: bytes(16, 1),
        chunk_index: 0,
        chunk_count: 1,
        total_size: 1,
        sha256_digest: bytes(31, 2),
        data: vec![3],
    };
    assert!(matches!(
        decode_as(Type::KeyTransferChunk, &malformed_key_chunk),
        Err(ProtocolError::InvalidField {
            field: "sha256_digest"
        })
    ));
}

#[test]
fn synthetic_transfer_payloads_match_shared_rust_kotlin_vectors()
-> Result<(), Box<dyn std::error::Error>> {
    let vectors = synthetic_transfer_vectors();
    let expected_types = [23, 24, 25, 26, 27, 28];
    assert_eq!(vectors.len(), expected_types.len());

    for ((message_type, payload, expected_hex, expected_channel), expected_type) in
        vectors.into_iter().zip(expected_types)
    {
        assert_eq!(message_type as i32, expected_type);
        assert_eq!(payload, decode_hex(&expected_hex)?);

        let wire = frame(&SESSION, 1, message_type as i32, &payload, PROTOCOL_VERSION);
        let decoded = SessionCodec::new(SESSION).decode_on(expected_channel, &wire)?;
        assert_eq!(decoded.message_type as i32, message_type as i32);
        assert_eq!(decoded.channel, expected_channel);
        assert!(format!("{decoded:?}").contains("[redacted]"));
    }

    let (message_type, payload, _, _) = synthetic_transfer_vectors()
        .into_iter()
        .find(|(message_type, _, _, _)| *message_type == proto::MessageType::SyntheticKeyChunk)
        .ok_or_else(|| std::io::Error::other("synthetic chunk vector is missing"))?;
    let data_frame = frame(&SESSION, 1, message_type as i32, &payload, PROTOCOL_VERSION);
    assert!(matches!(
        SessionCodec::new(SESSION).decode_on(Channel::Control, &data_frame),
        Err(ProtocolError::WrongChannel {
            expected: Channel::Data,
            actual: Channel::Control,
        })
    ));
    Ok(())
}

#[test]
fn synthetic_transfer_field_limits_reject_malformed_ids_chunks_and_challenges() {
    use proto::MessageType as Type;

    let mut offer = proto::SyntheticKeyOffer {
        transfer_id: bytes(16, 1),
        openpgp_fingerprint: bytes(20, 2),
        total_size: 1,
        sha256_digest: bytes(32, 3),
        encrypted_challenge: vec![4],
    };
    assert!(decode_as(Type::SyntheticKeyOffer, &offer).is_ok());
    offer.total_size = 1024 * 1024;
    offer.encrypted_challenge = bytes(16 * 1024, 4);
    assert!(decode_as(Type::SyntheticKeyOffer, &offer).is_ok());
    offer.transfer_id = bytes(16, 0);
    assert!(matches!(
        decode_as(Type::SyntheticKeyOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "transfer_id"
        })
    ));
    offer.transfer_id = bytes(15, 1);
    assert!(matches!(
        decode_as(Type::SyntheticKeyOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "transfer_id"
        })
    ));
    offer.transfer_id = bytes(16, 1);
    offer.openpgp_fingerprint = bytes(19, 2);
    assert!(matches!(
        decode_as(Type::SyntheticKeyOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "openpgp_fingerprint"
        })
    ));
    offer.openpgp_fingerprint = bytes(20, 2);
    offer.sha256_digest = bytes(31, 3);
    assert!(matches!(
        decode_as(Type::SyntheticKeyOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "sha256_digest"
        })
    ));
    offer.sha256_digest = bytes(32, 3);
    offer.total_size = 0;
    assert!(matches!(
        decode_as(Type::SyntheticKeyOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));
    offer.total_size = 1024 * 1024 + 1;
    assert!(matches!(
        decode_as(Type::SyntheticKeyOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));
    offer.total_size = 1;
    offer.encrypted_challenge.clear();
    assert!(matches!(
        decode_as(Type::SyntheticKeyOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "encrypted_challenge"
        })
    ));
    offer.encrypted_challenge = bytes(16 * 1024 + 1, 4);
    assert!(matches!(
        decode_as(Type::SyntheticKeyOffer, &offer),
        Err(ProtocolError::InvalidField {
            field: "encrypted_challenge"
        })
    ));

    let mut decision = proto::SyntheticKeyDecision {
        transfer_id: bytes(16, 1),
        accepted: false,
    };
    assert!(decode_as(Type::SyntheticKeyDecision, &decision).is_ok());
    decision.transfer_id.clear();
    assert!(matches!(
        decode_as(Type::SyntheticKeyDecision, &decision),
        Err(ProtocolError::InvalidField {
            field: "transfer_id"
        })
    ));

    let mut chunk = proto::SyntheticKeyChunk {
        transfer_id: bytes(16, 1),
        chunk_index: 0,
        chunk_count: 1,
        total_size: 32 * 1024,
        sha256_digest: bytes(32, 2),
        data: bytes(32 * 1024, 3),
    };
    assert!(decode_as(Type::SyntheticKeyChunk, &chunk).is_ok());
    chunk.total_size = 1024 * 1024;
    chunk.chunk_count = 32;
    assert!(decode_as(Type::SyntheticKeyChunk, &chunk).is_ok());
    chunk.transfer_id = bytes(15, 1);
    assert!(matches!(
        decode_as(Type::SyntheticKeyChunk, &chunk),
        Err(ProtocolError::InvalidField {
            field: "transfer_id"
        })
    ));
    chunk.transfer_id = bytes(16, 1);
    chunk.chunk_index = chunk.chunk_count;
    assert!(matches!(
        decode_as(Type::SyntheticKeyChunk, &chunk),
        Err(ProtocolError::InvalidField {
            field: "chunk_index"
        })
    ));
    chunk.chunk_index = 0;
    chunk.chunk_count = 0;
    assert!(matches!(
        decode_as(Type::SyntheticKeyChunk, &chunk),
        Err(ProtocolError::InvalidField {
            field: "chunk_index"
        })
    ));
    chunk.chunk_count = 1;
    chunk.sha256_digest.pop();
    assert!(matches!(
        decode_as(Type::SyntheticKeyChunk, &chunk),
        Err(ProtocolError::InvalidField {
            field: "sha256_digest"
        })
    ));
    chunk.sha256_digest = bytes(32, 2);
    chunk.total_size = 0;
    assert!(matches!(
        decode_as(Type::SyntheticKeyChunk, &chunk),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));
    chunk.total_size = 1024 * 1024 + 1;
    assert!(matches!(
        decode_as(Type::SyntheticKeyChunk, &chunk),
        Err(ProtocolError::InvalidField {
            field: "total_size"
        })
    ));
    chunk.total_size = 1;
    chunk.data.clear();
    assert!(matches!(
        decode_as(Type::SyntheticKeyChunk, &chunk),
        Err(ProtocolError::InvalidField { field: "data" })
    ));
    chunk.data = bytes(32 * 1024 + 1, 3);
    assert!(matches!(
        decode_as(Type::SyntheticKeyChunk, &chunk),
        Err(ProtocolError::InvalidField { .. })
    ));

    let mut complete = proto::SyntheticKeyComplete {
        transfer_id: bytes(16, 1),
    };
    assert!(decode_as(Type::SyntheticKeyComplete, &complete).is_ok());
    complete.transfer_id = bytes(16, 0);
    assert!(matches!(
        decode_as(Type::SyntheticKeyComplete, &complete),
        Err(ProtocolError::InvalidField {
            field: "transfer_id"
        })
    ));

    let mut receipt = proto::SyntheticKeyReceipt {
        transfer_id: bytes(16, 1),
        openpgp_fingerprint: bytes(20, 2),
        challenge_sha256: bytes(32, 3),
    };
    assert!(decode_as(Type::SyntheticKeyReceipt, &receipt).is_ok());
    receipt.openpgp_fingerprint.pop();
    assert!(matches!(
        decode_as(Type::SyntheticKeyReceipt, &receipt),
        Err(ProtocolError::InvalidField {
            field: "openpgp_fingerprint"
        })
    ));
    receipt.openpgp_fingerprint = bytes(20, 2);
    receipt.challenge_sha256 = bytes(31, 3);
    assert!(matches!(
        decode_as(Type::SyntheticKeyReceipt, &receipt),
        Err(ProtocolError::InvalidField {
            field: "challenge_sha256"
        })
    ));
    receipt.transfer_id = bytes(16, 0);
    receipt.challenge_sha256 = bytes(32, 3);
    assert!(matches!(
        decode_as(Type::SyntheticKeyReceipt, &receipt),
        Err(ProtocolError::InvalidField {
            field: "transfer_id"
        })
    ));

    let mut abort = proto::SyntheticKeyAbort {
        transfer_id: bytes(16, 1),
        code: proto::AbortCode::UserCancelled as i32,
    };
    assert!(decode_as(Type::SyntheticKeyAbort, &abort).is_ok());
    for code in [0, 99] {
        abort.code = code;
        assert!(matches!(
            decode_as(Type::SyntheticKeyAbort, &abort),
            Err(ProtocolError::InvalidField { field: "code" })
        ));
    }
    abort.transfer_id = bytes(15, 1);
    abort.code = proto::AbortCode::Failed as i32;
    assert!(matches!(
        decode_as(Type::SyntheticKeyAbort, &abort),
        Err(ProtocolError::InvalidField {
            field: "transfer_id"
        })
    ));
}

#[test]
fn synthetic_transfer_session_hello_keeps_capability_disabled_by_default()
-> Result<(), Box<dyn std::error::Error>> {
    let hello = proto::SessionHello {
        session_version: 1,
        device_id: bytes(32, 1),
        reconnect_port: 0,
        synthetic_key_transfer_version: 0,
        sync_version: 0,
    };
    let decoded = decode_as(proto::MessageType::SessionHello, &hello)?;
    let kryptopass_protocol::MessagePayload::SessionHello(decoded_hello) = decoded.payload else {
        return Err(
            std::io::Error::other("session hello decoded as the wrong payload type").into(),
        );
    };
    assert_eq!(decoded_hello.synthetic_key_transfer_version, 0);
    Ok(())
}

#[test]
fn synthetic_transfer_wire_debug_output_redacts_payload_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let marker = b"synthetic-test-payload-marker";
    let offer = proto::SyntheticKeyOffer {
        transfer_id: bytes(16, 1),
        openpgp_fingerprint: bytes(20, 2),
        total_size: 1,
        sha256_digest: bytes(32, 3),
        encrypted_challenge: marker.to_vec(),
    };
    let decoded = decode_as(proto::MessageType::SyntheticKeyOffer, &offer)?;
    let debug = format!("{decoded:?}");
    assert!(debug.contains("[redacted]"));
    assert!(!debug.contains(std::str::from_utf8(marker)?));
    Ok(())
}

#[test]
fn required_fields_and_enum_ranges_are_validated_for_each_message_family() {
    use proto::MessageType as Type;

    let mut invite = proto::PairingInvite {
        invite_id: bytes(16, 1),
        single_use_token: bytes(32, 2),
        expires_at_unix_ms: 1,
        protocol_version: PROTOCOL_VERSION,
    };
    invite.invite_id = bytes(15, 1);
    assert!(matches!(
        encode_pairing_invite(&invite),
        Err(ProtocolError::InvalidField { field: "invite_id" })
    ));
    invite.invite_id = bytes(16, 1);
    invite.single_use_token = bytes(31, 2);
    assert!(matches!(
        encode_pairing_invite(&invite),
        Err(ProtocolError::InvalidField {
            field: "single_use_token"
        })
    ));
    invite.single_use_token = bytes(32, 2);
    invite.expires_at_unix_ms = 0;
    assert!(matches!(
        encode_pairing_invite(&invite),
        Err(ProtocolError::InvalidField {
            field: "expires_at_unix_ms"
        })
    ));

    let mut hello = proto::PairingHello {
        invite_id: bytes(16, 3),
    };
    hello.invite_id = bytes(16, 0);
    assert!(matches!(
        decode_as(Type::PairingHello, &hello),
        Err(ProtocolError::InvalidField { field: "invite_id" })
    ));

    let mut noise = proto::NoiseHandshake {
        step: 1,
        handshake_message: vec![4],
    };
    for step in [0, 4] {
        noise.step = step;
        assert!(matches!(
            decode_as(Type::NoiseHandshake, &noise),
            Err(ProtocolError::InvalidField { field: "step" })
        ));
    }
    noise.step = 3;
    noise.handshake_message.clear();
    assert!(matches!(
        decode_as(Type::NoiseHandshake, &noise),
        Err(ProtocolError::InvalidField {
            field: "handshake_message"
        })
    ));

    let mut identity = proto::PairingIdentity {
        device_id: bytes(32, 5),
        signing_public_key: bytes(32, 6),
        transcript_hash: bytes(32, 7),
        proof: bytes(64, 8),
    };
    identity.device_id = bytes(32, 0);
    assert!(matches!(
        decode_as(Type::PairingIdentity, &identity),
        Err(ProtocolError::InvalidField { field: "device_id" })
    ));
    identity.device_id = bytes(32, 5);
    identity.signing_public_key.pop();
    assert!(matches!(
        decode_as(Type::PairingIdentity, &identity),
        Err(ProtocolError::InvalidField {
            field: "signing_public_key"
        })
    ));
    identity.signing_public_key = bytes(32, 6);
    identity.transcript_hash.pop();
    assert!(matches!(
        decode_as(Type::PairingIdentity, &identity),
        Err(ProtocolError::InvalidField {
            field: "transcript_hash"
        })
    ));
    identity.transcript_hash = bytes(32, 7);
    identity.proof.pop();
    assert!(matches!(
        decode_as(Type::PairingIdentity, &identity),
        Err(ProtocolError::InvalidField { field: "proof" })
    ));

    let confirmation = proto::PairingConfirmation {
        transcript_hash: bytes(31, 9),
        accepted: true,
    };
    assert!(matches!(
        decode_as(Type::PairingConfirmation, &confirmation),
        Err(ProtocolError::InvalidField {
            field: "transcript_hash"
        })
    ));
    let complete = proto::PairingComplete {
        transcript_hash: bytes(33, 10),
    };
    assert!(matches!(
        decode_as(Type::PairingComplete, &complete),
        Err(ProtocolError::InvalidField {
            field: "transcript_hash"
        })
    ));

    for code in [0, 99] {
        assert!(matches!(
            decode_as(Type::PairingAbort, &proto::PairingAbort { code }),
            Err(ProtocolError::InvalidField { field: "code" })
        ));
    }

    let mut manifest = proto::SyncManifest {
        snapshot_id: bytes(16, 1),
        heads: vec![proto::RevisionHead {
            record_id: bytes(16, 2),
            revision_id: bytes(16, 3),
            revision_hash: bytes(32, 4),
        }],
    };
    manifest.heads[0].revision_hash.pop();
    assert!(matches!(
        decode_as(Type::SyncManifest, &manifest),
        Err(ProtocolError::InvalidField {
            field: "revision_hash"
        })
    ));
    manifest.heads[0].revision_hash = bytes(32, 4);
    manifest.heads.push(manifest.heads[0].clone());
    assert!(matches!(
        decode_as(Type::SyncManifest, &manifest),
        Err(ProtocolError::InvalidField {
            field: "duplicate_manifest_revision"
        })
    ));

    let mut commit = proto::SyncCommit {
        transfer_id: bytes(16, 1),
        sha256_digest: bytes(32, 2),
    };
    commit.sha256_digest.pop();
    assert!(matches!(
        decode_as(Type::SyncCommit, &commit),
        Err(ProtocolError::InvalidField {
            field: "sha256_digest"
        })
    ));
    for status in [0, 99] {
        assert!(matches!(
            decode_as(
                Type::SyncAck,
                &proto::SyncAck {
                    transfer_id: bytes(16, 1),
                    status,
                }
            ),
            Err(ProtocolError::InvalidField { field: "status" })
        ));
    }

    let mut request = proto::SyncRequest {
        snapshot_id: bytes(16, 1),
        revisions: vec![proto::RequestedRevision {
            record_id: bytes(16, 2),
            revision_id: bytes(16, 3),
        }],
    };
    request.revisions.push(request.revisions[0].clone());
    assert!(matches!(
        decode_as(Type::SyncRequest, &request),
        Err(ProtocolError::InvalidField {
            field: "duplicate_requested_revision"
        })
    ));

    let mut key_complete = proto::KeyTransferComplete {
        transfer_id: bytes(16, 1),
        openpgp_fingerprint: bytes(19, 2),
    };
    assert!(matches!(
        decode_as(Type::KeyTransferComplete, &key_complete),
        Err(ProtocolError::InvalidField {
            field: "openpgp_fingerprint"
        })
    ));
    key_complete.openpgp_fingerprint = bytes(32, 2);
    assert!(decode_as(Type::KeyTransferComplete, &key_complete).is_ok());

    for code in [0, 99] {
        assert!(matches!(
            decode_as(
                Type::KeyTransferAbort,
                &proto::KeyTransferAbort {
                    transfer_id: bytes(16, 1),
                    code,
                }
            ),
            Err(ProtocolError::InvalidField { field: "code" })
        ));
    }
}

#[test]
fn protobuf_preflight_caps_repeated_items_before_typed_decode() {
    let manifest_payload = proto::SyncManifest {
        snapshot_id: bytes(16, 1),
        heads: vec![],
    }
    .encode_to_vec();
    let mut too_many_heads = manifest_payload;
    for _ in 0..=1024 {
        // Repeated field 2, length-delimited, with an empty nested message.
        too_many_heads.extend_from_slice(&[0x12, 0x00]);
    }
    assert!(matches!(
        SessionCodec::new(SESSION).decode(&frame(
            &SESSION,
            1,
            proto::MessageType::SyncManifest as i32,
            &too_many_heads,
            PROTOCOL_VERSION,
        )),
        Err(ProtocolError::FragmentLimitExceeded)
    ));

    let mut max_count_manifest = proto::SyncManifest {
        snapshot_id: bytes(16, 4),
        heads: vec![],
    }
    .encode_to_vec();
    for _ in 0..1024 {
        // The maximum count passes the preflight cap; typed validation then
        // rejects the empty nested records for their missing IDs.
        max_count_manifest.extend_from_slice(&[0x12, 0x00]);
    }
    assert!(matches!(
        SessionCodec::new(SESSION).decode(&frame(
            &SESSION,
            1,
            proto::MessageType::SyncManifest as i32,
            &max_count_manifest,
            PROTOCOL_VERSION,
        )),
        Err(ProtocolError::InvalidField { field: "record_id" })
    ));

    let mut too_many_conflicts = proto::SyncConflict {
        record_id: bytes(16, 1),
        revision_ids: vec![],
    }
    .encode_to_vec();
    for _ in 0..17 {
        append_unknown_bytes_field(&mut too_many_conflicts, 2, &bytes(16, 2));
    }
    assert!(matches!(
        SessionCodec::new(SESSION).decode(&frame(
            &SESSION,
            1,
            proto::MessageType::SyncConflict as i32,
            &too_many_conflicts,
            PROTOCOL_VERSION,
        )),
        Err(ProtocolError::FragmentLimitExceeded)
    ));
}

#[test]
fn malformed_wire_bytes_never_panic_or_escape_protocol_errors() {
    let mut seed = 0x4d59_5df4_d0f3_3173_u64;
    for length in 0..=256 {
        for _ in 0..4 {
            let mut candidate = Vec::with_capacity(length);
            for _ in 0..length {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                candidate.push((seed >> 32) as u8);
            }
            let outcome = std::panic::catch_unwind(|| {
                let _ = SessionCodec::new(SESSION).decode(&candidate);
            });
            assert!(outcome.is_ok(), "decoder panicked for {length} input bytes");
        }
    }
}

fn small_limits() -> FragmentLimits {
    FragmentLimits {
        max_active_transfers: 1,
        max_total_size: 8,
        max_fragments: 4,
        max_fragment_size: 4,
        max_buffered_bytes: 8,
        transfer_timeout: Duration::from_secs(5),
    }
}

fn fragment<'a>(
    transfer_id: &'a [u8],
    index: u32,
    count: u32,
    total_size: usize,
    digest: &'a [u8],
    data: &'a [u8],
) -> Fragment<'a> {
    Fragment {
        transfer_id,
        index,
        count,
        total_size,
        digest,
        data,
    }
}

#[test]
fn fragment_assembler_reassembles_out_of_order_and_is_idempotent_for_identical_duplicates() {
    let all = b"abcdefgh";
    let digest = Sha256::digest(all);
    let transfer_id = [0x31; 16];
    let mut assembler = FragmentAssembler::new(small_limits()).expect("valid limits");

    assert!(
        assembler
            .push(fragment(&transfer_id, 1, 2, 8, &digest, b"efgh"))
            .expect("first fragment")
            .is_none()
    );
    assert_eq!(assembler.active_transfers(), 1);
    assert_eq!(assembler.reserved_bytes(), 8);
    assert!(
        assembler
            .push(fragment(&transfer_id, 1, 2, 8, &digest, b"efgh"))
            .expect("identical duplicate is idempotent")
            .is_none()
    );
    assert!(matches!(
        assembler.push(fragment(&transfer_id, 1, 2, 8, &digest, b"xxxx")),
        Err(ProtocolError::ConflictingFragment)
    ));

    let assembled = assembler
        .push(fragment(&transfer_id, 0, 2, 8, &digest, b"abcd"))
        .expect("completion")
        .expect("all fragments present");
    assert_eq!(assembled, all);
    assert_eq!(assembler.active_transfers(), 0);
    assert_eq!(assembler.reserved_bytes(), 0);
}

#[test]
fn incomplete_assemblies_are_bounded_and_cancel_releases_the_reservation() {
    let digest = Sha256::digest(b"abcdefgh");
    let first_id = [0x41; 16];
    let second_id = [0x42; 16];
    let mut assembler = FragmentAssembler::new(small_limits()).expect("valid limits");
    assert!(
        assembler
            .push(fragment(&first_id, 0, 2, 8, &digest, b"abcd"))
            .expect("incomplete transfer")
            .is_none()
    );
    assert_eq!(assembler.reserved_bytes(), 8);
    assert!(matches!(
        assembler.push(fragment(&second_id, 0, 1, 1, &digest, b"x")),
        Err(ProtocolError::TooManyTransfers)
    ));
    assert!(assembler.cancel(&first_id));
    assert!(!assembler.cancel(&first_id));
    assert_eq!(assembler.active_transfers(), 0);
    assert_eq!(assembler.reserved_bytes(), 0);
    let retry_digest = Sha256::digest(b"x");
    assert!(
        assembler
            .push(fragment(&second_id, 0, 1, 1, &retry_digest, b"x"))
            .expect("a new transfer can use the released budget")
            .is_some()
    );
}

#[test]
fn assembler_accepts_its_maximum_fragment_count_and_enforces_declared_byte_budget() {
    let data = b"abcd";
    let digest = Sha256::digest(data);
    let transfer_id = [0x49; 16];
    let mut assembler = FragmentAssembler::new(small_limits()).expect("valid limits");
    for (index, byte) in data.iter().enumerate() {
        let result = assembler
            .push(fragment(
                &transfer_id,
                index as u32,
                4,
                4,
                &digest,
                std::slice::from_ref(byte),
            ))
            .expect("within the configured fragment-count limit");
        if index < 3 {
            assert!(result.is_none());
        } else {
            assert_eq!(result.as_deref(), Some(data.as_slice()));
        }
    }

    let limits = FragmentLimits::default();
    let mut assembler = FragmentAssembler::new(limits).expect("default limits");
    let digest = [0x88; 32];
    let max_transfer_id = [0x71; 16];
    assert!(
        assembler
            .push(fragment(
                &max_transfer_id,
                0,
                MAX_FRAGMENT_COUNT as u32,
                MAX_TRANSFER_SIZE,
                &digest,
                b"x",
            ))
            .expect("maximum incomplete transfer")
            .is_none()
    );
    assert_eq!(assembler.active_transfers(), 1);
    assert_eq!(assembler.reserved_bytes(), MAX_BUFFERED_TRANSFER_BYTES);
    assert!(matches!(
        assembler.push(fragment(&[0x72; 16], 0, 1, 1, &digest, b"x")),
        Err(ProtocolError::TooManyTransfers)
    ));
    assert!(assembler.cancel(&max_transfer_id));

    let half = MAX_BUFFERED_TRANSFER_BYTES / 2;
    for transfer_id in [[0x73; 16], [0x74; 16]] {
        assert!(
            assembler
                .push(fragment(
                    &transfer_id,
                    0,
                    MAX_FRAGMENT_COUNT as u32,
                    half,
                    &digest,
                    b"x",
                ))
                .expect("bounded incomplete transfer")
                .is_none()
        );
    }
    assert_eq!(assembler.active_transfers(), 2);
    assert_eq!(assembler.reserved_bytes(), MAX_BUFFERED_TRANSFER_BYTES);
    assert!(matches!(
        assembler.push(fragment(&[0x75; 16], 0, 1, 1, &digest, b"x")),
        Err(ProtocolError::TooManyTransfers)
    ));
}

#[test]
fn fragment_assembler_rejects_zero_or_over_ceiling_configuration_limits() {
    let valid = FragmentLimits::default();
    assert!(FragmentAssembler::new(valid).is_ok());

    let invalid = [
        FragmentLimits {
            max_active_transfers: 0,
            ..valid
        },
        FragmentLimits {
            max_active_transfers: 3,
            ..valid
        },
        FragmentLimits {
            max_total_size: 0,
            ..valid
        },
        FragmentLimits {
            max_total_size: MAX_TRANSFER_SIZE + 1,
            ..valid
        },
        FragmentLimits {
            max_fragments: 0,
            ..valid
        },
        FragmentLimits {
            max_fragments: MAX_FRAGMENT_COUNT + 1,
            ..valid
        },
        FragmentLimits {
            max_fragment_size: 0,
            ..valid
        },
        FragmentLimits {
            max_fragment_size: MAX_FRAGMENT_SIZE + 1,
            ..valid
        },
        FragmentLimits {
            max_buffered_bytes: 0,
            ..valid
        },
        FragmentLimits {
            max_buffered_bytes: 16 * 1024 * 1024 + 1,
            ..valid
        },
        FragmentLimits {
            transfer_timeout: Duration::ZERO,
            ..valid
        },
    ];
    for limits in invalid {
        assert!(matches!(
            FragmentAssembler::new(limits),
            Err(ProtocolError::InvalidFragmentLimits)
        ));
    }
}

#[test]
fn fragment_assembler_rejects_bad_metadata_limits_and_corruption() {
    let transfer_id = [0x51; 16];
    let digest = Sha256::digest(b"abcdefgh");
    let mut assembler = FragmentAssembler::new(small_limits()).expect("valid limits");

    assert!(matches!(
        assembler.push(fragment(&transfer_id, 0, 0, 8, &digest, b"abcd")),
        Err(ProtocolError::FragmentLimitExceeded)
    ));
    assert!(matches!(
        assembler.push(fragment(&transfer_id, 0, 5, 8, &digest, b"abcd")),
        Err(ProtocolError::FragmentLimitExceeded)
    ));
    assert!(matches!(
        assembler.push(fragment(&transfer_id, 0, 2, 9, &digest, b"abcd")),
        Err(ProtocolError::FragmentLimitExceeded)
    ));
    assert!(matches!(
        assembler.push(fragment(&transfer_id, 0, 2, 1, &digest, b"a")),
        Err(ProtocolError::FragmentLimitExceeded)
    ));
    assert!(matches!(
        assembler.push(fragment(&transfer_id, 0, 2, 8, &digest[..31], b"abcd")),
        Err(ProtocolError::InvalidFragment)
    ));

    assert!(
        assembler
            .push(fragment(&transfer_id, 0, 2, 8, &digest, b"abcd"))
            .expect("first fragment")
            .is_none()
    );
    assert!(matches!(
        assembler.push(fragment(&transfer_id, 1, 2, 8, &digest, b"efg")),
        Err(ProtocolError::InvalidFragment)
    ));
    assert_eq!(assembler.active_transfers(), 0);
    assert_eq!(assembler.reserved_bytes(), 0);

    let bad_digest = [0x99; 32];
    assert!(
        assembler
            .push(fragment(&transfer_id, 0, 2, 8, &bad_digest, b"abcd"))
            .expect("first corrupted transfer fragment")
            .is_none()
    );
    assert!(matches!(
        assembler.push(fragment(&transfer_id, 1, 2, 8, &bad_digest, b"efgh")),
        Err(ProtocolError::FragmentDigestMismatch)
    ));
    assert_eq!(assembler.active_transfers(), 0);
    assert_eq!(assembler.reserved_bytes(), 0);
}

#[test]
fn incomplete_fragment_assemblies_expire_and_release_reserved_bytes() {
    let mut limits = small_limits();
    limits.transfer_timeout = Duration::from_millis(2);
    let mut assembler = FragmentAssembler::new(limits).expect("valid limits");
    let data = b"abcdefgh";
    let digest = Sha256::digest(data);
    let transfer_id = [0x61; 16];
    assert!(
        assembler
            .push(fragment(&transfer_id, 0, 2, 8, &digest, b"abcd"))
            .expect("incomplete transfer")
            .is_none()
    );
    assert_eq!(assembler.reserved_bytes(), 8);
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(assembler.expire_stale(), 1);
    assert_eq!(assembler.active_transfers(), 0);
    assert_eq!(assembler.reserved_bytes(), 0);

    // An expired transfer ID can begin a fresh assembly without stale state.
    let short_digest = Sha256::digest(b"abcd");
    assert!(
        assembler
            .push(fragment(&transfer_id, 0, 1, 4, &short_digest, b"abcd"))
            .expect("reuse after expiry")
            .is_some()
    );
}
