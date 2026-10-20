use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use reqwest::{
    Client, StatusCode,
    header::{ACCEPT, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use url::{Host, Url};
use zeroize::Zeroizing;

use crate::{IceServerConfig, LanError};

const BROKER_ENDPOINT: &str = "/kp/turn/v1/credentials";
const BROKER_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REQUEST_BYTES: usize = 2048;
const MAX_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_SERVERS: usize = 4;
const MAX_URLS_PER_SERVER: usize = 4;
const MAX_URLS_TOTAL: usize = MAX_SERVERS * MAX_URLS_PER_SERVER;
const MAX_CREDENTIAL_BYTES: usize = 1024;
const CACHE_SKEW_SECONDS: u64 = 120;
const MAX_CACHE_ENTRIES: usize = 64;
const SIGNING_DOMAIN: &[u8] = b"KryptoPass TURN credentials v1\0";

static CACHE: OnceLock<Mutex<HashMap<CacheKey, CacheEntry>>> = OnceLock::new();

struct CacheKey {
    origin: String,
    device_id: [u8; 32],
}

impl PartialEq for CacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.origin == other.origin && self.device_id == other.device_id
    }
}

impl Eq for CacheKey {}

impl std::hash::Hash for CacheKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.origin.hash(state);
        self.device_id.hash(state);
    }
}

struct CacheEntry {
    servers: Vec<IceServerConfig>,
    cache_until: u64,
}

#[derive(Serialize)]
struct CredentialRequest<'a> {
    v: u8,
    device: &'a str,
    timestamp: u64,
    nonce: &'a str,
    signature: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialResponse {
    v: u8,
    expires_at: u64,
    ice_servers: Vec<CredentialIceServer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialIceServer {
    urls: Vec<String>,
    username: String,
    credential: String,
}

impl Drop for CredentialIceServer {
    fn drop(&mut self) {
        use zeroize::Zeroize;

        self.username.zeroize();
        self.credential.zeroize();
    }
}

/// Fetch fresh credentials from the locally configured HTTPS broker.
///
/// The response is accepted only for the official Cloudflare TURN endpoints.
/// TCP/TLS endpoints are validated and then omitted because this WebRTC build
/// supports TURN over UDP only. Successful credentials are kept in a bounded
/// process-memory cache keyed by broker origin and local device ID.
pub(super) async fn fetch_credentials<F>(
    origin: &str,
    device_id: [u8; 32],
    signer: F,
) -> Result<Vec<IceServerConfig>, LanError>
where
    F: Fn(&[u8]) -> [u8; 64] + Send + Sync,
{
    validate_origin(origin, false)?;
    if device_id.iter().all(|byte| *byte == 0) {
        return Err(LanError::TurnBroker);
    }

    let cache_key = CacheKey {
        origin: origin.to_owned(),
        device_id,
    };
    let now = unix_time()?;
    if let Some(servers) = cached_servers(&cache_key, now)? {
        return Ok(servers);
    }

    let timestamp = now;
    let nonce = fresh_nonce()?;
    let signature_message = signing_message(origin, &device_id, timestamp, &nonce)?;
    let signature = signer(&signature_message);
    let device_hex = lower_hex(&device_id);
    let nonce_hex = lower_hex(&nonce);
    let signature_hex = lower_hex(&signature);
    let request = CredentialRequest {
        v: 1,
        device: &device_hex,
        timestamp,
        nonce: &nonce_hex,
        signature: &signature_hex,
    };
    let request_body = serde_json::to_vec(&request).map_err(|_| LanError::TurnBroker)?;
    if request_body.len() > MAX_REQUEST_BYTES {
        return Err(LanError::TurnBroker);
    }

    let client = Client::builder()
        .connect_timeout(BROKER_TIMEOUT)
        .timeout(BROKER_TIMEOUT)
        .redirect(Policy::none())
        .no_proxy()
        .build()
        .map_err(|_| LanError::TurnBroker)?;
    let response = client
        .post(format!("{origin}{BROKER_ENDPOINT}"))
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(CACHE_CONTROL, "no-store")
        .body(request_body)
        .send()
        .await
        .map_err(|_| LanError::TurnBroker)?;
    if response.status() != StatusCode::OK
        || !is_json_response(response.headers().get(CONTENT_TYPE))
        || !has_no_store(response.headers().get(CACHE_CONTROL))
        || response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|header| header.to_str().ok())
            .and_then(|length| length.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
    {
        return Err(LanError::TurnBroker);
    }

    let mut body = Zeroizing::new(Vec::with_capacity(MAX_RESPONSE_BYTES.min(4096)));
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| LanError::TurnBroker)?;
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(LanError::TurnBroker);
        }
        body.extend_from_slice(&chunk);
    }

    let now = unix_time()?;
    let (servers, cache_until) = parse_response(&body, now)?;
    if cache_until > now {
        store_cache(cache_key, servers.clone(), cache_until)?;
    }
    Ok(servers)
}

fn parse_response(body: &[u8], now: u64) -> Result<(Vec<IceServerConfig>, u64), LanError> {
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(LanError::TurnBroker);
    }
    let response: CredentialResponse =
        serde_json::from_slice(body).map_err(|_| LanError::TurnBroker)?;
    let latest_expiry = now.checked_add(3660).ok_or(LanError::TurnBroker)?;
    if response.v != 1
        || response.expires_at <= now.saturating_add(30)
        || response.expires_at > latest_expiry
        || response.ice_servers.is_empty()
        || response.ice_servers.len() > MAX_SERVERS
    {
        return Err(LanError::TurnBroker);
    }

    let mut total_urls = 0_usize;
    let mut servers = Vec::with_capacity(response.ice_servers.len());
    for mut server in response.ice_servers {
        if server.urls.is_empty()
            || server.urls.len() > MAX_URLS_PER_SERVER
            || server
                .username
                .len()
                .checked_add(server.credential.len())
                .is_none_or(|length| length > MAX_CREDENTIAL_BYTES)
            || server.username.is_empty()
            || server.credential.is_empty()
            || server.username.chars().any(char::is_control)
            || server.credential.chars().any(char::is_control)
        {
            return Err(LanError::TurnBroker);
        }
        total_urls = total_urls
            .checked_add(server.urls.len())
            .ok_or(LanError::TurnBroker)?;
        if total_urls > MAX_URLS_TOTAL {
            return Err(LanError::TurnBroker);
        }

        let mut udp_urls = Vec::with_capacity(server.urls.len());
        for url in &server.urls {
            if !is_official_turn_url(url) {
                return Err(LanError::TurnBroker);
            }
            if is_supported_udp_url(url) {
                udp_urls.push(url.clone());
            }
        }
        if !udp_urls.is_empty() {
            servers.push(IceServerConfig::with_urls(
                udp_urls,
                std::mem::take(&mut server.username),
                std::mem::take(&mut server.credential),
            ));
        }
    }
    if servers.is_empty() {
        return Err(LanError::TurnBroker);
    }

    Ok((
        servers,
        response.expires_at.saturating_sub(CACHE_SKEW_SECONDS),
    ))
}

fn cached_servers(key: &CacheKey, now: u64) -> Result<Option<Vec<IceServerConfig>>, LanError> {
    let Some(cache) = CACHE.get() else {
        return Ok(None);
    };
    let mut cache = cache.lock().map_err(|_| LanError::TurnBroker)?;
    cache.retain(|_, entry| entry.cache_until > now);
    Ok(cache
        .get(key)
        .filter(|entry| entry.cache_until > now)
        .map(|entry| entry.servers.clone()))
}

fn store_cache(
    key: CacheKey,
    servers: Vec<IceServerConfig>,
    cache_until: u64,
) -> Result<(), LanError> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().map_err(|_| LanError::TurnBroker)?;
    if cache.len() >= MAX_CACHE_ENTRIES && !cache.contains_key(&key) {
        if let Some(oldest) =
            cache
                .iter()
                .min_by_key(|(_, entry)| entry.cache_until)
                .map(|(key, _)| CacheKey {
                    origin: key.origin.clone(),
                    device_id: key.device_id,
                })
        {
            cache.remove(&oldest);
        }
    }
    cache.insert(
        key,
        CacheEntry {
            servers,
            cache_until,
        },
    );
    Ok(())
}

fn validate_origin(origin: &str, allow_loopback_http: bool) -> Result<(), LanError> {
    if origin.is_empty()
        || origin.len() > 512
        || !origin.is_ascii()
        || origin.trim() != origin
        || origin.chars().any(char::is_control)
    {
        return Err(LanError::TurnBroker);
    }
    let parsed = Url::parse(origin).map_err(|_| LanError::TurnBroker)?;
    let valid_scheme = parsed.scheme() == "https"
        || (allow_loopback_http && parsed.scheme() == "http" && is_loopback(parsed.host()));
    if !valid_scheme
        || parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
        || parsed.port() == Some(0)
        || parsed.host().is_none()
        || parsed.origin().ascii_serialization() != origin
    {
        return Err(LanError::TurnBroker);
    }
    Ok(())
}

fn signing_message(
    origin: &str,
    device_id: &[u8; 32],
    timestamp: u64,
    nonce: &[u8; 32],
) -> Result<Vec<u8>, LanError> {
    let origin_len = u32::try_from(origin.len()).map_err(|_| LanError::TurnBroker)?;
    let mut message = Vec::with_capacity(SIGNING_DOMAIN.len() + 4 + origin.len() + 32 + 8 + 32);
    message.extend_from_slice(SIGNING_DOMAIN);
    message.extend_from_slice(&origin_len.to_be_bytes());
    message.extend_from_slice(origin.as_bytes());
    message.extend_from_slice(device_id);
    message.extend_from_slice(&timestamp.to_be_bytes());
    message.extend_from_slice(nonce);
    Ok(message)
}

fn fresh_nonce() -> Result<[u8; 32], LanError> {
    for _ in 0..4 {
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| LanError::TurnBroker)?;
        if nonce.iter().any(|byte| *byte != 0) {
            return Ok(nonce);
        }
    }
    Err(LanError::TurnBroker)
}

fn is_json_response(content_type: Option<&reqwest::header::HeaderValue>) -> bool {
    content_type
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
}

fn has_no_store(cache_control: Option<&reqwest::header::HeaderValue>) -> bool {
    cache_control
        .and_then(|value| value.to_str().ok())
        .is_some_and(|directives| {
            directives
                .split(',')
                .any(|directive| directive.trim().eq_ignore_ascii_case("no-store"))
        })
}

fn is_official_turn_url(value: &str) -> bool {
    matches!(
        value,
        "turn:turn.cloudflare.com:3478?transport=udp"
            | "turn:turn.cloudflare.com:443?transport=udp"
            | "turn:turn.cloudflare.com:3478?transport=tcp"
            | "turns:turn.cloudflare.com:443?transport=tcp"
    )
}

fn is_supported_udp_url(value: &str) -> bool {
    matches!(
        value,
        "turn:turn.cloudflare.com:3478?transport=udp"
            | "turn:turn.cloudflare.com:443?transport=udp"
    )
}

fn is_loopback(host: Option<Host<&str>>) -> bool {
    matches!(host, Some(Host::Ipv4(ip)) if ip.is_loopback())
        || matches!(host, Some(Host::Ipv6(ip)) if ip.is_loopback())
}

fn unix_time() -> Result<u64, LanError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LanError::TurnBroker)
        .map(|duration| duration.as_secs())
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

#[cfg(test)]
#[path = "turn_tests.rs"]
mod tests;
