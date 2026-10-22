#![allow(clippy::expect_used)] // Test fixtures use expect to attach clear setup-failure context.

use std::time::Duration;

use kryptopass_protocol::{Channel, MessagePayload, ProtocolError, SessionCodec, proto};
use kryptopass_transport::{
    MAX_TRANSPORT_FRAME_SIZE, MAX_TRANSPORT_QUEUE_CAPACITY, MemoryTransport, MemoryTransportConfig,
    PeerTransport, TransportError, TransportFrame,
};

const SESSION: [u8; 16] = [0x61; 16];

fn bytes(length: usize, value: u8) -> Vec<u8> {
    vec![value; length]
}

fn sync_hello(codec: &mut SessionCodec, salt: u8) -> Vec<u8> {
    codec
        .encode(
            proto::MessageType::SyncHello,
            &proto::SyncHello {
                vault_id: bytes(16, salt),
                device_id: bytes(32, salt.wrapping_add(1)),
                known_revision_count: u64::from(salt),
                openpgp_fingerprint: vec![],
            },
        )
        .expect("valid SyncHello")
}

fn sync_chunk(codec: &mut SessionCodec, salt: u8) -> Vec<u8> {
    codec
        .encode(
            proto::MessageType::SyncChunk,
            &proto::SyncChunk {
                transfer_id: bytes(16, salt),
                chunk_index: 0,
                chunk_count: 1,
                total_size: 1,
                sha256_digest: bytes(32, salt.wrapping_add(1)),
                data: vec![salt],
            },
        )
        .expect("valid SyncChunk")
}

fn unbounded_config() -> MemoryTransportConfig {
    MemoryTransportConfig {
        send_timeout: None,
        receive_timeout: None,
        ..MemoryTransportConfig::default()
    }
}

#[tokio::test]
async fn two_nodes_exchange_and_decode_protobuf_messages_in_both_directions() {
    let (mut a, mut b) = MemoryTransport::pair(unbounded_config()).expect("memory pair");
    let mut codec_a = SessionCodec::new(SESSION);
    let mut codec_b = SessionCodec::new(SESSION);

    let a_wire = sync_hello(&mut codec_a, 1);
    a.send(Channel::Control, &a_wire)
        .await
        .expect("A sends SyncHello");
    let received_by_b = b.receive().await.expect("B receives SyncHello");
    assert_eq!(received_by_b.channel, Channel::Control);
    let decoded_at_b = codec_b
        .decode_on(received_by_b.channel, &received_by_b.data)
        .expect("B validates A's message");
    assert_eq!(decoded_at_b.message_id, 1);
    assert!(matches!(decoded_at_b.payload, MessagePayload::SyncHello(_)));

    let b_wire = sync_hello(&mut codec_b, 2);
    b.send(Channel::Control, &b_wire)
        .await
        .expect("B sends SyncHello");
    let received_by_a = a.receive().await.expect("A receives SyncHello");
    let decoded_at_a = codec_a
        .decode_on(received_by_a.channel, &received_by_a.data)
        .expect("A validates B's message");
    assert_eq!(decoded_at_a.message_id, 1);
    assert!(matches!(decoded_at_a.payload, MessagePayload::SyncHello(_)));
}

#[tokio::test]
async fn each_direction_preserves_one_fifo_order_across_control_and_data_channels() {
    let config = MemoryTransportConfig {
        queue_capacity: 4,
        ..unbounded_config()
    };
    let (mut a, mut b) = MemoryTransport::pair(config).expect("memory pair");
    let mut sender = SessionCodec::new(SESSION);
    let mut receiver = SessionCodec::new(SESSION);
    let hello = sync_hello(&mut sender, 1);
    let chunk = sync_chunk(&mut sender, 2);
    let commit = sender
        .encode(
            proto::MessageType::SyncCommit,
            &proto::SyncCommit {
                transfer_id: bytes(16, 3),
                sha256_digest: bytes(32, 4),
            },
        )
        .expect("valid SyncCommit");

    a.send(Channel::Control, &hello).await.expect("queue hello");
    a.send(Channel::Data, &chunk).await.expect("queue chunk");
    a.send(Channel::Control, &commit)
        .await
        .expect("queue commit");

    for (expected_id, expected_type, expected_channel) in [
        (1, proto::MessageType::SyncHello, Channel::Control),
        (2, proto::MessageType::SyncChunk, Channel::Data),
        (3, proto::MessageType::SyncCommit, Channel::Control),
    ] {
        let received = b.receive().await.expect("receive in FIFO order");
        assert_eq!(received.channel, expected_channel);
        let decoded = receiver
            .decode_on(received.channel, &received.data)
            .expect("valid ordered protocol message");
        assert_eq!(decoded.message_id, expected_id);
        assert_eq!(decoded.message_type as i32, expected_type as i32);
    }
}

#[tokio::test]
async fn protocol_errors_cross_the_transport_without_consuming_the_next_sequence() {
    let (mut a, mut b) = MemoryTransport::pair(unbounded_config()).expect("memory pair");
    let mut expected_codec = SessionCodec::new(SESSION);
    let mut other_session_codec = SessionCodec::new([0x62; 16]);
    let wrong_session = sync_hello(&mut other_session_codec, 3);
    a.send(Channel::Control, &wrong_session)
        .await
        .expect("transport is opaque");
    let received = b.receive().await.expect("receive opaque bytes");
    assert!(matches!(
        expected_codec.decode_on(received.channel, &received.data),
        Err(ProtocolError::SessionMismatch)
    ));

    let malformed = [0xff, 0xff];
    a.send(Channel::Control, &malformed)
        .await
        .expect("transport forwards malformed protocol bytes");
    let received = b.receive().await.expect("receive malformed bytes");
    assert!(matches!(
        expected_codec.decode_on(received.channel, &received.data),
        Err(ProtocolError::MalformedEnvelope)
    ));

    let mut actual_sender = SessionCodec::new(SESSION);
    let valid = sync_hello(&mut actual_sender, 4);
    a.send(Channel::Control, &valid)
        .await
        .expect("send valid recovery frame");
    let received = b.receive().await.expect("receive valid recovery frame");
    let decoded = expected_codec
        .decode_on(received.channel, &received.data)
        .expect("failed frames did not consume the protocol sequence");
    assert_eq!(decoded.message_id, 1);
}

#[tokio::test]
async fn frame_and_configuration_limits_are_checked_at_the_transport_boundary() {
    let config = MemoryTransportConfig {
        queue_capacity: 1,
        max_frame_size: 3,
        ..unbounded_config()
    };
    let (mut a, mut b) = MemoryTransport::pair(config).expect("bounded pair");
    assert!(matches!(
        a.send(Channel::Control, b"four").await,
        Err(TransportError::FrameTooLarge {
            actual: 4,
            maximum: 3,
        })
    ));
    a.send(Channel::Data, b"123")
        .await
        .expect("frame exactly at configured maximum");
    let received = b.receive().await.expect("receive boundary frame");
    assert_eq!(received.channel, Channel::Data);
    assert_eq!(received.data, b"123");

    let invalid = [
        MemoryTransportConfig {
            queue_capacity: 0,
            ..unbounded_config()
        },
        MemoryTransportConfig {
            queue_capacity: MAX_TRANSPORT_QUEUE_CAPACITY + 1,
            ..unbounded_config()
        },
        MemoryTransportConfig {
            max_frame_size: 0,
            ..unbounded_config()
        },
        MemoryTransportConfig {
            max_frame_size: MAX_TRANSPORT_FRAME_SIZE + 1,
            ..unbounded_config()
        },
        MemoryTransportConfig {
            send_timeout: Some(Duration::ZERO),
            ..unbounded_config()
        },
        MemoryTransportConfig {
            receive_timeout: Some(Duration::ZERO),
            ..unbounded_config()
        },
    ];
    for config in invalid {
        assert!(matches!(
            MemoryTransport::pair(config),
            Err(TransportError::InvalidConfig { .. })
        ));
    }

    let maximum = MemoryTransportConfig {
        queue_capacity: MAX_TRANSPORT_QUEUE_CAPACITY,
        max_frame_size: MAX_TRANSPORT_FRAME_SIZE,
        ..unbounded_config()
    };
    assert!(MemoryTransport::pair(maximum).is_ok());
}

#[tokio::test]
async fn timeout_and_backpressure_are_observable_without_losing_queued_frames() {
    let config = MemoryTransportConfig {
        queue_capacity: 1,
        send_timeout: Some(Duration::from_millis(10)),
        receive_timeout: Some(Duration::from_millis(10)),
        ..MemoryTransportConfig::default()
    };
    let (mut a, mut b) = MemoryTransport::pair(config).expect("pair with deadlines");
    a.send(Channel::Control, b"first")
        .await
        .expect("first frame fills the queue");
    assert_eq!(
        a.send(Channel::Data, b"second").await,
        Err(TransportError::SendTimedOut)
    );
    let received = b.receive().await.expect("first queued frame remains");
    assert_eq!(received.data, b"first");
    assert!(matches!(
        b.receive().await,
        Err(TransportError::ReceiveTimedOut)
    ));

    let config = MemoryTransportConfig {
        queue_capacity: 1,
        send_timeout: None,
        receive_timeout: None,
        ..MemoryTransportConfig::default()
    };
    let (mut a, mut b) = MemoryTransport::pair(config).expect("pair without deadlines");
    a.send(Channel::Control, b"head")
        .await
        .expect("fill bounded queue");
    let mut blocked = Box::pin(a.send(Channel::Data, b"backpressured"));
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut blocked)
            .await
            .is_err()
    );
    assert_eq!(
        b.receive().await.expect("free one queue slot").data,
        b"head"
    );
    blocked
        .await
        .expect("pending send proceeds when capacity opens");
    assert_eq!(
        b.receive().await.expect("receive second frame").data,
        b"backpressured"
    );
}

#[tokio::test]
async fn cancellation_close_and_drop_wake_pending_operations_on_both_nodes() {
    let config = MemoryTransportConfig {
        queue_capacity: 1,
        send_timeout: None,
        receive_timeout: None,
        ..MemoryTransportConfig::default()
    };
    let (mut a, mut b) = MemoryTransport::pair(config.clone()).expect("pair");
    a.send(Channel::Control, b"queued")
        .await
        .expect("fill queue");
    let cancel = a.cancel_handle();
    let mut pending_send = Box::pin(a.send(Channel::Data, b"blocked"));
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut pending_send)
            .await
            .is_err()
    );
    cancel.cancel();
    assert_eq!(pending_send.await, Err(TransportError::Closed));
    assert!(cancel.is_cancelled());
    assert!(matches!(b.receive().await, Err(TransportError::Closed)));
    assert_eq!(
        b.send(Channel::Control, b"closed").await,
        Err(TransportError::Closed)
    );

    let (a, mut b) = MemoryTransport::pair(config.clone()).expect("pair for receive cancel");
    let cancel = a.cancel_handle();
    let mut receive_task = tokio::spawn(async move { b.receive().await });
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut receive_task)
            .await
            .is_err()
    );
    cancel.cancel();
    assert!(matches!(
        receive_task.await.expect("receive task joins"),
        Err(TransportError::Closed)
    ));

    let (mut a, mut b) = MemoryTransport::pair(config.clone()).expect("pair for close");
    a.close().await.expect("close link");
    a.close().await.expect("close is idempotent");
    assert_eq!(
        a.send(Channel::Control, b"closed").await,
        Err(TransportError::Closed)
    );
    assert!(matches!(b.receive().await, Err(TransportError::Closed)));

    let (a, mut b) = MemoryTransport::pair(config).expect("pair for drop");
    drop(a);
    assert!(matches!(b.receive().await, Err(TransportError::Closed)));
}

#[test]
fn frame_and_cancel_handle_debug_output_do_not_expose_payload_bytes() {
    let secret = b"pairing-bearer-secret";
    let frame = TransportFrame {
        channel: Channel::Control,
        data: secret.to_vec(),
    };
    let frame_debug = format!("{frame:?}");
    assert!(!frame_debug.contains("pairing-bearer-secret"));
    assert!(frame_debug.contains("data_len"));

    let (endpoint, _peer) = MemoryTransport::pair(unbounded_config()).expect("pair");
    let cancel_debug = format!("{:?}", endpoint.cancel_handle());
    assert!(!cancel_debug.contains("pairing-bearer-secret"));
}
