#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{Candidate, CredentialProvider, Login, access, set_locked, vault};

const ASSOCIATION_ID: &str = "browser-association";
const ASSOCIATION_KEY: &str = "test-association-key";
const ENTRY_ID: &str = "selected-login";

struct ResetVault;

impl Drop for ResetVault {
    fn drop(&mut self) {
        set_locked(true, true);
    }
}

#[derive(Default)]
struct DecryptGate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

impl DecryptGate {
    fn pause(&self) {
        let mut state = self.state.lock().expect("decrypt gate");
        state.0 = true;
        self.changed.notify_all();
        while !state.1 {
            state = self.changed.wait(state).expect("decrypt gate wait");
        }
    }

    fn wait_until_started(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = self.state.lock().expect("decrypt gate");
        while !state.0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "credential read did not start");
            let (next, timeout) = self
                .changed
                .wait_timeout(state, remaining)
                .expect("decrypt gate wait");
            state = next;
            assert!(
                !timeout.timed_out() || state.0,
                "credential read did not start"
            );
        }
    }

    fn release(&self) {
        let mut state = self.state.lock().expect("decrypt gate");
        state.1 = true;
        self.changed.notify_all();
    }
}

struct MockProvider {
    candidates: Vec<Candidate>,
    urls: Vec<String>,
    username_calls: Mutex<Vec<String>>,
    username_gate: Option<Arc<DecryptGate>>,
    username_failure: Option<String>,
    username_absent: bool,
    authorized_after_username: Mutex<Option<bool>>,
    login_calls: Mutex<Vec<String>>,
    candidate_calls: Mutex<usize>,
    decrypt_gate: Option<Arc<DecryptGate>>,
    authorized_after_decrypt: Mutex<Option<bool>>,
}

impl MockProvider {
    fn new(candidates: Vec<Candidate>, urls: &[&str]) -> Self {
        Self {
            candidates,
            urls: urls.iter().map(|url| (*url).to_owned()).collect(),
            username_calls: Mutex::new(Vec::new()),
            username_gate: None,
            username_failure: None,
            username_absent: false,
            authorized_after_username: Mutex::new(None),
            login_calls: Mutex::new(Vec::new()),
            candidate_calls: Mutex::new(0),
            decrypt_gate: None,
            authorized_after_decrypt: Mutex::new(None),
        }
    }

    fn slow(candidates: Vec<Candidate>, urls: &[&str], gate: Arc<DecryptGate>) -> Self {
        let mut provider = Self::new(candidates, urls);
        provider.decrypt_gate = Some(gate);
        provider
    }

    fn slow_username(candidates: Vec<Candidate>, urls: &[&str], gate: Arc<DecryptGate>) -> Self {
        let mut provider = Self::new(candidates, urls);
        provider.username_gate = Some(gate);
        provider
    }

    fn failing_username(candidates: Vec<Candidate>, urls: &[&str], failed_candidate: &str) -> Self {
        let mut provider = Self::new(candidates, urls);
        provider.username_failure = Some(failed_candidate.to_owned());
        provider
    }

    fn username_absent(candidates: Vec<Candidate>, urls: &[&str]) -> Self {
        let mut provider = Self::new(candidates, urls);
        provider.username_absent = true;
        provider
    }

    fn username_calls(&self) -> Vec<String> {
        self.username_calls
            .lock()
            .expect("username call list")
            .clone()
    }

    fn login_calls(&self) -> Vec<String> {
        self.login_calls.lock().expect("login call list").clone()
    }

    fn candidate_calls(&self) -> usize {
        *self.candidate_calls.lock().expect("candidate call count")
    }
}

impl CredentialProvider for MockProvider {
    fn candidates(&self) -> Result<Vec<Candidate>> {
        *self.candidate_calls.lock().expect("candidate call count") += 1;
        Ok(self.candidates.clone())
    }

    fn username(
        &self,
        candidate: &Candidate,
        still_authorized: &dyn Fn() -> bool,
    ) -> Result<Option<Zeroizing<String>>> {
        self.username_calls
            .lock()
            .expect("username call list")
            .push(candidate.id.clone());
        if let Some(gate) = &self.username_gate {
            gate.pause();
            let authorized = still_authorized();
            *self
                .authorized_after_username
                .lock()
                .expect("username authorization result") = Some(authorized);
            ensure!(authorized, "vault changed during username preview");
        }
        ensure!(
            self.username_failure.as_deref() != Some(candidate.id.as_str()),
            "username preview failed"
        );
        if self.username_absent {
            return Ok(None);
        }
        Ok(Some(Zeroizing::new(format!("preview-{}", candidate.id))))
    }

    fn login(&self, candidate: &Candidate, still_authorized: &dyn Fn() -> bool) -> Result<Login> {
        self.login_calls
            .lock()
            .expect("login call list")
            .push(candidate.id.clone());
        if let Some(gate) = &self.decrypt_gate {
            gate.pause();
            let authorized = still_authorized();
            *self
                .authorized_after_decrypt
                .lock()
                .expect("authorization result") = Some(authorized);
            ensure!(authorized, "vault changed during credential read");
        }
        Ok(Login {
            username: Zeroizing::new(format!("user-{}", candidate.id)),
            password: Zeroizing::new(format!("password-{}", candidate.id)),
            urls: self.urls.iter().cloned().map(Zeroizing::new).collect(),
        })
    }
}

fn candidate(id: &str, name: &str, revision: u64) -> Candidate {
    Candidate {
        id: id.to_owned(),
        name: name.to_owned(),
        revision,
    }
}

fn install_provider(provider: Arc<dyn CredentialProvider>) {
    set_locked(true, true);
    let mut state = vault().lock().expect("vault state");
    state.unlocked = true;
    state.hash = "access-test-vault".to_owned();
    state.associations = BTreeMap::from([(ASSOCIATION_ID.to_owned(), ASSOCIATION_KEY.to_owned())]);
    state.provider = Some(provider);
}

fn request() -> Value {
    json!({"action": "get-logins", "requestID": "access-test"})
}

fn data(url: &str) -> Value {
    json!({
        "keys": [{"id": ASSOCIATION_ID, "key": ASSOCIATION_KEY}],
        "url": url,
    })
}

fn start_access(data: Value) -> thread::JoinHandle<Value> {
    thread::spawn(move || access::get_logins(&request(), &data))
}

fn wait_for_access() -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let pending = super::pending_access();
        if !pending.is_empty() {
            let pending: Value = serde_json::from_str(&pending).expect("pending access JSON");
            return pending;
        }
        assert!(
            Instant::now() < deadline,
            "access request did not become pending"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_username_state(token: &str, entry_id: &str, expected: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let pending = super::pending_access();
        if !pending.is_empty() {
            let pending: Value = serde_json::from_str(&pending).expect("pending access JSON");
            let matching_candidate = pending["candidates"].as_array().and_then(|candidates| {
                candidates
                    .iter()
                    .find(|candidate| candidate["id"] == entry_id)
            });
            if pending["token"] == token
                && matching_candidate
                    .is_some_and(|candidate| candidate["usernameState"] == expected)
            {
                return pending;
            }
        }
        assert!(
            Instant::now() < deadline,
            "candidate username did not reach state {expected}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn request_username(pending: &Value, entry_id: &str) {
    let token = pending["token"].as_str().expect("access token");
    access::request_username(token, entry_id);
}

fn request_username_and_wait(pending: &Value, entry_id: &str) -> Value {
    request_username(pending, entry_id);
    wait_for_username_state(
        pending["token"].as_str().expect("access token"),
        entry_id,
        "ready",
    )
}

fn assert_denied(response: &Value) {
    assert_eq!(response["errorCode"], 6, "response: {response}");
}

fn valid_data_with_keys(url: &str, keys: Value) -> Value {
    json!({"keys": keys, "url": url})
}

#[test]
fn get_logins_releases_only_an_approved_associated_login_for_the_exact_origin() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    let candidates = vec![
        candidate("other-login", "A different login", 3),
        candidate(ENTRY_ID, "Work account", 7),
    ];

    // Missing, unknown, or incorrect association keys fail before listing candidates.
    let provider = Arc::new(MockProvider::new(
        candidates.clone(),
        &["https://login.example.test/saved"],
    ));
    install_provider(provider.clone());
    for keys in [
        json!([]),
        json!([{"id": "unknown-association", "key": ASSOCIATION_KEY}]),
        json!([{"id": ASSOCIATION_ID, "key": "wrong-key"}]),
    ] {
        let response = access::get_logins(
            &request(),
            &valid_data_with_keys("https://login.example.test/form", keys),
        );
        assert_denied(&response);
        assert!(super::pending_access().is_empty());
    }
    assert_eq!(provider.candidate_calls(), 0);
    assert!(provider.login_calls().is_empty());

    // A cross-origin submit URL is rejected before presenting a choice.
    let response = access::get_logins(
        &request(),
        &json!({
            "keys": [{"id": ASSOCIATION_ID, "key": ASSOCIATION_KEY}],
            "url": "https://login.example.test/form",
            "submitUrl": "https://attacker.example.test/submit",
        }),
    );
    assert_denied(&response);
    assert!(super::pending_access().is_empty());
    assert_eq!(provider.candidate_calls(), 0);
    assert!(provider.login_calls().is_empty());

    // A pending request exposes metadata immediately without reading any usernames.
    let pending_request = start_access(data("https://login.example.test/form"));
    let pending = wait_for_access();
    assert_eq!(pending["previewReady"], true);
    assert_eq!(pending["origin"], "https://login.example.test");
    assert_eq!(
        pending["candidates"][0],
        json!({
            "id": "other-login",
            "name": "A different login",
            "revision": 3,
            "username": "",
            "usernameState": "unloaded"
        })
    );
    assert_eq!(
        pending["candidates"][1],
        json!({
            "id": ENTRY_ID,
            "name": "Work account",
            "revision": 7,
            "username": "",
            "usernameState": "unloaded"
        })
    );
    assert!(provider.username_calls().is_empty());
    assert!(provider.login_calls().is_empty());
    // Requesting a single username reveals only that value and never reads a login.
    let pending = request_username_and_wait(&pending, ENTRY_ID);
    assert_eq!(pending["candidates"][0]["usernameState"], "unloaded");
    assert_eq!(pending["candidates"][0]["username"], "");
    assert_eq!(pending["candidates"][1]["usernameState"], "ready");
    assert_eq!(
        pending["candidates"][1]["username"],
        format!("preview-{ENTRY_ID}")
    );
    assert!(!pending.to_string().contains("password"));
    assert_eq!(provider.username_calls(), vec![ENTRY_ID]);
    assert!(provider.login_calls().is_empty());
    super::decide_access(
        pending["token"].as_str().expect("access token"),
        "not-listed",
        true,
    );
    assert!(provider.login_calls().is_empty());
    assert!(
        !super::pending_access().is_empty(),
        "an unlisted choice must not approve access"
    );
    super::decide_access(pending["token"].as_str().expect("access token"), "", false);
    assert_denied(&pending_request.join().expect("cancelled access request"));
    assert!(provider.login_calls().is_empty());

    // The same HTTP(S) origin permits a saved URL with a different path, and releases only the chosen entry.
    let successful = start_access(data("https://login.example.test/form/submit"));
    let pending = request_username_and_wait(&wait_for_access(), ENTRY_ID);
    super::decide_access(
        pending["token"].as_str().expect("access token"),
        ENTRY_ID,
        true,
    );
    let response = successful.join().expect("approved access request");
    assert_eq!(response["success"], "true");
    assert_eq!(response["count"], "1");
    assert_eq!(response["entries"].as_array().map(Vec::len), Some(1));
    let entry = &response["entries"][0];
    assert_eq!(entry["uuid"], ENTRY_ID);
    assert_eq!(entry["name"], "Work account");
    assert_eq!(entry["login"], format!("user-{ENTRY_ID}"));
    assert_eq!(entry["password"], format!("password-{ENTRY_ID}"));
    assert_eq!(entry["expired"], "false");
    assert_eq!(entry["totp"], "");
    assert_eq!(entry["stringFields"], json!([]));
    assert_eq!(provider.login_calls(), vec![ENTRY_ID]);

    // Scheme, hostname suffix, and port are all part of the origin comparison.
    for saved_url in [
        "http://login.example.test:8443/saved",
        "https://login.example.test.attacker.test:8443/saved",
        "https://login.example.test/saved",
    ] {
        let provider = Arc::new(MockProvider::new(
            vec![candidate(ENTRY_ID, "Work account", 7)],
            &[saved_url],
        ));
        install_provider(provider.clone());
        let pending_request = start_access(data("https://login.example.test:8443/form"));
        let pending = request_username_and_wait(&wait_for_access(), ENTRY_ID);
        super::decide_access(
            pending["token"].as_str().expect("access token"),
            ENTRY_ID,
            true,
        );
        assert_denied(&pending_request.join().expect("origin-mismatched request"));
        assert_eq!(provider.login_calls(), vec![ENTRY_ID]);
    }

    // Locking while a choice is pending cancels the request before any credential read.
    let provider = Arc::new(MockProvider::new(
        vec![candidate(ENTRY_ID, "Work account", 7)],
        &["https://login.example.test/saved"],
    ));
    install_provider(provider.clone());
    let locked_request = start_access(data("https://login.example.test/form"));
    let _pending = request_username_and_wait(&wait_for_access(), ENTRY_ID);
    set_locked(true, false);
    assert!(super::pending_access().is_empty());
    assert_denied(&locked_request.join().expect("locked access request"));
    assert!(provider.login_calls().is_empty());

    // An unlock after a lock still invalidates the pending request's captured epoch.
    let provider = Arc::new(MockProvider::new(
        vec![candidate(ENTRY_ID, "Work account", 7)],
        &["https://login.example.test/saved"],
    ));
    install_provider(provider.clone());
    let stale_request = start_access(data("https://login.example.test/form"));
    let _pending = request_username_and_wait(&wait_for_access(), ENTRY_ID);
    set_locked(true, false);
    set_locked(false, false);
    assert_denied(&stale_request.join().expect("stale access request"));
    assert!(provider.login_calls().is_empty());

    // Locking during a slow credential read invalidates the callback before credentials can return.
    let gate = Arc::new(DecryptGate::default());
    let provider = Arc::new(MockProvider::slow(
        vec![candidate(ENTRY_ID, "Work account", 7)],
        &["https://login.example.test/saved"],
        gate.clone(),
    ));
    install_provider(provider.clone());
    let slow_request = start_access(data("https://login.example.test/form"));
    let pending = request_username_and_wait(&wait_for_access(), ENTRY_ID);
    super::decide_access(
        pending["token"].as_str().expect("access token"),
        ENTRY_ID,
        true,
    );
    gate.wait_until_started();
    set_locked(true, false);
    gate.release();
    assert_denied(&slow_request.join().expect("locked slow access request"));
    assert_eq!(provider.login_calls(), vec![ENTRY_ID]);
    assert_eq!(
        *provider
            .authorized_after_decrypt
            .lock()
            .expect("authorization result"),
        Some(false)
    );
}

#[test]
fn username_preview_is_lazy_and_exposes_only_the_requested_username() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    let provider = Arc::new(MockProvider::new(
        vec![
            candidate("other-login", "A different login", 3),
            candidate(ENTRY_ID, "Work account", 7),
        ],
        &["https://login.example.test/saved"],
    ));
    install_provider(provider.clone());

    let pending_request = start_access(data("https://login.example.test/form"));
    let pending = wait_for_access();
    assert_eq!(pending["previewReady"], true);
    assert_eq!(pending["candidates"].as_array().map(Vec::len), Some(2));
    assert!(provider.login_calls().is_empty());
    assert!(provider.username_calls().is_empty());

    let pending = request_username_and_wait(&pending, ENTRY_ID);
    assert_eq!(pending["candidates"][0]["usernameState"], "unloaded");
    assert_eq!(pending["candidates"][1]["usernameState"], "ready");
    assert_eq!(
        pending["candidates"][1]["username"],
        format!("preview-{ENTRY_ID}")
    );
    assert!(!pending.to_string().contains("password"));
    assert_eq!(provider.username_calls(), vec![ENTRY_ID]);

    super::decide_access(pending["token"].as_str().expect("access token"), "", false);
    assert_denied(&pending_request.join().expect("cancelled preview request"));
    assert!(provider.login_calls().is_empty());
}

#[test]
fn username_preview_failure_sets_error_and_cannot_be_approved() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    let provider = Arc::new(MockProvider::failing_username(
        vec![
            candidate("bad-preview", "A unavailable username", 2),
            candidate("available", "B available username", 4),
        ],
        &["https://login.example.test/saved"],
        "bad-preview",
    ));
    install_provider(provider.clone());

    let pending_request = start_access(data("https://login.example.test/form"));
    let pending = wait_for_access();
    assert_eq!(pending["candidates"][0]["usernameState"], "unloaded");
    request_username(&pending, "bad-preview");
    let pending = wait_for_username_state(
        pending["token"].as_str().expect("access token"),
        "bad-preview",
        "error",
    );
    assert_eq!(pending["candidates"][0]["username"], "");
    assert_eq!(provider.username_calls(), vec!["bad-preview"]);
    assert!(provider.login_calls().is_empty());

    super::decide_access(
        pending["token"].as_str().expect("access token"),
        "bad-preview",
        true,
    );
    assert!(!super::pending_access().is_empty());
    assert!(provider.login_calls().is_empty());
    super::decide_access(pending["token"].as_str().expect("access token"), "", false);
    assert_denied(
        &pending_request
            .join()
            .expect("cancelled username error request"),
    );
}

#[test]
fn empty_username_preview_is_ready_and_can_be_approved() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    let provider = Arc::new(MockProvider::username_absent(
        vec![candidate(ENTRY_ID, "Work account", 7)],
        &["https://login.example.test/saved"],
    ));
    install_provider(provider.clone());

    let pending_request = start_access(data("https://login.example.test/form"));
    let pending = request_username_and_wait(&wait_for_access(), ENTRY_ID);
    assert_eq!(pending["candidates"][0]["usernameState"], "ready");
    assert_eq!(pending["candidates"][0]["username"], "");
    assert_eq!(provider.username_calls(), vec![ENTRY_ID]);

    super::decide_access(
        pending["token"].as_str().expect("access token"),
        ENTRY_ID,
        true,
    );
    let response = pending_request
        .join()
        .expect("approved empty username request");
    assert_eq!(response["success"], "true");
    assert_eq!(response["entries"][0]["login"], format!("user-{ENTRY_ID}"));
    assert_eq!(provider.login_calls(), vec![ENTRY_ID]);
}

#[test]
fn cancelling_during_username_read_denies_access_and_discards_preview() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    let gate = Arc::new(DecryptGate::default());
    let provider = Arc::new(MockProvider::slow_username(
        vec![candidate(ENTRY_ID, "Work account", 7)],
        &["https://login.example.test/saved"],
        gate.clone(),
    ));
    install_provider(provider.clone());

    let pending_request = start_access(data("https://login.example.test/form"));
    let pending = wait_for_access();
    request_username(&pending, ENTRY_ID);
    gate.wait_until_started();
    let token = pending["token"].as_str().expect("access token").to_owned();
    let pending = wait_for_username_state(&token, ENTRY_ID, "loading");
    assert_eq!(pending["candidates"][0]["username"], "");
    assert!(provider.login_calls().is_empty());

    super::decide_access(&token, "", false);
    gate.release();
    assert_denied(&pending_request.join().expect("cancelled username read"));
    assert!(super::pending_access().is_empty());
    assert!(provider.login_calls().is_empty());
    assert_eq!(
        *provider
            .authorized_after_username
            .lock()
            .expect("username authorization result"),
        Some(false)
    );
}

#[test]
fn locking_during_username_read_invalidates_and_discards_preview() {
    let _test_guard = super::tests::TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = ResetVault;
    let gate = Arc::new(DecryptGate::default());
    let provider = Arc::new(MockProvider::slow_username(
        vec![candidate(ENTRY_ID, "Work account", 7)],
        &["https://login.example.test/saved"],
        gate.clone(),
    ));
    install_provider(provider.clone());

    let pending_request = start_access(data("https://login.example.test/form"));
    let pending = wait_for_access();
    request_username(&pending, ENTRY_ID);
    gate.wait_until_started();
    let token = pending["token"].as_str().expect("access token").to_owned();
    let pending = wait_for_username_state(&token, ENTRY_ID, "loading");
    assert_eq!(pending["candidates"][0]["username"], "");

    set_locked(true, false);
    assert!(super::pending_access().is_empty());
    gate.release();
    assert_denied(&pending_request.join().expect("locked username read"));
    assert!(super::pending_access().is_empty());
    assert!(provider.login_calls().is_empty());
    assert_eq!(
        *provider
            .authorized_after_username
            .lock()
            .expect("username authorization result"),
        Some(false)
    );
}
