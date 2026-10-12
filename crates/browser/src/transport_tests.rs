#![cfg(unix)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use crypto_box::{
    PublicKey, SalsaBox, SecretKey,
    aead::{Aead, OsRng},
};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{
    Candidate, CredentialProvider, Login, access, decide_access, serve_connection, set_locked,
    vault,
};

const ASSOCIATION_ID: &str = "transport-test-association";
const ASSOCIATION_KEY: &str = "transport-test-key";
const ENTRY_ID: &str = "transport-test-login";
const IO_TIMEOUT: Duration = Duration::from_secs(3);

struct ResetVault;

impl Drop for ResetVault {
    fn drop(&mut self) {
        set_locked(true, true);
    }
}

struct Connection {
    peer: Option<UnixStream>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Connection {
    fn start() -> Self {
        let (mut server_stream, peer) = UnixStream::pair().expect("create transport socket pair");
        server_stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .expect("set server write timeout");
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server = thread::spawn(move || {
            serve_connection(
                &mut server_stream,
                "transport-test-version".to_owned(),
                &server_stop,
            )
        });
        Self {
            peer: Some(peer),
            stop,
            server: Some(server),
        }
    }

    fn peer(&mut self) -> &mut UnixStream {
        self.peer.as_mut().expect("transport peer is open")
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        drop(self.peer.take());
        if let Some(server) = self.server.take() {
            server
                .join()
                .expect("transport server exits after peer closes");
        }
    }
}

struct TestProvider {
    login_calls: AtomicUsize,
}

impl TestProvider {
    fn new() -> Self {
        Self {
            login_calls: AtomicUsize::new(0),
        }
    }
}

impl CredentialProvider for TestProvider {
    fn candidates(&self) -> Result<Vec<Candidate>> {
        Ok(vec![Candidate {
            id: ENTRY_ID.to_owned(),
            name: "Transport test login".to_owned(),
            revision: 7,
        }])
    }

    fn login(&self, _candidate: &Candidate, still_authorized: &dyn Fn() -> bool) -> Result<Login> {
        self.login_calls.fetch_add(1, Ordering::Relaxed);
        anyhow::ensure!(
            still_authorized(),
            "transport test login lost authorization"
        );
        Ok(Login {
            username: Zeroizing::new("transport-user".to_owned()),
            password: Zeroizing::new("transport-password".to_owned()),
            urls: vec![Zeroizing::new(
                "https://login.transport.example.test/saved".to_owned(),
            )],
        })
    }
}

fn install_unlocked_vault(provider: Arc<TestProvider>) {
    set_locked(false, true);
    let mut state = vault().lock().expect("lock test vault state");
    state.hash = "transport-test-vault-hash".to_owned();
    state.associations = BTreeMap::from([(ASSOCIATION_ID.to_owned(), ASSOCIATION_KEY.to_owned())]);
    state.provider = Some(provider);
}

fn write_frame(stream: &mut UnixStream, payload: &[u8]) {
    stream
        .write_all(&(payload.len() as u32).to_ne_bytes())
        .expect("write transport frame length");
    stream
        .write_all(payload)
        .expect("write transport frame payload");
}

fn read_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_ne_bytes(size) as usize;
    let mut payload = vec![0; size];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}

fn read_json_frame(stream: &mut UnixStream) -> Value {
    serde_json::from_slice(&read_frame(stream).expect("read transport frame"))
        .expect("parse transport JSON")
}

fn wait_for_pending_access() -> Value {
    let deadline = Instant::now() + IO_TIMEOUT;
    loop {
        let pending = access::pending_access();
        if !pending.is_empty() {
            return serde_json::from_str(&pending).expect("parse pending access JSON");
        }
        assert!(
            Instant::now() < deadline,
            "transport get-logins request did not become pending"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

struct BrowserPeer {
    client_id: String,
    cipher: SalsaBox,
    next_nonce: u8,
    request_id: u32,
}

impl BrowserPeer {
    fn connect(stream: &mut UnixStream) -> Self {
        let client_secret = SecretKey::generate(&mut OsRng);
        let public_key = STANDARD.encode(client_secret.public_key().as_bytes());
        let client_id = STANDARD.encode([0x39_u8; 24]);
        let request = json!({
            "action": "change-public-keys",
            "requestID": "transport-handshake",
            "clientID": client_id,
            "publicKey": public_key,
            "nonce": STANDARD.encode([0_u8; 24]),
        });
        write_frame(
            stream,
            &serde_json::to_vec(&request).expect("serialize key exchange request"),
        );

        let response = read_json_frame(stream);
        assert_eq!(response["action"], "change-public-keys");
        assert_eq!(response["success"], "true");
        assert_eq!(response["version"], "transport-test-version");
        let host_public: [u8; 32] = STANDARD
            .decode(response["publicKey"].as_str().expect("host public key"))
            .expect("decode host public key")
            .try_into()
            .expect("host public key length");

        Self {
            client_id,
            cipher: SalsaBox::new(&PublicKey::from(host_public), &client_secret),
            next_nonce: 1,
            request_id: 0,
        }
    }

    fn encrypted_request(&mut self, action: &str, mut data: Value) -> Value {
        let nonce = [self.next_nonce; 24];
        self.next_nonce = self.next_nonce.wrapping_add(1);
        self.request_id += 1;
        data["action"] = json!(action);
        let plaintext = serde_json::to_vec(&data).expect("serialize encrypted request");
        let ciphertext = self
            .cipher
            .encrypt((&nonce).into(), plaintext.as_slice())
            .expect("encrypt browser request");
        json!({
            "action": action,
            "requestID": self.request_id.to_string(),
            "clientID": self.client_id,
            "nonce": STANDARD.encode(nonce),
            "message": STANDARD.encode(ciphertext),
        })
    }

    fn decrypt_response(&self, response: &Value) -> Value {
        let nonce: [u8; 24] = STANDARD
            .decode(response["nonce"].as_str().expect("response nonce"))
            .expect("decode response nonce")
            .try_into()
            .expect("response nonce length");
        let ciphertext = STANDARD
            .decode(response["message"].as_str().expect("encrypted response"))
            .expect("decode encrypted response");
        let plaintext = self
            .cipher
            .decrypt((&nonce).into(), ciphertext.as_slice())
            .expect("decrypt browser response");
        serde_json::from_slice(&plaintext).expect("parse decrypted response")
    }
}

fn get_logins_request(peer: &mut BrowserPeer) -> Value {
    peer.encrypted_request(
        "get-logins",
        json!({
            "keys": [{"id": ASSOCIATION_ID, "key": ASSOCIATION_KEY}],
            "url": "https://login.transport.example.test/form",
        }),
    )
}

#[test]
fn status_events_report_lock_changes_while_transport_is_idle() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    set_locked(true, true);

    let mut connection = Connection::start();
    connection
        .peer()
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("set idle read timeout");
    assert!(
        matches!(read_frame(connection.peer()), Err(error)
            if matches!(error.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock)),
        "an idle connection should receive no initial status frame"
    );

    connection
        .peer()
        .set_read_timeout(Some(IO_TIMEOUT))
        .expect("set status read timeout");
    set_locked(false, false);
    assert_eq!(
        read_json_frame(connection.peer()),
        json!({"action": "database-unlocked"})
    );
    set_locked(true, false);
    assert_eq!(
        read_json_frame(connection.peer()),
        json!({"action": "database-locked"})
    );
}

#[test]
fn encrypted_get_logins_is_cancelled_when_vault_locks() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    let provider = Arc::new(TestProvider::new());
    install_unlocked_vault(provider.clone());

    let mut connection = Connection::start();
    connection
        .peer()
        .set_read_timeout(Some(IO_TIMEOUT))
        .expect("set cancellation read timeout");
    let mut browser = BrowserPeer::connect(connection.peer());
    write_frame(
        connection.peer(),
        &serde_json::to_vec(&get_logins_request(&mut browser)).expect("serialize request"),
    );

    let pending = wait_for_pending_access();
    assert_eq!(pending["origin"], "https://login.transport.example.test");
    assert_eq!(pending["candidates"][0]["id"], ENTRY_ID);
    assert_eq!(provider.login_calls.load(Ordering::Relaxed), 0);

    set_locked(true, false);
    assert!(access::pending_access().is_empty());

    // Lock status may race with the cancelled request. No encrypted get-logins
    // response or credentials may be written after the epoch changes.
    if let Ok(frame) = read_frame(connection.peer()) {
        let event: Value = serde_json::from_slice(&frame).expect("parse status frame");
        assert_eq!(event, json!({"action": "database-locked"}));
    }
    assert!(
        matches!(read_frame(connection.peer()), Err(error)
            if error.kind() == io::ErrorKind::UnexpectedEof),
        "cancelled get-logins transport should close without a response"
    );
    assert_eq!(provider.login_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn encrypted_approved_get_logins_matches_native_protocol() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    let provider = Arc::new(TestProvider::new());
    install_unlocked_vault(provider.clone());

    let mut connection = Connection::start();
    connection
        .peer()
        .set_read_timeout(Some(IO_TIMEOUT))
        .expect("set response read timeout");
    let mut browser = BrowserPeer::connect(connection.peer());
    write_frame(
        connection.peer(),
        &serde_json::to_vec(&get_logins_request(&mut browser)).expect("serialize request"),
    );

    let pending = wait_for_pending_access();
    decide_access(
        pending["token"].as_str().expect("access token"),
        ENTRY_ID,
        true,
    );

    let envelope = read_json_frame(connection.peer());
    assert_eq!(envelope["action"], "get-logins");
    let response = browser.decrypt_response(&envelope);
    let mut response_nonce = [1_u8; 24];
    response_nonce[0] += 1;
    assert_eq!(
        response,
        json!({
            "success": "true",
            "hash": "transport-test-vault-hash",
            "nonce": STANDARD.encode(response_nonce),
            "version": "transport-test-version",
            "count": "1",
            "entries": [{
                "uuid": ENTRY_ID,
                "name": "Transport test login",
                "login": "transport-user",
                "password": "transport-password",
                "expired": "false",
                "totp": "",
                "stringFields": [],
            }],
        })
    );
    assert_eq!(provider.login_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn denied_get_logins_leaves_encrypted_transport_usable() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    let provider = Arc::new(TestProvider::new());
    install_unlocked_vault(provider.clone());

    let mut connection = Connection::start();
    connection
        .peer()
        .set_read_timeout(Some(IO_TIMEOUT))
        .expect("set response read timeout");
    let mut browser = BrowserPeer::connect(connection.peer());
    write_frame(
        connection.peer(),
        &serde_json::to_vec(&get_logins_request(&mut browser)).expect("serialize request"),
    );

    let pending = wait_for_pending_access();
    decide_access(pending["token"].as_str().expect("access token"), "", false);

    let denied = read_json_frame(connection.peer());
    assert_eq!(
        denied,
        json!({
            "action": "get-logins",
            "requestID": "1",
            "errorCode": 6,
            "error": "Login access denied, expired, or selected entry does not match the site",
        })
    );

    let request = browser.encrypted_request(
        "test-associate",
        json!({"id": ASSOCIATION_ID, "key": ASSOCIATION_KEY}),
    );
    write_frame(
        connection.peer(),
        &serde_json::to_vec(&request).expect("serialize association check"),
    );
    let response = read_json_frame(connection.peer());
    assert_eq!(response["action"], "test-associate");
    let response = browser.decrypt_response(&response);
    assert_eq!(response["success"], "true");
    assert_eq!(response["id"], ASSOCIATION_ID);
    assert_eq!(response["hash"], "transport-test-vault-hash");
    assert_eq!(provider.login_calls.load(Ordering::Relaxed), 0);
}
