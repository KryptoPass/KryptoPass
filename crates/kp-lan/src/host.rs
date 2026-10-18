use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{mpsc, watch},
    time::timeout,
};
use tokio_tungstenite::{
    accept_hdr_async_with_config,
    tungstenite::{
        handshake::server::{ErrorResponse, Request, Response},
        protocol::{CloseFrame, Message, WebSocketConfig, frame::coding::CloseCode},
    },
};
use webrtc::{
    data_channel::DataChannel,
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
        RTCIceGatheringState, RTCSessionDescription,
    },
};

use crate::{LanError, address::is_supported_address, transport::WebRtcTransport};

const SERVICE_TYPE: &str = "_kryptopass._tcp.local.";
const DATA_CHANNEL_LABEL: &str = "kryptopass-v1";
const SIGNALING_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_SIGNALING_MESSAGE: usize = 256 * 1024;
const MAX_CANDIDATES: usize = 64;
const DATA_CHANNEL_SEND_BUFFER_LIMIT: usize = 256 * 1024;

/// TCP signaling endpoint and optional mDNS advertisement for one pairing session.
///
/// The listener is bound to the exact address supplied to [`bind`](Self::bind).
/// An mDNS error only disables discovery; the returned host remains usable with
/// an address entered or discovered through another local mechanism.
pub struct LanHost {
    listener: TcpListener,
    address: IpAddr,
    session_id: [u8; 16],
    discovery: Option<MdnsRegistration>,
}

impl LanHost {
    /// Bind a signaling listener to an explicit loopback or LAN address.
    ///
    /// The session identifier is used only in the local WebSocket path. It is
    /// never included in DNS-SD records.
    pub async fn bind(address: IpAddr, session_id: [u8; 16]) -> Result<Self, LanError> {
        if !is_supported_address(address) {
            return Err(LanError::InvalidAddress);
        }
        if session_id.iter().all(|byte| *byte == 0) {
            return Err(LanError::InvalidSession);
        }

        let listener = TcpListener::bind(SocketAddr::new(address, 0))
            .await
            .map_err(|_| LanError::Bind)?;
        let port = listener.local_addr().map_err(|_| LanError::Bind)?.port();
        let discovery = register_mdns(address, port);
        tracing::info!(mdns_available = discovery.is_some(), "LAN listener ready");

        Ok(Self {
            listener,
            address,
            session_id,
            discovery,
        })
    }

    /// Return the ephemeral TCP port used for local signaling.
    pub fn port(&self) -> u16 {
        self.listener
            .local_addr()
            .map(|address| address.port())
            .unwrap_or_default()
    }

    /// Whether mDNS registration was accepted by the local daemon.
    ///
    /// This is a best-effort registration status, not a guarantee that the
    /// service has reached every interface or peer.
    pub fn discovery_available(&self) -> bool {
        self.discovery.is_some()
    }

    /// Accept one Android-style SDP offer and establish a reliable DataChannel.
    ///
    /// This first waits for the TCP signaling client. Once TCP connects, a
    /// 30-second deadline covers WebSocket signaling, ICE gathering, and
    /// DataChannel opening. Exactly one offer and one in-band
    /// `kryptopass-v1` data channel are accepted.
    pub async fn accept(self) -> Result<WebRtcTransport, LanError> {
        let (socket, remote) = self
            .listener
            .accept()
            .await
            .map_err(|_| LanError::Signaling)?;
        match timeout(SIGNALING_TIMEOUT, self.accept_inner(socket, remote)).await {
            Ok(result) => result,
            Err(_) => at_stage(Err(LanError::Timeout), "negotiation_deadline"),
        }
    }

    async fn accept_inner(
        self,
        socket: tokio::net::TcpStream,
        remote: SocketAddr,
    ) -> Result<WebRtcTransport, LanError> {
        let expected_path = signaling_path(&self.session_id);
        if !is_supported_address(remote.ip()) || remote.is_ipv4() != self.address.is_ipv4() {
            return Err(LanError::InvalidSignaling);
        }

        let ws_config = WebSocketConfig::default()
            .read_buffer_size(8 * 1024)
            .write_buffer_size(8 * 1024)
            .max_write_buffer_size(MAX_SIGNALING_MESSAGE + 16 * 1024)
            .max_message_size(Some(MAX_SIGNALING_MESSAGE))
            .max_frame_size(Some(MAX_SIGNALING_MESSAGE));
        let mut websocket = at_stage(
            accept_hdr_async_with_config(
                socket,
                move |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
                    let valid =
                        request.uri().path() == expected_path && request.uri().query().is_none();
                    if valid {
                        Ok(response)
                    } else {
                        let mut response = tokio_tungstenite::tungstenite::http::Response::new(
                            Some("invalid signaling path".to_owned()),
                        );
                        *response.status_mut() =
                            tokio_tungstenite::tungstenite::http::StatusCode::BAD_REQUEST;
                        Err(response)
                    }
                },
                Some(ws_config),
            )
            .await
            .map_err(|_| LanError::InvalidSignaling),
            "websocket_upgrade",
        )?;

        let offer = at_stage(receive_offer(&mut websocket).await, "receive_offer")?;
        at_stage(validate_sdp(&offer.sdp), "offer_sdp")?;
        tracing::info!("LAN offer accepted");

        let (data_channel_tx, mut data_channel_rx) = mpsc::channel(1);
        let (gathering_tx, mut gathering_rx) = watch::channel(false);
        let handler = Arc::new(LanPeerConnectionHandler {
            data_channel_tx,
            gathering_tx,
            received_channels: AtomicUsize::new(0),
            peer_connection: Mutex::new(None),
        });
        let configuration = RTCConfigurationBuilder::default()
            .with_ice_servers(Vec::new())
            .build();
        let pc = at_stage(
            PeerConnectionBuilder::<SocketAddr>::new()
                .with_configuration(configuration)
                .with_udp_addrs(vec![SocketAddr::new(self.address, 0)])
                .with_data_channel_send_buffer_limit(DATA_CHANNEL_SEND_BUFFER_LIMIT)
                .with_dedicated_reactor_pool_size(1)
                .with_handler(handler.clone())
                .build()
                .await
                .map_err(|_| LanError::WebRtc),
            "peer_connection",
        )?;
        let pc: Arc<dyn PeerConnection> = Arc::new(pc);
        *handler
            .peer_connection
            .lock()
            .map_err(|_| LanError::WebRtc)? = Some(Arc::downgrade(&pc));
        let pc_guard = PeerConnectionGuard::new(Arc::clone(&pc));

        let remote_description =
            RTCSessionDescription::offer(offer.sdp).map_err(|_| LanError::InvalidSignaling)?;
        at_stage(
            pc.set_remote_description(remote_description)
                .await
                .map_err(|_| LanError::WebRtc),
            "set_remote_offer",
        )?;
        let answer = at_stage(
            pc.create_answer(None).await.map_err(|_| LanError::WebRtc),
            "create_answer",
        )?;
        at_stage(
            pc.set_local_description(answer)
                .await
                .map_err(|_| LanError::WebRtc),
            "set_local_answer",
        )?;

        if !*gathering_rx.borrow() {
            gathering_rx.changed().await.map_err(|_| LanError::WebRtc)?;
        }
        if !*gathering_rx.borrow() {
            return Err(LanError::WebRtc);
        }

        let local_description = pc.local_description().await.ok_or(LanError::WebRtc)?;
        let answer_sdp = at_stage(
            data_channel_answer_sdp(&local_description.sdp),
            "answer_sdp",
        )?;
        let answer_json = serde_json::to_string(&SignalingAnswer {
            version: 1,
            kind: SignalType::Answer,
            sdp: &answer_sdp,
        })
        .map_err(|_| LanError::Signaling)?;
        if answer_json.len() > MAX_SIGNALING_MESSAGE {
            return Err(LanError::InvalidSignaling);
        }
        at_stage(
            websocket
                .send(Message::Text(answer_json.into()))
                .await
                .map_err(|_| LanError::Signaling),
            "send_answer",
        )?;
        tracing::info!("LAN answer sent");

        let data_channel = at_stage(
            data_channel_rx
                .recv()
                .await
                .unwrap_or(Err(LanError::InvalidDataChannel)),
            "data_channel",
        )?;
        at_stage(wait_for_open(&data_channel).await, "data_channel_open")?;
        tracing::info!("LAN DataChannel opened");
        websocket
            .close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: "".into(),
            }))
            .await
            .map_err(|_| LanError::Signaling)?;

        Ok(WebRtcTransport::new(pc_guard.disarm(), data_channel))
    }
}

fn at_stage<T>(result: Result<T, LanError>, stage: &'static str) -> Result<T, LanError> {
    result.inspect_err(|error| {
        tracing::warn!(stage, error = %error, "LAN negotiation failed");
    })
}

struct MdnsRegistration {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Drop for MdnsRegistration {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

fn register_mdns(address: IpAddr, port: u16) -> Option<MdnsRegistration> {
    if address.is_loopback() {
        return None;
    }
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random).ok()?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let instance = format!("kryptopass-{suffix}");
    let hostname = format!("kp-{suffix}.local.");
    let properties = [("v", "1"), ("transport", "webrtc")];
    let service = ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        &hostname,
        address,
        port,
        &properties[..],
    )
    .ok()?;
    let fullname = service.get_fullname().to_owned();
    let daemon = ServiceDaemon::new().ok()?;
    daemon.register(service).ok()?;
    Some(MdnsRegistration { daemon, fullname })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignalingMessage {
    #[serde(rename = "v")]
    version: u8,
    #[serde(rename = "type")]
    kind: SignalType,
    sdp: String,
}

#[derive(Serialize)]
struct SignalingAnswer<'a> {
    #[serde(rename = "v")]
    version: u8,
    #[serde(rename = "type")]
    kind: SignalType,
    sdp: &'a str,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SignalType {
    Offer,
    Answer,
}

async fn receive_offer<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Result<SignalingMessage, LanError>
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
                if text.len() > MAX_SIGNALING_MESSAGE {
                    return Err(LanError::InvalidSignaling);
                }
                let message: SignalingMessage =
                    serde_json::from_str(text.as_str()).map_err(|_| LanError::InvalidSignaling)?;
                if message.version != 1 || message.kind != SignalType::Offer {
                    return Err(LanError::InvalidSignaling);
                }
                return Ok(message);
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

fn signaling_path(session_id: &[u8; 16]) -> String {
    let mut path = String::from("/kp/lan/v1/");
    for byte in session_id {
        use std::fmt::Write;
        let _ = write!(path, "{byte:02x}");
    }
    path
}

fn data_channel_answer_sdp(sdp: &str) -> Result<String, LanError> {
    if sdp.is_empty() || sdp.len() > MAX_SIGNALING_MESSAGE {
        return Err(LanError::InvalidSignaling);
    }
    let mut answer = String::with_capacity(sdp.len());
    for line in sdp.lines() {
        let line = line.trim_end_matches('\r');
        // webrtc 0.21 serializes both ICE components even for application media.
        // SCTP DataChannel uses only component 1; remove the local RTCP duplicate.
        if let Some(candidate) = line.strip_prefix("a=candidate:") {
            if candidate.split_ascii_whitespace().nth(1) == Some("2") {
                continue;
            }
        }
        answer.push_str(line);
        answer.push_str("\r\n");
    }
    validate_sdp(&answer)?;
    Ok(answer)
}

fn validate_sdp(sdp: &str) -> Result<(), LanError> {
    if sdp.is_empty() || sdp.len() > MAX_SIGNALING_MESSAGE {
        return Err(LanError::InvalidSignaling);
    }
    let mut candidates = 0_usize;
    let mut application_media = 0_usize;
    for line in sdp.lines() {
        let line = line.trim_end_matches('\r');
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("m=") {
            if !lower.starts_with("m=application ") {
                return Err(LanError::InvalidSignaling);
            }
            application_media += 1;
            if application_media > 1 {
                return Err(LanError::InvalidSignaling);
            }
        } else if lower.starts_with("c=") {
            validate_sdp_address_line(line, 2)?;
        } else if lower.starts_with("o=") {
            validate_sdp_address_line(line, 5)?;
        } else if lower.starts_with("a=candidate:") {
            candidates += 1;
            if candidates > MAX_CANDIDATES {
                return Err(LanError::InvalidSignaling);
            }
            validate_candidate(line)?;
        } else if lower.contains("stun:")
            || lower.contains("turn:")
            || lower.starts_with("a=ice-server:")
            || lower.starts_with("a=remote-candidates:")
        {
            return Err(LanError::InvalidSignaling);
        }
    }
    if application_media != 1 || candidates == 0 {
        return Err(LanError::InvalidSignaling);
    }
    Ok(())
}

fn validate_sdp_address_line(line: &str, address_field: usize) -> Result<(), LanError> {
    let fields: Vec<_> = line.split_ascii_whitespace().collect();
    if fields.len() <= address_field {
        return Err(LanError::InvalidSignaling);
    }
    let address: IpAddr = fields[address_field]
        .split('/')
        .next()
        .ok_or(LanError::InvalidSignaling)?
        .parse()
        .map_err(|_| LanError::InvalidSignaling)?;
    if !address.is_unspecified() && !is_supported_address(address) {
        return Err(LanError::InvalidSignaling);
    }
    Ok(())
}

fn validate_candidate(line: &str) -> Result<(), LanError> {
    if line.len() > 512 {
        return Err(LanError::InvalidSignaling);
    }
    let candidate = line
        .get("a=candidate:".len()..)
        .ok_or(LanError::InvalidSignaling)?;
    let fields: Vec<_> = candidate.split_ascii_whitespace().collect();
    if fields.len() < 8
        || fields[0].is_empty()
        || fields[1] != "1"
        || !fields[2].eq_ignore_ascii_case("udp")
        || !fields[6].eq_ignore_ascii_case("typ")
        || !fields[7].eq_ignore_ascii_case("host")
    {
        return Err(LanError::InvalidSignaling);
    }
    let address: IpAddr = fields[4].parse().map_err(|_| LanError::InvalidSignaling)?;
    let port = fields[5]
        .parse::<u16>()
        .map_err(|_| LanError::InvalidSignaling)?;
    if !is_supported_address(address) || port == 0 {
        return Err(LanError::InvalidSignaling);
    }
    if fields[8..].chunks_exact(2).any(|extension| {
        extension[0].eq_ignore_ascii_case("raddr")
            || extension[0].eq_ignore_ascii_case("rport")
            || extension[0].eq_ignore_ascii_case("tcptype")
    }) {
        return Err(LanError::InvalidSignaling);
    }
    if fields.len() > 8 && (fields.len() - 8) % 2 != 0 {
        return Err(LanError::InvalidSignaling);
    }
    Ok(())
}

struct LanPeerConnectionHandler {
    data_channel_tx: mpsc::Sender<Result<Arc<dyn DataChannel>, LanError>>,
    gathering_tx: watch::Sender<bool>,
    received_channels: AtomicUsize,
    peer_connection: Mutex<Option<Weak<dyn PeerConnection>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for LanPeerConnectionHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathering_tx.send_replace(true);
        }
    }

    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        if self.received_channels.fetch_add(1, Ordering::AcqRel) != 0 {
            let _ = self
                .data_channel_tx
                .try_send(Err(LanError::InvalidDataChannel));
            let connection = self
                .peer_connection
                .lock()
                .ok()
                .and_then(|weak| weak.as_ref().and_then(Weak::upgrade));
            if let Some(connection) = connection {
                if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                    runtime.spawn(async move {
                        let _ = channel.close().await;
                        let _ = connection.close().await;
                    });
                }
            }
            return;
        }
        if self.data_channel_tx.try_send(Ok(channel.clone())).is_err() {
            let connection = self
                .peer_connection
                .lock()
                .ok()
                .and_then(|weak| weak.as_ref().and_then(Weak::upgrade));
            if let Some(connection) = connection {
                if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                    runtime.spawn(async move {
                        let _ = channel.close().await;
                        let _ = connection.close().await;
                    });
                }
            }
        }
    }
}

async fn wait_for_open(channel: &Arc<dyn DataChannel>) -> Result<(), LanError> {
    let valid = matches!(channel.label().await, Ok(label) if label == DATA_CHANNEL_LABEL)
        && matches!(channel.ordered().await, Ok(true))
        && matches!(channel.max_packet_life_time().await, Ok(None))
        && matches!(channel.max_retransmits().await, Ok(None))
        && matches!(channel.protocol().await, Ok(protocol) if protocol.is_empty())
        && matches!(channel.negotiated().await, Ok(false));
    if !valid {
        return Err(LanError::InvalidDataChannel);
    }
    loop {
        match channel.poll().await {
            Some(webrtc::data_channel::DataChannelEvent::OnOpen) => return Ok(()),
            Some(
                webrtc::data_channel::DataChannelEvent::OnClose
                | webrtc::data_channel::DataChannelEvent::OnClosing
                | webrtc::data_channel::DataChannelEvent::OnError,
            )
            | None => return Err(LanError::InvalidDataChannel),
            Some(webrtc::data_channel::DataChannelEvent::OnMessage(_)) => {
                return Err(LanError::InvalidDataChannel);
            }
            Some(webrtc::data_channel::DataChannelEvent::OnBufferedAmountLow)
            | Some(webrtc::data_channel::DataChannelEvent::OnBufferedAmountHigh) => {}
            _ => {}
        }
    }
}

struct PeerConnectionGuard {
    connection: Arc<dyn PeerConnection>,
    armed: bool,
}

impl PeerConnectionGuard {
    fn new(connection: Arc<dyn PeerConnection>) -> Self {
        Self {
            connection,
            armed: true,
        }
    }

    fn disarm(mut self) -> Arc<dyn PeerConnection> {
        self.armed = false;
        Arc::clone(&self.connection)
    }
}

impl Drop for PeerConnectionGuard {
    fn drop(&mut self) {
        if self.armed {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                let connection = Arc::clone(&self.connection);
                runtime.spawn(async move {
                    let _ = connection.close().await;
                });
            }
        }
    }
}
