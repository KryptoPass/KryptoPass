#![allow(
    clippy::expect_used,
    reason = "Property tests use expect for generated valid control fixtures and shrinking-safe assertions."
)]

use kryptopass_pairing::PairingInvitation;
use kryptopass_protocol::{HandshakeRole, NoiseHandshakeOrder, SessionCodec, proto};
use proptest::prelude::*;
use prost::Message;

fn assert_failed_decode_preserves_receive_id(frame: &[u8]) {
    let mut receiver = SessionCodec::new([0x31; 16]);
    if receiver.decode(frame).is_err() {
        let mut sender = SessionCodec::new([0x31; 16]);
        let valid_frame = sender
            .encode(
                proto::MessageType::SyncHello,
                &proto::SyncHello {
                    vault_id: vec![0x42; 16],
                    device_id: vec![0x53; 32],
                    known_revision_count: 7,
                    openpgp_fingerprint: vec![0x64; 20],
                },
            )
            .expect("encode a valid successor frame with message ID one");
        let valid_decoded = receiver
            .decode(&valid_frame)
            .expect("failed arbitrary input must not consume the receive ID");
        assert_eq!(valid_decoded.message_id, 1);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn arbitrary_bounded_frames_never_panic_and_failed_decode_preserves_receive_id(
        frame in proptest::collection::vec(any::<u8>(), 0..=65_536)
    ) {
        assert_failed_decode_preserves_receive_id(&frame);
    }

    #[test]
    fn random_payloads_inside_well_formed_envelopes_never_panic_or_consume_ids_on_error(
        payload in proptest::collection::vec(any::<u8>(), 0..=8_192)
    ) {
        let frame = proto::Envelope {
            protocol_version: 1,
            session_id: vec![0x31; 16],
            message_id: 1,
            message_type: proto::MessageType::SyncHello as i32,
            payload,
        }
        .encode_to_vec();
        assert_failed_decode_preserves_receive_id(&frame);
    }

    #[test]
    fn generated_noise_order_operations_advance_only_the_role_turn(
        initiator in any::<bool>(),
        operations in proptest::collection::vec((any::<bool>(), 0_u32..=4), 0..=64)
    ) {
        let role = if initiator {
            HandshakeRole::Initiator
        } else {
            HandshakeRole::Responder
        };
        let expected: [(bool, u32); 3] = if initiator {
            [(true, 1), (false, 2), (true, 3)]
        } else {
            [(false, 1), (true, 2), (false, 3)]
        };
        let mut order = NoiseHandshakeOrder::new(role);
        let mut accepted = 0_usize;

        for (outgoing, step) in operations {
            let is_expected = expected.get(accepted).is_some_and(|(expected_out, expected_step)| {
                outgoing == *expected_out && step == *expected_step
            });
            let result = if outgoing {
                order.record_outgoing(step)
            } else {
                order.record_incoming(step)
            };
            prop_assert_eq!(result.is_ok(), is_expected);
            if is_expected {
                accepted += 1;
            }

            let next = expected.get(accepted).copied();
            prop_assert_eq!(order.next_outgoing(), next.filter(|(outgoing, _)| *outgoing).map(|(_, step)| step));
            prop_assert_eq!(order.next_incoming(), next.filter(|(outgoing, _)| !*outgoing).map(|(_, step)| step));
            prop_assert_eq!(order.is_complete(), accepted == expected.len());
        }

        let mut canonical = NoiseHandshakeOrder::new(role);
        for (outgoing, step) in expected {
            if outgoing {
                canonical.record_outgoing(step).expect("canonical outgoing step");
            } else {
                canonical.record_incoming(step).expect("canonical incoming step");
            }
        }
        prop_assert!(canonical.is_complete());
    }

    #[test]
    fn arbitrary_bounded_pairing_qr_bytes_never_panic_and_success_roundtrips(
        qr in proptest::collection::vec(any::<u8>(), 0..=600)
    ) {
        if let Ok(invitation) = PairingInvitation::from_qr(&qr) {
            let canonical_qr = invitation
                .to_qr_bytes()
                .expect("successfully parsed invitation can be encoded");
            let reparsed = PairingInvitation::from_qr(canonical_qr.as_bytes())
                .expect("canonical invitation QR reparses");
            prop_assert_eq!(invitation.invite_id(), reparsed.invite_id());
            prop_assert_eq!(invitation.expires_at_ms(), reparsed.expires_at_ms());
            prop_assert!(canonical_qr.as_bytes().len() <= 512);
        }
    }
}
