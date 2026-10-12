#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::STANDARD};
use crypto_box::{
    PublicKey, SalsaBox, SecretKey,
    aead::{Aead, OsRng},
};
use serde_json::{Value, json};

use super::{Session, decide_pairing, pending_pairing, set_locked, vault};

pub(super) static TEST_LOCK: Mutex<()> = Mutex::new(());

struct ResetVault;

impl Drop for ResetVault {
    fn drop(&mut self) {
        set_locked(true, true);
    }
}

struct BrowserPeer {
    cipher: SalsaBox,
    client_id: String,
    public_key: String,
    next_nonce: u8,
    request_id: u32,
}

impl BrowserPeer {
    fn connect(session: &mut Session) -> Self {
        let client_secret = SecretKey::generate(&mut OsRng);
        let public_key = STANDARD.encode(client_secret.public_key().as_bytes());
        let client_id = STANDARD.encode([0x31_u8; 24]);
        let nonce = [0_u8; 24];

        let response = session.handle(json!({
            "action": "change-public-keys",
            "requestID": "handshake",
            "clientID": client_id,
            "publicKey": public_key,
            "nonce": STANDARD.encode(nonce),
        }));
        assert_eq!(response["success"], "true");
        assert_eq!(response["version"], session.version);

        let host_public: [u8; 32] = STANDARD
            .decode(response["publicKey"].as_str().expect("host public key"))
            .expect("base64 host public key")
            .try_into()
            .expect("32-byte host public key");
        let cipher = SalsaBox::new(&PublicKey::from(host_public), &client_secret);

        Self {
            cipher,
            client_id,
            public_key,
            next_nonce: 10,
            request_id: 0,
        }
    }

    fn encrypted_request(&mut self, action: &str, data: Value) -> Value {
        let nonce = [self.next_nonce; 24];
        self.next_nonce = self.next_nonce.wrapping_add(1);
        self.request_id += 1;
        let mut data = data;
        data["action"] = json!(action);
        let plaintext = serde_json::to_vec(&data).expect("serialize browser request");
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
            .expect("base64 response nonce")
            .try_into()
            .expect("24-byte response nonce");
        let ciphertext = STANDARD
            .decode(response["message"].as_str().expect("encrypted response"))
            .expect("base64 encrypted response");
        let plaintext = self
            .cipher
            .decrypt((&nonce).into(), ciphertext.as_slice())
            .expect("decrypt browser response");
        serde_json::from_slice(&plaintext).expect("parse browser response")
    }
}

fn await_pending_pairing() -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let token = pending_pairing();
        if !token.is_empty() {
            return token;
        }
        assert!(
            Instant::now() < deadline,
            "association request did not become pending"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn new_session() -> Session {
    Session {
        version: "test-version".to_owned(),
        ..Session::default()
    }
}

fn assert_low_order_exchange_key_is_rejected(public_key: [u8; 32]) {
    let mut session = new_session();
    let response = session.handle(json!({
        "action": "change-public-keys",
        "requestID": "low-order-key",
        "clientID": STANDARD.encode([0x41_u8; 24]),
        "publicKey": STANDARD.encode(public_key),
        "nonce": STANDARD.encode([0_u8; 24]),
    }));

    assert_eq!(response["errorCode"], 9);
    assert_eq!(response["error"], "Invalid public key");
}

#[test]
fn encrypted_association_requires_confirmation_and_is_unavailable_when_locked() {
    let _test_guard = TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    set_locked(false, true);

    let id_key = STANDARD.encode([0x52_u8; 32]);
    let mut session = new_session();
    let mut peer = BrowserPeer::connect(&mut session);
    let client_public_key = peer.public_key.clone();
    let associate_request = peer.encrypted_request(
        "associate",
        json!({"key": client_public_key, "idKey": id_key}),
    );

    let association = thread::spawn(move || {
        let response = session.handle(associate_request);
        (session, response)
    });

    let approval_token = await_pending_pairing();
    {
        let state = vault().lock().expect("vault lock");
        assert!(state.associations.is_empty());
        assert_eq!(
            state.pending.as_ref().map(|pending| pending.approved),
            Some(None)
        );
    }
    decide_pairing(&approval_token, true);

    let (mut session, encrypted_response) = association.join().expect("association thread");
    let response = peer.decrypt_response(&encrypted_response);
    assert_eq!(response["success"], "true");
    let expected_hash = response["hash"].as_str().expect("vault hash").to_owned();
    let association_id = response["id"].as_str().expect("association id").to_owned();
    assert_eq!(
        vault()
            .lock()
            .expect("vault lock")
            .associations
            .get(&association_id),
        Some(&id_key),
    );

    let test_associate_request = peer.encrypted_request(
        "test-associate",
        json!({"id": association_id.clone(), "key": id_key.clone()}),
    );
    let response = peer.decrypt_response(&session.handle(test_associate_request.clone()));
    assert_eq!(response["success"], "true");
    assert_eq!(response["hash"], expected_hash);

    let replay_response = session.handle(test_associate_request);
    assert_eq!(replay_response["errorCode"], 12);
    assert_eq!(
        replay_response["error"],
        "Nonce already used or session exhausted"
    );

    let missing_key_request =
        peer.encrypted_request("test-associate", json!({"id": "unknown-association"}));
    let response = peer.decrypt_response(&session.handle(missing_key_request));
    assert_eq!(response["errorCode"], 8);
    assert_eq!(response["error"], "Association failed");

    let null_key_request = peer.encrypted_request(
        "test-associate",
        json!({"id": "unknown-association", "key": null}),
    );
    let response = peer.decrypt_response(&session.handle(null_key_request));
    assert_eq!(response["errorCode"], 8);
    assert_eq!(response["error"], "Association failed");

    set_locked(true, false);
    let locked_request = peer.encrypted_request(
        "test-associate",
        json!({"id": association_id, "key": id_key}),
    );
    let response = peer.decrypt_response(&session.handle(locked_request));
    assert_eq!(response["errorCode"], 1);
    assert_eq!(response["error"], "Database not opened");

    assert_low_order_exchange_key_is_rejected([0; 32]);
    let mut order_two_public_key = [0; 32];
    order_two_public_key[0] = 1;
    assert_low_order_exchange_key_is_rejected(order_two_public_key);

    set_locked(false, true);
    let mut rejected_session = new_session();
    let mut rejected_peer = BrowserPeer::connect(&mut rejected_session);
    let rejected_public_key = rejected_peer.public_key.clone();
    let rejected_request = rejected_peer.encrypted_request(
        "associate",
        json!({
            "key": rejected_public_key,
            "idKey": STANDARD.encode([0x63_u8; 32]),
        }),
    );
    let rejected_association = thread::spawn(move || {
        let response = rejected_session.handle(rejected_request);
        (rejected_session, response)
    });
    let rejection_token = await_pending_pairing();
    decide_pairing(&rejection_token, false);
    let (_rejected_session, encrypted_response) = rejected_association
        .join()
        .expect("rejected association thread");
    let response = rejected_peer.decrypt_response(&encrypted_response);
    assert_eq!(response["errorCode"], 8);
    assert_eq!(response["error"], "Association rejected");

    let mut locked_session = new_session();
    let mut locked_peer = BrowserPeer::connect(&mut locked_session);
    let locked_public_key = locked_peer.public_key.clone();
    let locked_association_request = locked_peer.encrypted_request(
        "associate",
        json!({
            "key": locked_public_key,
            "idKey": STANDARD.encode([0x74_u8; 32]),
        }),
    );
    let locked_association = thread::spawn(move || {
        let response = locked_session.handle(locked_association_request);
        (locked_session, response)
    });
    let _cancellation_token = await_pending_pairing();
    set_locked(true, false);
    assert!(pending_pairing().is_empty());
    let (_locked_session, encrypted_response) = locked_association
        .join()
        .expect("lock-cancelled association thread");
    let response = locked_peer.decrypt_response(&encrypted_response);
    assert_eq!(response["errorCode"], 1);
    assert_eq!(response["error"], "Database not opened");
}
