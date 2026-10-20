#![allow(
    clippy::expect_used,
    reason = "Integration test setup and protocol assertions use expect with local failure context."
)]

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use kryptopass_lan::{IceServerConfig, InternetIceConfig, LanError, RelayHost, RelayRoute};
use tokio::{
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_tungstenite::{
    WebSocketStream, accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{Request, Response},
    },
};

const ROOM: [u8; 32] = [0x41; 32];
const SESSION: [u8; 16] = [0x72; 16];
const INITIATOR_DEVICE: [u8; 32] = [0x5a; 32];
const SHORT_TIMEOUT: Duration = Duration::from_secs(3);

#[test]
fn relay_route_rejects_malformed_or_unsafe_origins_and_rooms() {
    let invalid_origins = [
        "",
        "not a URL",
        "https://signal.example",
        "ws://signal.example",
        "ws://192.0.2.10",
        "wss://signal.example?token=secret",
        "wss://signal.example#fragment",
        "wss://signal.example/relay",
        "wss://user@signal.example",
        "wss://user:password@signal.example",
        "wss://signal.example:0",
    ];

    for origin in invalid_origins {
        assert!(
            matches!(
                RelayRoute::new(origin, ROOM),
                Err(LanError::InvalidRelayRoute)
            ),
            "origin should be rejected: {origin:?}"
        );
    }

    assert!(matches!(
        RelayRoute::new("wss://signal.example", [0; 32]),
        Err(LanError::InvalidRelayRoute)
    ));

    let local_websocket = RelayRoute::new("ws://127.0.0.1:8080/", ROOM);
    if cfg!(debug_assertions) {
        assert!(
            local_websocket.is_ok(),
            "debug builds allow loopback ws tests"
        );
    } else {
        assert!(matches!(local_websocket, Err(LanError::InvalidRelayRoute)));
    }

    let route = RelayRoute::new("wss://SIGNAL.example:443/", ROOM)
        .expect("a TLS relay origin with an optional root slash is valid");
    assert_eq!(route.relay_origin(), "wss://signal.example");
    assert_eq!(route.room(), ROOM);
    assert_eq!(route.room_hex(), "41".repeat(32));
}

#[tokio::test]
async fn relay_host_connect_validates_ice_bounds_before_network_access() {
    let unreachable_route = RelayRoute::new("wss://no-dns-needed.invalid", ROOM)
        .expect("syntactically valid relay origin");
    let invalid_configs = [
        InternetIceConfig::new(vec![IceServerConfig::stun("https://stun.example")]),
        InternetIceConfig::new(vec![IceServerConfig::turn(
            "turn:turn.example:3478",
            "user",
            "",
        )]),
        InternetIceConfig::new(vec![IceServerConfig::with_urls(
            vec![
                "stun:stun.example:3478".to_owned(),
                "turn:turn.example:3478".to_owned(),
            ],
            "user",
            "secret",
        )]),
        InternetIceConfig::new(vec![IceServerConfig::with_urls(
            vec!["stun:stun.example:3478".to_owned()],
            "unexpected-user",
            "unexpected-credential",
        )]),
        InternetIceConfig::new(
            (0..5)
                .map(|_| IceServerConfig::stun("stun:stun.example:3478"))
                .collect(),
        ),
        InternetIceConfig::new(vec![IceServerConfig::with_urls(
            (0..5)
                .map(|port| format!("stun:stun.example:{}", 3478 + port))
                .collect(),
            "",
            "",
        )]),
        InternetIceConfig::new(vec![IceServerConfig::turn(
            "turn:turn.example:3478",
            "u".repeat(600),
            "c".repeat(600),
        )]),
    ];

    for config in invalid_configs {
        let result = timeout(
            SHORT_TIMEOUT,
            RelayHost::connect(unreachable_route.clone(), config),
        )
        .await
        .expect("invalid configuration fails before a network timeout");
        assert!(matches!(result, Err(LanError::InvalidIceConfig)));
    }

    let default_servers = InternetIceConfig::default();
    assert_eq!(
        default_servers.servers()[0].urls(),
        &["stun:stun.cloudflare.com:3478".to_owned()]
    );
}

#[tokio::test]
async fn relay_host_connect_accepts_bounded_stun_turn_configuration() {
    let (route, listener) = local_relay().await;
    let expected_path = relay_host_path();
    let broker = tokio::spawn(async move {
        let mut websocket = accept_local_websocket(listener, &expected_path).await;
        let _ = timeout(SHORT_TIMEOUT, websocket.next()).await;
    });

    let config = InternetIceConfig::new(vec![
        IceServerConfig::with_urls(
            vec![
                "stun:stun-one.example:3478".to_owned(),
                "stun:stun-one.example:3479".to_owned(),
                "stun:stun-one.example:3480".to_owned(),
                "stun:stun-one.example:3481".to_owned(),
            ],
            "",
            "",
        ),
        IceServerConfig::with_urls(
            vec![
                "turn:turn.example:3478?transport=udp".to_owned(),
                "turn:turn.example:3479?transport=udp".to_owned(),
                "turn:turn.example:3480?transport=udp".to_owned(),
                "turn:turn.example:3481?transport=udp".to_owned(),
            ],
            "turn-user",
            "turn-credential",
        ),
        IceServerConfig::with_urls(
            vec![
                "stuns:stun-two.example:5349".to_owned(),
                "stuns:stun-two.example:5350".to_owned(),
                "stuns:stun-two.example:5351".to_owned(),
                "stuns:stun-two.example:5352".to_owned(),
            ],
            "",
            "",
        ),
        IceServerConfig::with_urls(
            vec![
                "stun:stun-three.example:3478".to_owned(),
                "stun:stun-three.example:3479".to_owned(),
                "stun:stun-three.example:3480".to_owned(),
                "stun:stun-three.example:3481".to_owned(),
            ],
            "",
            "",
        ),
    ]);

    let host = timeout(SHORT_TIMEOUT, RelayHost::connect(route, config))
        .await
        .expect("local relay handshake completes within its deadline")
        .expect("bounded valid STUN/TURN configuration is accepted");
    drop(host);
    timeout(SHORT_TIMEOUT, broker)
        .await
        .expect("fake relay observes the host connection ending")
        .expect("fake relay did not panic");
}

#[tokio::test]
async fn relay_host_rejects_malformed_ready_and_request_without_authorizing() {
    let malformed_cases = [
        ("{\"v\":2,\"type\":\"ready\"}", None),
        (
            "{\"v\":1,\"type\":\"ready\"}",
            Some("{\"v\":1,\"type\":\"request\",\"device\":\"5A\"}"),
        ),
        (
            "{\"v\":1,\"type\":\"ready\"}",
            Some(
                "{\"v\":1,\"type\":\"request\",\"device\":\"0000000000000000000000000000000000000000000000000000000000000000\"}",
            ),
        ),
        ("{\"v\":1,\"type\":\"ready\",\"extra\":true}", None),
    ];

    for (ready, request) in malformed_cases {
        let callback_count = Arc::new(AtomicUsize::new(0));
        let callback_count_for_host = Arc::clone(&callback_count);
        let (route, listener) = local_relay().await;
        let expected_path = relay_host_path();
        let broker = tokio::spawn(async move {
            let mut websocket = accept_local_websocket(listener, &expected_path).await;
            send_text(&mut websocket, ready).await;
            if let Some(request) = request {
                send_text(&mut websocket, request).await;
            }
            let _ = timeout(SHORT_TIMEOUT, websocket.next()).await;
        });

        let host = RelayHost::connect(route, InternetIceConfig::new(Vec::new()))
            .await
            .expect("host connects to the fake local relay");
        let accepting = tokio::spawn(async move {
            host.accept(SESSION, move |_| {
                callback_count_for_host.fetch_add(1, Ordering::SeqCst);
                true
            })
            .await
        });
        let result = timeout(SHORT_TIMEOUT, accepting)
            .await
            .expect("malformed control message is rejected promptly")
            .expect("host accept task did not panic");
        assert!(matches!(result, Err(LanError::InvalidSignaling)));
        timeout(SHORT_TIMEOUT, broker)
            .await
            .expect("fake relay task completes")
            .expect("fake relay did not panic");
        assert_eq!(callback_count.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn relay_host_denies_device_before_sending_resume_notice() {
    let callback_device = Arc::new(AtomicUsize::new(0));
    let callback_device_for_host = Arc::clone(&callback_device);
    let (route, listener) = local_relay().await;
    let expected_path = relay_host_path();
    let broker = tokio::spawn(async move {
        let mut websocket = accept_local_websocket(listener, &expected_path).await;
        send_text(&mut websocket, r#"{"v":1,"type":"ready"}"#).await;
        send_text(&mut websocket, &request_message(INITIATOR_DEVICE)).await;
        let next = timeout(SHORT_TIMEOUT, websocket.next()).await;
        assert!(
            !matches!(next, Ok(Some(Ok(Message::Text(_))))),
            "denied devices must not receive a resume or offer/answer message"
        );
    });

    let host = RelayHost::connect(route, InternetIceConfig::new(Vec::new()))
        .await
        .expect("host connects to the fake local relay");
    let result = timeout(
        SHORT_TIMEOUT,
        host.accept(SESSION, move |device| {
            assert_eq!(device, INITIATOR_DEVICE);
            callback_device_for_host.fetch_add(1, Ordering::SeqCst);
            false
        }),
    )
    .await
    .expect("authorization result is returned before timeout");

    assert!(matches!(result, Err(LanError::UnauthorizedDevice)));
    assert_eq!(callback_device.load(Ordering::SeqCst), 1);
    timeout(SHORT_TIMEOUT, broker)
        .await
        .expect("fake relay completes the denied attempt")
        .expect("fake relay did not panic");
}

#[tokio::test]
async fn relay_host_resume_uses_the_supplied_fresh_session() {
    let callback_count = Arc::new(AtomicUsize::new(0));
    let callback_count_for_host = Arc::clone(&callback_count);
    let (route, listener) = local_relay().await;
    let expected_path = relay_host_path();
    let broker = tokio::spawn(async move {
        let mut websocket = accept_local_websocket(listener, &expected_path).await;
        send_text(&mut websocket, r#"{"v":1,"type":"ready"}"#).await;
        send_text(&mut websocket, &request_message(INITIATOR_DEVICE)).await;

        let message = timeout(SHORT_TIMEOUT, websocket.next())
            .await
            .expect("authorized host emits its resume notice")
            .expect("resume notice message exists")
            .expect("resume notice frame is valid");
        let Message::Text(text) = message else {
            panic!("resume notice must be a text frame");
        };
        let resume: serde_json::Value =
            serde_json::from_str(text.as_str()).expect("resume notice is JSON");
        let fields = resume.as_object().expect("resume notice is an object");
        assert_eq!(fields.len(), 3, "resume notice contains no extra fields");
        assert_eq!(fields.get("v").and_then(serde_json::Value::as_u64), Some(1));
        assert_eq!(
            fields.get("type").and_then(serde_json::Value::as_str),
            Some("resume")
        );
        assert_eq!(
            fields.get("session").and_then(serde_json::Value::as_str),
            Some("72".repeat(16).as_str()),
            "resume uses the fresh session supplied by the caller"
        );

        // An invalid SDP stops the attempt before WebRTC creates ICE sockets.
        send_text(&mut websocket, r#"{"v":1,"type":"offer","sdp":""}"#).await;
    });

    let host = RelayHost::connect(route, InternetIceConfig::new(Vec::new()))
        .await
        .expect("host connects to the fake local relay");
    let result = timeout(
        SHORT_TIMEOUT,
        host.accept(SESSION, move |device| {
            callback_count_for_host.fetch_add(1, Ordering::SeqCst);
            device == INITIATOR_DEVICE
        }),
    )
    .await
    .expect("malformed offer ends the accepted attempt promptly");
    assert!(matches!(result, Err(LanError::InvalidSignaling)));
    assert_eq!(callback_count.load(Ordering::SeqCst), 1);
    timeout(SHORT_TIMEOUT, broker)
        .await
        .expect("fake relay completes after the malformed offer")
        .expect("fake relay did not panic");
}

async fn local_relay() -> (RelayRoute, TcpListener) {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind fake relay on an ephemeral loopback port");
    let address = listener.local_addr().expect("read fake relay address");
    let route = RelayRoute::new(&format!("ws://{address}"), ROOM)
        .expect("debug tests permit a loopback-only ws relay");
    (route, listener)
}

async fn accept_local_websocket(
    listener: TcpListener,
    expected_path: &str,
) -> WebSocketStream<TcpStream> {
    let (stream, _) = timeout(SHORT_TIMEOUT, listener.accept())
        .await
        .expect("host connects to the fake relay before the deadline")
        .expect("fake relay accepts one local connection");
    let expected_path = expected_path.to_owned();
    timeout(
        SHORT_TIMEOUT,
        accept_hdr_async(stream, move |request: &Request, response: Response| {
            assert_eq!(request.uri().path(), expected_path);
            Ok(response)
        }),
    )
    .await
    .expect("WebSocket handshake finishes before the deadline")
    .expect("host sends a valid WebSocket upgrade request")
}

async fn send_text(websocket: &mut WebSocketStream<TcpStream>, text: &str) {
    websocket
        .send(Message::Text(text.to_owned().into()))
        .await
        .expect("fake relay sends a local signaling message");
}

fn request_message(device_id: [u8; 32]) -> String {
    let encoded_id = device_id
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("");
    format!(r#"{{"v":1,"type":"request","device":"{encoded_id}"}}"#)
}

fn relay_host_path() -> String {
    format!("/kp/relay/v1/{}/host", "41".repeat(32))
}
