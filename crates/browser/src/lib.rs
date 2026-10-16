//! Encrypted browser transport with durable association and per-request authorization.

mod access;
mod persistence;
pub use access::{
    Candidate, CredentialProvider, Login, configure_vault, decide_access, pending_access,
    request_username, site_origin,
};

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::STANDARD};
use crypto_box::{
    PublicKey, SalsaBox, SecretKey,
    aead::{Aead, OsRng},
};
use kryptopass_secure_memory::SensitiveBytes;
use serde_json::{Value, json};
use uuid::Uuid;
use zeroize::Zeroizing;

const MAX_MESSAGE: usize = 1024 * 1024;
const PAIRING_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
struct VaultState {
    unlocked: bool,
    epoch: u64,
    hash: String,
    associations: BTreeMap<String, String>,
    pending: Option<Pairing>,
    access: Option<access::Access>,
    provider: Option<Arc<dyn CredentialProvider>>,
    store_path: Option<std::path::PathBuf>,
}

struct Pairing {
    token: String,
    expires: Instant,
    approved: Option<bool>,
}

static VAULT: OnceLock<Mutex<VaultState>> = OnceLock::new();

fn vault() -> &'static Mutex<VaultState> {
    VAULT.get_or_init(|| Mutex::new(VaultState::default()))
}

pub fn set_locked(locked: bool, changed_vault: bool) {
    if let Ok(mut state) = vault().lock() {
        state.unlocked = !locked;
        state.epoch = state.epoch.wrapping_add(1);
        state.pending = None;
        state.access = None;
        if changed_vault || state.hash.is_empty() {
            state.hash = Uuid::new_v4().to_string();
            state.associations.clear();
            state.provider = None;
            state.store_path = None;
        }
    }
}

pub fn pending_pairing() -> String {
    vault()
        .lock()
        .ok()
        .and_then(|state| {
            state
                .pending
                .as_ref()
                .filter(|pending| pending.expires > Instant::now() && pending.approved.is_none())
                .map(|pending| pending.token.clone())
        })
        .unwrap_or_default()
}

pub fn decide_pairing(token: &str, approve: bool) {
    if let Ok(mut state) = vault().lock() {
        let unlocked = state.unlocked;
        if let Some(pending) = state.pending.as_mut() {
            if pending.token == token && pending.expires > Instant::now() && unlocked {
                pending.approved = Some(approve);
            }
        }
    }
}

#[derive(Default)]
struct Session {
    client_id: String,
    public_key: String,
    cipher: Option<SalsaBox>,
    version: String,
    used_nonces: BTreeSet<[u8; 24]>,
}

fn wipe_json(value: &mut Value) {
    use zeroize::Zeroize;
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(values) => values.iter_mut().for_each(wipe_json),
        Value::Object(values) => {
            for (mut key, mut value) in std::mem::take(values) {
                key.zeroize();
                wipe_json(&mut value);
            }
        }
        _ => {}
    }
}

fn decode<const N: usize>(value: &Value) -> Option<[u8; N]> {
    STANDARD.decode(value.as_str()?).ok()?.try_into().ok()
}

fn increment_nonce(mut nonce: [u8; 24]) -> Option<[u8; 24]> {
    for byte in &mut nonce {
        let (next, carry) = byte.overflowing_add(1);
        *byte = next;
        if !carry {
            return Some(nonce);
        }
    }
    None
}

fn protocol_error(request: &Value, code: u8, message: &str) -> Value {
    json!({"action": request["action"], "requestID": request["requestID"],
        "errorCode": code, "error": message})
}

impl Session {
    fn handle(&mut self, request: Value) -> Value {
        let Some(nonce) = decode::<24>(&request["nonce"]).and_then(increment_nonce) else {
            return protocol_error(&request, 12, "Invalid nonce");
        };
        if request["action"] == "change-public-keys" {
            let Some(public) = decode::<32>(&request["publicKey"]) else {
                return protocol_error(&request, 9, "Invalid public key");
            };
            if decode::<24>(&request["clientID"]).is_none() {
                return protocol_error(&request, 9, "Invalid client identifier");
            }
            let secret = SecretKey::generate(&mut OsRng);
            let shared = Zeroizing::new(
                curve25519_dalek::montgomery::MontgomeryPoint(public)
                    .mul_clamped(secret.to_bytes())
                    .to_bytes(),
            );
            if *shared == [0; 32] {
                return protocol_error(&request, 9, "Invalid public key");
            }
            let host_public = STANDARD.encode(secret.public_key().as_bytes());
            self.used_nonces.clear();
            self.cipher = Some(SalsaBox::new(&PublicKey::from(public), &secret));
            self.client_id = request["clientID"].as_str().unwrap_or_default().to_owned();
            self.public_key = request["publicKey"].as_str().unwrap_or_default().to_owned();
            return json!({"action": "change-public-keys", "success": "true",
                "version": self.version,
                "publicKey": host_public, "nonce": STANDARD.encode(nonce)});
        }
        let Some(cipher) = self.cipher.as_ref() else {
            return protocol_error(&request, 3, "Key exchange required");
        };
        if request["clientID"].as_str() != Some(&self.client_id) {
            return protocol_error(&request, 10, "Unknown client");
        }
        let Some(original_nonce) = decode::<24>(&request["nonce"]) else {
            return protocol_error(&request, 12, "Invalid nonce");
        };
        if self.used_nonces.contains(&original_nonce)
            || self.used_nonces.contains(&nonce)
            || self.used_nonces.len() >= 8192
        {
            return protocol_error(&request, 12, "Nonce already used or session exhausted");
        }
        let payload = request["message"]
            .as_str()
            .and_then(|message| STANDARD.decode(message).ok());
        let plaintext = payload
            .and_then(|payload| {
                cipher
                    .decrypt((&original_nonce).into(), payload.as_slice())
                    .ok()
            })
            .map(SensitiveBytes::from);
        let Some(plaintext) = plaintext else {
            return protocol_error(&request, 4, "Cannot decrypt message");
        };
        let Ok(mut data) = serde_json::from_slice::<Value>(&plaintext) else {
            return protocol_error(&request, 4, "Invalid message");
        };
        if data["action"] != request["action"] {
            wipe_json(&mut data);
            return protocol_error(&request, 12, "Action mismatch");
        }
        self.used_nonces.insert(original_nonce);
        self.used_nonces.insert(nonce);
        let mut encoded = SensitiveBytes::new();
        let encoding_result = if request["action"] == "get-logins" {
            let response = access::get_logins(&request, &data);
            wipe_json(&mut data);
            let response = match response {
                Ok(response) => response,
                // Credential denials must stay in the outer error envelope.
                Err(error) => return error,
            };
            #[derive(serde::Serialize)]
            struct Envelope<'a> {
                #[serde(flatten)]
                response: &'a access::LoginResponse,
                nonce: String,
                version: &'a str,
            }
            serde_json::to_writer(
                encoded.bounded_writer(MAX_MESSAGE),
                &Envelope {
                    response: &response,
                    nonce: STANDARD.encode(nonce),
                    version: &self.version,
                },
            )
        } else {
            let mut response = self.dispatch(&request, &data);
            wipe_json(&mut data);
            response["nonce"] = json!(STANDARD.encode(nonce));
            response["version"] = json!(self.version);
            let result = serde_json::to_writer(encoded.bounded_writer(MAX_MESSAGE), &response);
            wipe_json(&mut response);
            result
        };
        if encoding_result.is_err() {
            return protocol_error(&request, 4, "Cannot encode response");
        }
        let Some(cipher) = self.cipher.as_ref() else {
            return protocol_error(&request, 3, "Key exchange required");
        };
        match cipher.encrypt((&nonce).into(), encoded.as_slice()) {
            Ok(message) => json!({"action": request["action"], "message": STANDARD.encode(message),
                "nonce": STANDARD.encode(nonce)}),
            Err(_) => protocol_error(&request, 4, "Cannot encrypt response"),
        }
    }

    fn dispatch(&self, request: &Value, data: &Value) -> Value {
        let Ok(mut state) = vault().lock() else {
            return protocol_error(request, 1, "Vault unavailable");
        };
        if !state.unlocked {
            return protocol_error(request, 1, "Database not opened");
        }
        match request["action"].as_str().unwrap_or_default() {
            "get-databasehash" => json!({"action": "hash", "hash": state.hash, "success": "true"}),
            "test-associate" => {
                let id = data["id"].as_str().unwrap_or_default();
                if state
                    .associations
                    .get(id)
                    .zip(data["key"].as_str())
                    .is_some_and(|(stored, supplied)| stored == supplied)
                    && !id.is_empty()
                {
                    json!({"id": id, "hash": state.hash, "success": "true"})
                } else {
                    protocol_error(request, 8, "Association failed")
                }
            }
            "associate" => {
                if data["key"].as_str() != Some(&self.public_key)
                    || decode::<32>(&data["idKey"]).is_none()
                {
                    return protocol_error(request, 8, "Invalid association key");
                }
                if state
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.expires > Instant::now())
                    || state
                        .access
                        .as_ref()
                        .is_some_and(|access| access.expires > Instant::now())
                    || state.associations.len() >= 32
                {
                    return protocol_error(request, 8, "Association unavailable");
                }
                let token = Uuid::new_v4().to_string();
                let epoch = state.epoch;
                state.pending = Some(Pairing {
                    token: token.clone(),
                    expires: Instant::now() + PAIRING_TIMEOUT,
                    approved: None,
                });
                drop(state);
                loop {
                    std::thread::sleep(Duration::from_millis(50));
                    let Ok(mut state) = vault().lock() else {
                        return protocol_error(request, 8, "Association failed");
                    };
                    if !state.unlocked || state.epoch != epoch {
                        return protocol_error(request, 1, "Database not opened");
                    }
                    let Some(pending) = state
                        .pending
                        .as_ref()
                        .filter(|pending| pending.token == token)
                    else {
                        return protocol_error(request, 8, "Association cancelled");
                    };
                    if pending.expires <= Instant::now() || pending.approved == Some(false) {
                        state.pending = None;
                        return protocol_error(request, 8, "Association rejected");
                    }
                    if pending.approved == Some(true) {
                        state.pending = None;
                        let id = Uuid::new_v4().to_string();
                        let mut associations = state.associations.clone();
                        associations.insert(
                            id.clone(),
                            data["idKey"].as_str().unwrap_or_default().to_owned(),
                        );
                        if let Some(path) = &state.store_path {
                            let store = persistence::Store::from_parts(
                                state.hash.clone(),
                                associations.clone(),
                            );
                            if persistence::save(path, &store).is_err() {
                                return protocol_error(request, 8, "Cannot persist association");
                            }
                        }
                        state.associations = associations;
                        return json!({"id": id, "hash": state.hash, "success": "true"});
                    }
                }
            }
            _ => protocol_error(request, 12, "Credential operations are not implemented"),
        }
    }
}

fn incomplete_frame(error: io::Error) -> io::Error {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        io::Error::new(io::ErrorKind::InvalidData, "truncated browser frame")
    } else {
        error
    }
}

fn read_frame(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut size = [0; 4];
    // EOF at a frame boundary is normal; EOF inside a frame is malformed input.
    reader.read_exact(&mut size[..1])?;
    reader
        .read_exact(&mut size[1..])
        .map_err(incomplete_frame)?;
    let size = u32::from_ne_bytes(size) as usize;
    if size == 0 || size > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid browser frame length",
        ));
    }
    let mut message = vec![0; size];
    reader.read_exact(&mut message).map_err(incomplete_frame)?;
    Ok(message)
}

fn write_frame(writer: &mut impl Write, message: &[u8]) -> io::Result<()> {
    if message.is_empty() || message.len() > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid browser frame length",
        ));
    }
    writer.write_all(&(message.len() as u32).to_ne_bytes())?;
    writer.write_all(message)?;
    writer.flush()
}

#[cfg(unix)]
pub struct Server {
    path: std::path::PathBuf,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(unix)]
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        set_locked(true, true);
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
pub fn start(app_id: &str, version: &str) -> anyhow::Result<Server> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let runtime = PathBuf::from(
        std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| anyhow::anyhow!("XDG_RUNTIME_DIR is missing"))?,
    );
    let metadata = std::fs::symlink_metadata(&runtime)?;
    let uid = unsafe { libc::geteuid() };
    anyhow::ensure!(
        runtime.is_absolute()
            && metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == uid
            && metadata.permissions().mode() & 0o077 == 0,
        "runtime directory is not private"
    );
    anyhow::ensure!(
        !app_id.is_empty()
            && app_id.starts_with("org.kryptopass.")
            && app_id.bytes().all(|byte| byte.is_ascii_alphanumeric()
                || byte == b'.'
                || byte == b'-'
                || byte == b'_'),
        "invalid browser application identity"
    );
    let directory = runtime.join(app_id);
    match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = std::fs::symlink_metadata(&directory)?;
    anyhow::ensure!(
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == uid
            && metadata.permissions().mode() & 0o077 == 0,
        "browser directory is not private"
    );
    let path = directory.join("browser.sock");
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let version = version.to_owned();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let connections = Arc::new(AtomicUsize::new(0));
    std::thread::Builder::new()
        .name("browser-listener".into())
        .spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if connections.load(Ordering::Acquire) >= 8 {
                            continue;
                        }
                        connections.fetch_add(1, Ordering::AcqRel);
                        let active = connections.clone();
                        let version = version.clone();
                        let session_stop = stopping.clone();
                        let result = std::thread::Builder::new()
                            .name("browser-session".into())
                            .spawn(move || {
                                serve_connection(&mut stream, version, &session_stop);
                                active.fetch_sub(1, Ordering::AcqRel);
                            });
                        if result.is_err() {
                            connections.fetch_sub(1, Ordering::AcqRel);
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(50))
                    }
                    Err(_) => break,
                }
            }
        })?;
    Ok(Server { path, stop })
}

#[cfg(unix)]
fn serve_connection(
    stream: &mut std::os::unix::net::UnixStream,
    version: String,
    stop: &Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1800)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let Ok(output) = stream.try_clone() else {
        return;
    };
    let output = Arc::new(Mutex::new(output));
    let done = Arc::new(AtomicBool::new(false));
    let watcher_done = done.clone();
    let watcher_stop = stop.clone();
    let watcher_output = output.clone();
    let initial_epoch = vault().lock().ok().map(|state| state.epoch);
    let Ok(watcher) = std::thread::Builder::new().name("browser-status".into()).spawn(move || {
        let mut last_epoch = initial_epoch;
        while !watcher_done.load(Ordering::Acquire) && !watcher_stop.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(100));
            let Ok(state) = vault().lock() else { break; };
            if Some(state.epoch) != last_epoch {
                let event = json!({"action": if state.unlocked { "database-unlocked" } else { "database-locked" }}).to_string();
                let Ok(mut output) = watcher_output.lock() else { break; };
                if write_frame(&mut *output, event.as_bytes()).is_err() { break; }
                last_epoch = Some(state.epoch);
            }
        }
        if let Ok(output) = watcher_output.lock() { let _ = output.shutdown(std::net::Shutdown::Both); }
    }) else { return; };
    let mut session = Session {
        version,
        ..Session::default()
    };
    while let Ok(message) = read_frame(stream) {
        let Ok(request) = serde_json::from_slice::<Value>(&message) else {
            break;
        };
        let response_epoch = vault().lock().ok().map(|state| state.epoch);
        let sensitive = request["action"] == "get-logins";
        let response = session.handle(request);
        let Ok(encoded) = serde_json::to_vec(&response) else {
            break;
        };
        // Serialize authorization invalidation with the final credential write.
        let guard = if sensitive { vault().lock().ok() } else { None };
        if sensitive
            && !guard
                .as_ref()
                .is_some_and(|state| state.unlocked && Some(state.epoch) == response_epoch)
        {
            break;
        }
        let Ok(mut output) = output.lock() else {
            break;
        };
        if write_frame(&mut *output, &encoded).is_err() {
            break;
        }
    }
    done.store(true, Ordering::Release);
    let _ = watcher.join();
}

#[cfg(not(unix))]
pub struct Server;

#[cfg(not(unix))]
pub fn start(_app_id: &str, _version: &str) -> anyhow::Result<Server> {
    anyhow::bail!("browser integration currently requires Unix sockets")
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod transport_tests;

#[cfg(test)]
mod access_tests;

#[cfg(test)]
mod persistence_tests;

/// Relays bounded, length-prefixed JSON frames without decrypting the browser payload.
#[cfg(unix)]
pub fn run_proxy(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::net::UnixStream;
    let mut socket = UnixStream::connect(path)?;
    let mut responses = socket.try_clone()?;
    std::thread::Builder::new()
        .name("browser-proxy-output".into())
        .spawn(move || {
            let result = (|| -> io::Result<()> {
                let mut output = io::stdout().lock();
                loop {
                    let message = read_frame(&mut responses)?;
                    write_frame(&mut output, &message)?;
                }
            })();
            let code = match result {
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => 0,
                _ => 1,
            };
            std::process::exit(code);
        })?;
    let mut input = io::stdin().lock();
    loop {
        match read_frame(&mut input) {
            Ok(message) => write_frame(&mut socket, &message)?,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}
