#![allow(
    clippy::expect_used,
    reason = "Integration test setup and protocol assertions use expect with local failure context."
)]

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::StreamExt;
use kryptopass_lan::{LanError, ResumeListener};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{WebSocketStream, client_async, tungstenite::Message};

const DESKTOP_DEVICE_ID: [u8; 32] = [0x91; 32];
const INITIATOR_DEVICE_ID: [u8; 32] = [0x5a; 32];
const UNKNOWN_DEVICE_ID: [u8; 32] = [0x2c; 32];
const SHORT_TIMEOUT: Duration = Duration::from_secs(2);

#[tokio::test]
async fn durable_resume_rejects_bad_selectors_and_authorizes_before_notice() {
    let canonical_path = resume_path(INITIATOR_DEVICE_ID);
    let invalid_paths = [
        format!("/kp/lan/v1/resume/{}", "5a".repeat(63)),
        format!("/kp/lan/v1/resume/{}", "5A".repeat(32)),
        format!("/kp/lan/v1/resume/{}", "00".repeat(32)),
        format!("/kp/lan/v1/resume/{}", "gg".repeat(32)),
    ];

    for path in invalid_paths {
        assert_request_rejected(&path, None).await;
    }
    assert_request_rejected(&format!("{canonical_path}?attempt=1"), None).await;
    assert_request_rejected(&resume_path(UNKNOWN_DEVICE_ID), Some(UNKNOWN_DEVICE_ID)).await;
}

#[tokio::test]
async fn durable_resume_notice_is_fresh_and_cancelled_wait_keeps_listener_usable() {
    let listener = Arc::new(
        ResumeListener::bind(DESKTOP_DEVICE_ID)
            .await
            .expect("bind local durable resume listener"),
    );
    assert_ne!(listener.port(), 0);

    let pending_listener = Arc::clone(&listener);
    let pending = tokio::spawn(async move { pending_listener.accept_authorized(|_| true).await });
    tokio::task::yield_now().await;
    pending.abort();
    assert!(matches!(pending.await, Err(error) if error.is_cancelled()));

    let first_session = authorized_resume_notice(Arc::clone(&listener)).await;
    let second_session = authorized_resume_notice(listener).await;
    assert_ne!(first_session, second_session);
}

async fn assert_request_rejected(path: &str, expected_callback_id: Option<[u8; 32]>) {
    let listener = Arc::new(
        ResumeListener::bind(DESKTOP_DEVICE_ID)
            .await
            .expect("bind local durable resume listener"),
    );
    let callback_ids = Arc::new(Mutex::new(Vec::new()));
    let accepted_listener = Arc::clone(&listener);
    let recorded_ids = Arc::clone(&callback_ids);
    let server = tokio::spawn(async move {
        accepted_listener
            .accept_authorized(move |device_id| {
                recorded_ids
                    .lock()
                    .expect("callback ID mutex")
                    .push(device_id);
                false
            })
            .await
    });

    let socket_address = SocketAddr::from(([127, 0, 0, 1], listener.port()));
    let client_result = timeout(SHORT_TIMEOUT, connect_websocket(socket_address, path))
        .await
        .expect("resume endpoint responds within the local test deadline");
    assert!(
        client_result.is_err(),
        "request unexpectedly upgraded: {path}"
    );

    let server_result = timeout(SHORT_TIMEOUT, server)
        .await
        .expect("rejected request releases the listener future")
        .expect("listener task did not panic");
    assert!(matches!(server_result, Err(LanError::InvalidSignaling)));

    let callback_ids = callback_ids.lock().expect("callback ID mutex");
    match expected_callback_id {
        Some(expected_id) => assert_eq!(callback_ids.as_slice(), &[expected_id]),
        None => assert!(
            callback_ids.is_empty(),
            "invalid route reached authorization"
        ),
    }
}

async fn authorized_resume_notice(listener: Arc<ResumeListener>) -> String {
    let callback_ids = Arc::new(Mutex::new(Vec::new()));
    let recorded_ids = Arc::clone(&callback_ids);
    let accepted_listener = Arc::clone(&listener);
    let server = tokio::spawn(async move {
        accepted_listener
            .accept_authorized(move |device_id| {
                recorded_ids
                    .lock()
                    .expect("callback ID mutex")
                    .push(device_id);
                device_id == INITIATOR_DEVICE_ID
            })
            .await
    });

    let address = SocketAddr::from(([127, 0, 0, 1], listener.port()));
    let mut websocket = timeout(
        Duration::from_secs(4),
        connect_websocket(address, &resume_path(INITIATOR_DEVICE_ID)),
    )
    .await
    .expect("authorized resume request reached the listener before deadline")
    .expect("authorized selector upgraded to WebSocket");
    let message = timeout(Duration::from_secs(1), websocket.next())
        .await
        .expect("listener sent its resume notice promptly")
        .expect("resume notice message exists")
        .expect("resume notice frame is valid");
    let Message::Text(text) = message else {
        panic!("resume notice must be a text WebSocket message");
    };
    let notice: serde_json::Value =
        serde_json::from_str(text.as_str()).expect("resume notice is JSON");
    let fields = notice.as_object().expect("resume notice is an object");
    assert_eq!(fields.len(), 3, "resume notice has exactly three fields");
    assert_eq!(fields.get("v").and_then(serde_json::Value::as_u64), Some(1));
    assert_eq!(
        fields.get("type").and_then(serde_json::Value::as_str),
        Some("resume")
    );
    let session = fields
        .get("session")
        .and_then(serde_json::Value::as_str)
        .expect("resume notice has a session ID");
    assert!(is_canonical_nonzero_session(session));
    assert_eq!(
        callback_ids.lock().expect("callback ID mutex").as_slice(),
        &[INITIATOR_DEVICE_ID],
        "authorization callback ran before the resume notice"
    );

    // Closing before SDP confirms the listener sends the notice before it asks
    // for an offer; the next accepted attempt must receive a different session.
    drop(websocket);
    let server_result = timeout(SHORT_TIMEOUT, server)
        .await
        .expect("closing before SDP drains the current attempt")
        .expect("listener task did not panic");
    assert!(
        server_result.is_err(),
        "an offer is required to produce a transport"
    );

    session.to_owned()
}

async fn connect_websocket(
    address: SocketAddr,
    path: &str,
) -> Result<WebSocketStream<TcpStream>, tokio_tungstenite::tungstenite::Error> {
    let stream = TcpStream::connect(address).await?;
    let request = format!("ws://{address}{path}");
    let (websocket, _) = client_async(request, stream).await?;
    Ok(websocket)
}

fn resume_path(device_id: [u8; 32]) -> String {
    let encoded_id = device_id
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("");
    format!("/kp/lan/v1/resume/{encoded_id}")
}

fn is_canonical_nonzero_session(session: &str) -> bool {
    session.len() == 32
        && session
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && session.bytes().any(|byte| byte != b'0')
}
