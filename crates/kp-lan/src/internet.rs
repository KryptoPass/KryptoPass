use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::protocol::{CloseFrame, Message, WebSocketConfig, frame::coding::CloseCode},
};
use url::{Host, Url};
use webrtc::peer_connection::{RTCIceServer, RTCIceTransportPolicy};
use zeroize::Zeroize;

use crate::{
    LanError,
    address::{internet_udp_bind_addresses, is_supported_internet_address},
    host::{CandidatePolicy, negotiate_offer_answer},
    privacy::{NetworkPolicyGuard, NetworkPrivacyPolicy, NetworkUse},
    turn::fetch_credentials,
};

const RELAY_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RELAY_READY_TIMEOUT: Duration = Duration::from_secs(300);
const SIGNALING_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_SIGNALING_MESSAGE: usize = 256 * 1024;
const MAX_CONTROL_MESSAGE: usize = 256;
const MAX_MANUAL_ICE_SERVERS: usize = 4;
const MAX_ICE_SERVERS: usize = MAX_MANUAL_ICE_SERVERS;
const MAX_ICE_URLS_PER_SERVER: usize = 4;
const MAX_ICE_URLS_TOTAL: usize = MAX_ICE_SERVERS * MAX_ICE_URLS_PER_SERVER;
const MAX_BROKER_ICE_URLS_PER_SERVER: usize = MAX_ICE_URLS_PER_SERVER + 1;
const MAX_BROKER_ICE_URLS_TOTAL: usize = MAX_ICE_URLS_TOTAL + 1;
const MAX_ICE_URL_LENGTH: usize = 512;
const MAX_ICE_CREDENTIAL_LENGTH: usize = 1024;
const MAX_ICE_CONFIG_ENV_LENGTH: usize = 16 * 1024;

/// Candidate selection for an Internet WebRTC session.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InternetIceTransportPolicy {
    /// Use direct, STUN, and available TURN candidates.
    All,
    /// Permit only TURN relay candidates.
    Relay,
}

/// One locally configured STUN or TURN server.
///
/// Credential fields intentionally have no `Debug` implementation and are
/// never included in transport errors or tracing output.
#[derive(Clone)]
pub struct IceServerConfig {
    urls: Vec<String>,
    username: String,
    credential: String,
}

impl IceServerConfig {
    /// Create a STUN server entry from one STUN URL.
    #[must_use]
    pub fn stun(url: impl Into<String>) -> Self {
        Self {
            urls: vec![url.into()],
            username: String::new(),
            credential: String::new(),
        }
    }

    /// Create a TURN server entry with locally held credentials.
    #[must_use]
    pub fn turn(
        url: impl Into<String>,
        username: impl Into<String>,
        credential: impl Into<String>,
    ) -> Self {
        Self {
            urls: vec![url.into()],
            username: username.into(),
            credential: credential.into(),
        }
    }

    /// Create a server entry with the supplied URLs and credentials.
    ///
    /// Configuration validation enforces URL and credential limits. TURN URLs
    /// require credentials; ordinary STUN entries leave both credential
    /// strings empty. The broker path may combine the default STUN URL with
    /// returned TURN URLs to preserve the configured server-count limit.
    #[must_use]
    pub fn with_urls(
        urls: Vec<String>,
        username: impl Into<String>,
        credential: impl Into<String>,
    ) -> Self {
        Self {
            urls,
            username: username.into(),
            credential: credential.into(),
        }
    }

    /// Return the configured STUN/TURN endpoint URLs without credentials.
    #[must_use]
    pub fn urls(&self) -> &[String] {
        &self.urls
    }

    fn to_rtc_server(&self) -> RTCIceServer {
        RTCIceServer {
            urls: self.urls.clone(),
            username: self.username.clone(),
            credential: self.credential.clone(),
        }
    }
}

impl Drop for IceServerConfig {
    fn drop(&mut self) {
        self.username.zeroize();
        self.credential.zeroize();
    }
}

/// Locally configured Internet ICE servers.
///
/// The default uses Cloudflare's public STUN endpoint. These values stay on
/// the device and are never placed in the relay route or signaling messages.
#[derive(Clone)]
pub struct InternetIceConfig {
    servers: Vec<IceServerConfig>,
    broker_augmented: bool,
}

impl InternetIceConfig {
    /// Create an ICE configuration, replacing the default STUN server list.
    #[must_use]
    pub fn new(servers: Vec<IceServerConfig>) -> Self {
        Self {
            servers,
            broker_augmented: false,
        }
    }

    /// Load local ICE settings from `KP_INTERNET_ICE_SERVERS_JSON`.
    ///
    /// When the variable is absent, the default Cloudflare STUN server is
    /// used. When present, it must contain a strict JSON array of objects with
    /// exactly `urls`, `username`, and `credential` fields. An explicitly
    /// invalid value fails without falling back to the default.
    pub fn from_environment() -> Result<Self, LanError> {
        let Some(raw) = std::env::var_os("KP_INTERNET_ICE_SERVERS_JSON") else {
            return Ok(Self::default());
        };
        let raw = raw.into_string().map_err(|_| LanError::InvalidIceConfig)?;
        if raw.len() > MAX_ICE_CONFIG_ENV_LENGTH {
            return Err(LanError::InvalidIceConfig);
        }
        let entries: Vec<EnvironmentIceServer> =
            serde_json::from_str(&raw).map_err(|_| LanError::InvalidIceConfig)?;
        if entries.len() > MAX_MANUAL_ICE_SERVERS {
            return Err(LanError::InvalidIceConfig);
        }
        let config = Self::new(
            entries
                .into_iter()
                .map(|entry| {
                    IceServerConfig::with_urls(entry.urls, entry.username, entry.credential)
                })
                .collect(),
        );
        config.validate_and_convert()?;
        Ok(config)
    }

    /// Load local ICE settings and, when configured, obtain temporary TURN
    /// credentials using the supplied existing device identity signer.
    ///
    /// `KP_INTERNET_ICE_SERVERS_JSON` takes precedence over the broker. Without
    /// that override, broker failures retain the default STUN/direct path unless
    /// `KP_TURN_RELAY_ONLY=1` requires a relay candidate.
    pub async fn from_environment_with_turn_broker<F>(
        local_device_id: [u8; 32],
        signer: F,
    ) -> Result<(Self, InternetIceTransportPolicy), LanError>
    where
        F: Fn(&[u8]) -> [u8; 64] + Send + Sync,
    {
        let guard = NetworkPolicyGuard::new(NetworkPrivacyPolicy::default());
        Self::from_environment_with_policy(local_device_id, false, &guard, signer).await
    }

    /// Resolve the local privacy policy before broker access or any Internet
    /// side effect, then return one policy-consistent ICE configuration.
    pub async fn from_environment_with_policy<F>(
        local_device_id: [u8; 32],
        relay_only_requested: bool,
        guard: &NetworkPolicyGuard,
        signer: F,
    ) -> Result<(Self, InternetIceTransportPolicy), LanError>
    where
        F: Fn(&[u8]) -> [u8; 64] + Send + Sync,
    {
        guard
            .preflight(NetworkUse::Internet)
            .map_err(|_| LanError::NetworkPolicy)?;
        let relay_only = match std::env::var_os("KP_TURN_RELAY_ONLY") {
            None => false,
            Some(value) if value == "1" => true,
            Some(_) => return Err(LanError::InvalidIceConfig),
        } || relay_only_requested;
        if std::env::var_os("KP_INTERNET_ICE_SERVERS_JSON").is_some() {
            let config = Self::from_environment()?;
            guard
                .ensure_current()
                .map_err(|_| LanError::NetworkPolicy)?;
            let result = config_from_sources(Some(config), relay_only, None);
            guard
                .ensure_current()
                .map_err(|_| LanError::NetworkPolicy)?;
            return result;
        }

        guard
            .ensure_current()
            .map_err(|_| LanError::NetworkPolicy)?;
        let broker_result = match std::env::var_os("KP_TURN_BROKER_ORIGIN") {
            None => None,
            Some(origin) => {
                let operation = async {
                    match origin.into_string() {
                        Ok(origin) => fetch_credentials(&origin, local_device_id, signer).await,
                        Err(_) => Err(LanError::TurnBroker),
                    }
                };
                Some(tokio::select! {
                    result = operation => result,
                    _ = guard.wait_for_change(Duration::from_millis(250)) => {
                        return Err(LanError::NetworkPolicy);
                    }
                })
            }
        };
        guard
            .ensure_current()
            .map_err(|_| LanError::NetworkPolicy)?;
        let result = config_from_sources(None, relay_only, broker_result);
        guard
            .ensure_current()
            .map_err(|_| LanError::NetworkPolicy)?;
        result
    }

    /// Return the locally configured ICE servers.
    #[must_use]
    pub fn servers(&self) -> &[IceServerConfig] {
        &self.servers
    }

    fn retain_turn_only(&mut self) {
        for server in &mut self.servers {
            server.urls.retain(|url| {
                url.split_once(':').is_some_and(|(scheme, _)| {
                    scheme.eq_ignore_ascii_case("turn") || scheme.eq_ignore_ascii_case("turns")
                })
            });
        }
        self.servers.retain(|server| !server.urls.is_empty());
    }

    pub(crate) fn validate_and_convert(&self) -> Result<Vec<RTCIceServer>, LanError> {
        let max_urls_per_server = if self.broker_augmented {
            MAX_BROKER_ICE_URLS_PER_SERVER
        } else {
            MAX_ICE_URLS_PER_SERVER
        };
        let max_total_urls = if self.broker_augmented {
            MAX_BROKER_ICE_URLS_TOTAL
        } else {
            MAX_ICE_URLS_TOTAL
        };
        if self.servers.len() > MAX_ICE_SERVERS {
            return Err(LanError::InvalidIceConfig);
        }
        let mut total_urls = 0_usize;
        let mut converted = Vec::with_capacity(self.servers.len());
        for server in &self.servers {
            if server.urls.is_empty()
                || server.urls.len() > max_urls_per_server
                || server
                    .username
                    .len()
                    .checked_add(server.credential.len())
                    .is_none_or(|length| length > MAX_ICE_CREDENTIAL_LENGTH)
                || server.username.chars().any(char::is_control)
                || server.credential.chars().any(char::is_control)
            {
                return Err(LanError::InvalidIceConfig);
            }
            total_urls = total_urls
                .checked_add(server.urls.len())
                .ok_or(LanError::InvalidIceConfig)?;
            if total_urls > max_total_urls {
                return Err(LanError::InvalidIceConfig);
            }

            let mut has_turn = false;
            let mut has_stun = false;
            for url in &server.urls {
                if url.is_empty()
                    || url.len() > MAX_ICE_URL_LENGTH
                    || url.chars().any(char::is_control)
                    || url.chars().any(char::is_whitespace)
                    || url.contains(['@', '#'])
                {
                    return Err(LanError::InvalidIceConfig);
                }
                let (scheme, _) = url.split_once(':').ok_or(LanError::InvalidIceConfig)?;
                match scheme.to_ascii_lowercase().as_str() {
                    "stun" | "stuns" => has_stun = true,
                    "turn" | "turns" => has_turn = true,
                    _ => return Err(LanError::InvalidIceConfig),
                }
            }
            if has_turn && has_stun && !self.broker_augmented {
                return Err(LanError::InvalidIceConfig);
            }
            if has_turn {
                if server.username.is_empty() || server.credential.is_empty() {
                    return Err(LanError::InvalidIceConfig);
                }
            } else if !server.username.is_empty() || !server.credential.is_empty() {
                return Err(LanError::InvalidIceConfig);
            }

            let rtc_server = server.to_rtc_server();
            rtc_server.urls().map_err(|_| LanError::InvalidIceConfig)?;
            converted.push(rtc_server);
        }
        Ok(converted)
    }

    fn has_supported_udp_turn(&self) -> bool {
        self.servers
            .iter()
            .flat_map(|server| &server.urls)
            .any(|url| is_supported_udp_turn_url(url))
    }
}

pub(crate) fn config_from_sources(
    manual: Option<InternetIceConfig>,
    relay_only: bool,
    broker_result: Option<Result<Vec<IceServerConfig>, LanError>>,
) -> Result<(InternetIceConfig, InternetIceTransportPolicy), LanError> {
    let policy = if relay_only {
        InternetIceTransportPolicy::Relay
    } else {
        InternetIceTransportPolicy::All
    };
    if let Some(mut config) = manual {
        if relay_only {
            config.retain_turn_only();
        }
        config.validate_and_convert()?;
        if relay_only && !config.has_supported_udp_turn() {
            return Err(LanError::InvalidIceConfig);
        }
        return Ok((config, policy));
    }

    let mut config = InternetIceConfig::default();
    match broker_result {
        Some(Ok(mut servers)) => {
            if !relay_only {
                let Some(first_server) = servers.first_mut() else {
                    return Err(LanError::TurnBroker);
                };
                first_server
                    .urls
                    .insert(0, "stun:stun.cloudflare.com:3478".to_owned());
            }
            config = InternetIceConfig::new(servers);
            config.broker_augmented = true;
        }
        Some(Err(error)) if relay_only => return Err(error),
        Some(Err(_)) => {
            tracing::warn!("TURN broker unavailable; using STUN/direct ICE");
        }
        None => {}
    }
    if relay_only {
        config.retain_turn_only();
    }
    if relay_only && !config.has_supported_udp_turn() {
        return Err(LanError::TurnBroker);
    }
    config.validate_and_convert()?;
    Ok((config, policy))
}

fn is_supported_udp_turn_url(value: &str) -> bool {
    let Some((scheme, endpoint)) = value.split_once(':') else {
        return false;
    };
    if scheme != "turn" {
        return false;
    }
    match endpoint.split_once('?') {
        None => true,
        Some((_, "transport=udp")) => true,
        Some(_) => false,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentIceServer {
    urls: Vec<String>,
    username: String,
    credential: String,
}

impl Default for InternetIceConfig {
    fn default() -> Self {
        Self::new(vec![IceServerConfig::stun("stun:stun.cloudflare.com:3478")])
    }
}

/// Public signaling origin and room selector for one relay session.
///
/// The room is a routing value, not an authorization token. The relay path
/// carries no invitation, TURN credentials, or vault data.
#[derive(Clone)]
pub struct RelayRoute {
    relay_origin: String,
    room: [u8; 32],
}

impl RelayRoute {
    /// Validate and normalize a WSS origin and non-zero 32-byte room.
    ///
    /// `ws://` is accepted only for loopback hosts in debug builds so local
    /// transport tests can run without weakening release TLS validation.
    pub fn new(relay_origin: &str, room: [u8; 32]) -> Result<Self, LanError> {
        if relay_origin.is_empty()
            || relay_origin.len() > 2048
            || room.iter().all(|byte| *byte == 0)
        {
            return Err(LanError::InvalidRelayRoute);
        }
        let parsed = Url::parse(relay_origin).map_err(|_| LanError::InvalidRelayRoute)?;
        if parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
            || parsed.port() == Some(0)
            || parsed.host().is_none()
        {
            return Err(LanError::InvalidRelayRoute);
        }

        if relay_origin.trim() != relay_origin || relay_origin.chars().any(char::is_control) {
            return Err(LanError::InvalidRelayRoute);
        }
        let after_scheme = relay_origin
            .split_once("://")
            .ok_or(LanError::InvalidRelayRoute)?;
        let remainder = after_scheme.1;
        let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
        let authority = &remainder[..authority_end];
        let suffix = &remainder[authority_end..];
        if !suffix.is_empty() && suffix != "/" {
            return Err(LanError::InvalidRelayRoute);
        }
        if authority.contains('@') {
            return Err(LanError::InvalidRelayRoute);
        }

        match parsed.scheme() {
            "wss" => {}
            "ws" if cfg!(debug_assertions) && is_loopback_host(parsed.host()) => {}
            _ => return Err(LanError::InvalidRelayRoute),
        }

        Ok(Self {
            relay_origin: parsed.origin().ascii_serialization(),
            room,
        })
    }

    /// Return the normalized relay origin without a trailing slash.
    #[must_use]
    pub fn relay_origin(&self) -> &str {
        &self.relay_origin
    }

    /// Return the 32-byte public room selector.
    #[must_use]
    pub const fn room(&self) -> [u8; 32] {
        self.room
    }

    /// Return the room selector as 64 lower-case hexadecimal characters.
    #[must_use]
    pub fn room_hex(&self) -> String {
        lower_hex(&self.room)
    }

    fn host_url(&self) -> String {
        format!("{}/kp/relay/v1/{}/host", self.relay_origin, self.room_hex())
    }
}

/// An outbound relay host ready to accept one signaling attempt.
///
/// Connecting opens the WebSocket but deliberately does not wait for the
/// untrusted relay to report that a joining device is present.
pub struct RelayHost {
    websocket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    ice_servers: Vec<RTCIceServer>,
    ice_policy: InternetIceTransportPolicy,
    privacy_guard: NetworkPolicyGuard,
}

impl RelayHost {
    /// Open a bounded outbound WebSocket to this route's host endpoint.
    pub async fn connect(route: RelayRoute, config: InternetIceConfig) -> Result<Self, LanError> {
        Self::connect_with_policy(route, config, InternetIceTransportPolicy::All).await
    }

    /// Open a signaling connection with an explicit ICE candidate policy.
    pub async fn connect_with_policy(
        route: RelayRoute,
        config: InternetIceConfig,
        ice_policy: InternetIceTransportPolicy,
    ) -> Result<Self, LanError> {
        let guard = NetworkPolicyGuard::new(NetworkPrivacyPolicy {
            relay_only: ice_policy == InternetIceTransportPolicy::Relay,
            ..NetworkPrivacyPolicy::default()
        });
        Self::connect_with_privacy(route, config, ice_policy, guard).await
    }

    /// Open signaling only after the local policy permits Internet use.
    pub async fn connect_with_privacy(
        route: RelayRoute,
        mut config: InternetIceConfig,
        ice_policy: InternetIceTransportPolicy,
        privacy_guard: NetworkPolicyGuard,
    ) -> Result<Self, LanError> {
        privacy_guard
            .preflight(NetworkUse::Internet)
            .map_err(|_| LanError::NetworkPolicy)?;
        let ice_policy = if privacy_guard.relay_only() {
            config.retain_turn_only();
            InternetIceTransportPolicy::Relay
        } else {
            ice_policy
        };
        if ice_policy == InternetIceTransportPolicy::Relay && !config.has_supported_udp_turn() {
            return Err(LanError::InvalidIceConfig);
        }
        let ice_servers = config.validate_and_convert()?;
        let url = route.host_url();
        privacy_guard
            .ensure_current()
            .map_err(|_| LanError::NetworkPolicy)?;
        let operation = timeout(
            RELAY_CONNECT_TIMEOUT,
            connect_async_with_config(url.as_str(), Some(websocket_config()), false),
        );
        let connected = tokio::select! {
            result = operation => result.map_err(|_| LanError::Timeout)?
                .map_err(|_| LanError::Signaling)?,
            _ = privacy_guard.wait_for_change(Duration::from_millis(250)) => {
                return Err(LanError::NetworkPolicy);
            }
        };
        // The preflight checked the default route; this checks the socket the
        // system actually used, before any signaling message is exchanged.
        let local_address = connected
            .0
            .get_ref()
            .get_ref()
            .local_addr()
            .map_err(|_| LanError::Signaling)?
            .ip();
        privacy_guard
            .ensure_internet_source(local_address)
            .map_err(|_| LanError::NetworkPolicy)?;
        Ok(Self {
            websocket: connected.0,
            ice_servers,
            ice_policy,
            privacy_guard,
        })
    }

    /// Wait for the relay pair, authorize the requested device selector, and
    /// negotiate one fresh reliable DataChannel.
    ///
    /// Authorization runs after a strict `request` message is parsed and
    /// before any WebRTC peer connection or ICE gathering is created. Noise
    /// must still authenticate the returned selector before the app marks the
    /// session connected.
    pub async fn accept<F>(
        mut self,
        session_id: [u8; 16],
        authorize: F,
    ) -> Result<crate::ResumeAttempt, LanError>
    where
        F: Fn([u8; 32]) -> bool + Send + Sync + 'static,
    {
        if session_id.iter().all(|byte| *byte == 0) {
            return Err(LanError::InvalidSession);
        }
        self.privacy_guard
            .ensure_current()
            .map_err(|_| LanError::NetworkPolicy)?;
        let transport_guard = self.privacy_guard.clone();
        tokio::select! {
            result = timeout(RELAY_READY_TIMEOUT, receive_ready(&mut self.websocket)) => {
                result.map_err(|_| LanError::Timeout)??;
            }
            _ = self.privacy_guard.wait_for_change(Duration::from_millis(250)) => {
                return Err(LanError::NetworkPolicy);
            }
        }

        let mut attempt = timeout(SIGNALING_TIMEOUT, async move {
            let initiator_device_id = receive_request(&mut self.websocket).await?;
            if !authorize(initiator_device_id) {
                return Err(LanError::UnauthorizedDevice);
            }
            self.privacy_guard
                .ensure_current()
                .map_err(|_| LanError::NetworkPolicy)?;

            let session = lower_hex(&session_id);
            let resume = ResumeNotice {
                v: 1,
                kind: "resume",
                session: &session,
            };
            let payload = serde_json::to_string(&resume).map_err(|_| LanError::Signaling)?;
            send_text(&mut self.websocket, payload).await?;

            let route_address = self
                .websocket
                .get_ref()
                .get_ref()
                .local_addr()
                .map_err(|_| LanError::Signaling)?
                .ip();
            let allow_loopback = route_address.is_loopback();
            if !is_supported_internet_address(route_address) && !allow_loopback {
                return Err(LanError::InvalidAddress);
            }
            let bind_addresses = internet_udp_bind_addresses(route_address)?;
            self.privacy_guard
                .ensure_current()
                .map_err(|_| LanError::NetworkPolicy)?;
            let policy = if self.ice_policy == InternetIceTransportPolicy::Relay {
                CandidatePolicy::InternetRelayOnly { allow_loopback }
            } else {
                CandidatePolicy::Internet { allow_loopback }
            };
            let negotiation = negotiate_offer_answer(
                &mut self.websocket,
                bind_addresses,
                self.ice_servers,
                policy,
                match self.ice_policy {
                    InternetIceTransportPolicy::All => RTCIceTransportPolicy::All,
                    InternetIceTransportPolicy::Relay => RTCIceTransportPolicy::Relay,
                },
            );
            let transport = tokio::select! {
                result = negotiation => result?,
                _ = self.privacy_guard.wait_for_change(Duration::from_millis(250)) => {
                    return Err(LanError::NetworkPolicy);
                }
            };
            let close = self.websocket.close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: "".into(),
            }));
            let _ = timeout(Duration::from_millis(250), close).await;
            Ok(crate::ResumeAttempt {
                initiator_device_id,
                session_id,
                transport,
            })
        })
        .await
        .map_err(|_| LanError::Timeout)??;
        attempt
            .transport
            .enforce_network_policy(transport_guard)
            .map_err(|_| LanError::NetworkPolicy)?;
        Ok(attempt)
    }
}

fn websocket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .read_buffer_size(8 * 1024)
        .write_buffer_size(8 * 1024)
        .max_write_buffer_size(MAX_SIGNALING_MESSAGE + 16 * 1024)
        .max_message_size(Some(MAX_SIGNALING_MESSAGE))
        .max_frame_size(Some(MAX_SIGNALING_MESSAGE))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadyMessage {
    v: u8,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestMessage {
    v: u8,
    #[serde(rename = "type")]
    kind: String,
    device: String,
}

#[derive(Serialize)]
struct ResumeNotice<'a> {
    v: u8,
    #[serde(rename = "type")]
    kind: &'static str,
    session: &'a str,
}

async fn receive_ready<S>(websocket: &mut WebSocketStream<S>) -> Result<(), LanError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let payload = receive_text(websocket, MAX_CONTROL_MESSAGE).await?;
    let ready: ReadyMessage =
        serde_json::from_str(&payload).map_err(|_| LanError::InvalidSignaling)?;
    if ready.v != 1 || ready.kind != "ready" {
        return Err(LanError::InvalidSignaling);
    }
    Ok(())
}

async fn receive_request<S>(websocket: &mut WebSocketStream<S>) -> Result<[u8; 32], LanError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let payload = receive_text(websocket, MAX_CONTROL_MESSAGE).await?;
    let request: RequestMessage =
        serde_json::from_str(&payload).map_err(|_| LanError::InvalidSignaling)?;
    if request.v != 1 || request.kind != "request" {
        return Err(LanError::InvalidSignaling);
    }
    parse_device_id(&request.device).ok_or(LanError::InvalidSignaling)
}

async fn receive_text<S>(
    websocket: &mut WebSocketStream<S>,
    maximum: usize,
) -> Result<String, LanError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        let message = websocket
            .next()
            .await
            .ok_or(LanError::InvalidSignaling)?
            .map_err(|_| LanError::Signaling)?;
        match message {
            Message::Text(text) => {
                if text.len() > maximum {
                    return Err(LanError::InvalidSignaling);
                }
                return Ok(text.to_string());
            }
            Message::Ping(payload) => websocket
                .send(Message::Pong(payload))
                .await
                .map_err(|_| LanError::Signaling)?,
            Message::Pong(_) => {}
            Message::Close(_) | Message::Binary(_) | Message::Frame(_) => {
                return Err(LanError::InvalidSignaling);
            }
        }
    }
}

async fn send_text<S>(websocket: &mut WebSocketStream<S>, text: String) -> Result<(), LanError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    if text.len() > MAX_CONTROL_MESSAGE {
        return Err(LanError::InvalidSignaling);
    }
    websocket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| LanError::Signaling)
}

fn parse_device_id(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut device_id = [0_u8; 32];
    for (index, byte) in device_id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    device_id.iter().any(|byte| *byte != 0).then_some(device_id)
}

fn lower_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn is_loopback_host(host: Option<Host<&str>>) -> bool {
    match host {
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        Some(Host::Domain(name)) => {
            name.eq_ignore_ascii_case("localhost")
                || name.eq_ignore_ascii_case("localhost.")
                || name.to_ascii_lowercase().ends_with(".localhost")
        }
        None => false,
    }
}
