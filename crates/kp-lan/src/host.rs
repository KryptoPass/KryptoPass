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
    sync::{Mutex as AsyncMutex, mpsc, watch},
    time::{Instant, sleep_until, timeout},
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
        RTCIceCandidateType, RTCIceGatheringState, RTCIceProtocol, RTCIceServer,
        RTCIceTransportPolicy, RTCPeerConnectionIceEvent, RTCSessionDescription,
    },
};

use crate::{
    LanError,
    address::{is_supported_address, is_supported_internet_address},
    privacy::{NetworkPolicyGuard, NetworkUse},
    transport::WebRtcTransport,
};

const SERVICE_TYPE: &str = "_kryptopass._tcp.local.";
const DATA_CHANNEL_LABEL: &str = "kryptopass-v1";
const SIGNALING_TIMEOUT: Duration = Duration::from_secs(30);
// The answer carries a complete candidate snapshot (no Trickle ICE yet), so its
// gathering wait sits on the peer's connect deadline. A reachable relay usually
// allocates in well under a second; these caps only bound silent servers.
const TURN_GATHERING_DEADLINE: Duration = Duration::from_secs(5);
const STUN_GATHERING_DEADLINE: Duration = Duration::from_secs(3);
/// Short window after the first server-reflexive candidate so sibling
/// candidates from the same gathering pass join the snapshot.
const SERVER_REFLEXIVE_SETTLE: Duration = Duration::from_millis(300);
const RECONNECT_ACCEPT_INTERVAL: Duration = Duration::from_secs(2);
const RECONNECT_PATH: &str = "/kp/lan/v1/resume";
const RESUME_PATH_PREFIX: &str = "/kp/lan/v1/resume/";
const MAX_RESUME_MESSAGE: usize = 128;
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

/// Persistent LAN signaling endpoint for fresh reconnect handshakes.
///
/// Unlike [`LanHost`], this endpoint has no mDNS advertisement and accepts
/// repeated attempts at `/kp/lan/v1/resume`. Each accepted attempt gets a new
/// random signaling session ID before the peer's SDP offer is read. Signaling
/// only creates a carrier; the caller must still run a fresh authenticated
/// Noise reconnect against its persisted trust record.
pub struct ReconnectHost {
    listener: TcpListener,
    address: IpAddr,
    accept_state: AsyncMutex<ReconnectAcceptState>,
}

/// Process-owned wildcard LAN endpoint for reconnecting authorized peers.
///
/// The peer ID in the WebSocket path is an untrusted selector. The caller's
/// authorization callback runs before the server sends a resume notice or
/// creates a WebRTC peer connection. Identity authentication still belongs to
/// the caller's fresh Noise handshake.
pub struct ResumeListener {
    listener_v4: TcpListener,
    port: u16,
    bind_address: IpAddr,
    network_guard: Option<NetworkPolicyGuard>,
    accept_state: AsyncMutex<ReconnectAcceptState>,
    discovery: Mutex<ResumeDiscovery>,
}

struct ResumeDiscovery {
    _registrations: Vec<MdnsRegistration>,
    addresses: Vec<IpAddr>,
}

/// One accepted and negotiated transport selected by an initiator DeviceId.
pub struct ResumeAttempt {
    /// Untrusted initiator ID from the canonical WebSocket path. It is only a
    /// selector; the Noise handshake must authenticate this exact peer.
    pub initiator_device_id: [u8; 32],
    /// Fresh signaling session ID sent in the resume notice.
    pub session_id: [u8; 16],
    /// Newly negotiated WebRTC carrier for a fresh Noise handshake.
    pub transport: WebRtcTransport,
}

impl ResumeListener {
    /// Bind a loopback-only listener for local callers and tests.
    pub async fn bind(local_device_id: [u8; 32]) -> Result<Self, LanError> {
        Self::bind_inner(
            local_device_id,
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            None,
        )
        .await
    }

    /// Bind and advertise one explicitly selected local address after the
    /// persisted local policy allows LAN use.
    pub async fn bind_with_privacy(
        local_device_id: [u8; 32],
        bind_address: IpAddr,
        guard: NetworkPolicyGuard,
    ) -> Result<Self, LanError> {
        guard
            .preflight(NetworkUse::Lan)
            .map_err(|_| LanError::NetworkPolicy)?;
        Self::bind_inner(local_device_id, bind_address, Some(guard)).await
    }

    async fn bind_inner(
        local_device_id: [u8; 32],
        bind_address: IpAddr,
        network_guard: Option<NetworkPolicyGuard>,
    ) -> Result<Self, LanError> {
        if local_device_id.iter().all(|byte| *byte == 0) {
            return Err(LanError::InvalidSession);
        }
        if !is_supported_address(bind_address) {
            return Err(LanError::InvalidAddress);
        }
        if let Some(guard) = network_guard.as_ref() {
            guard
                .preflight(NetworkUse::Lan)
                .map_err(|_| LanError::NetworkPolicy)?;
        }
        let listener_v4 = TcpListener::bind(SocketAddr::new(bind_address, 0))
            .await
            .map_err(|_| LanError::Bind)?;
        let port = listener_v4.local_addr().map_err(|_| LanError::Bind)?.port();
        let (registrations, addresses) = register_resume_mdns(local_device_id, port, bind_address);
        tracing::info!(
            port,
            mdns_services = addresses.len(),
            selected_family = if bind_address.is_ipv4() {
                "ipv4"
            } else {
                "ipv6"
            },
            "LAN resume listener ready"
        );
        Ok(Self {
            listener_v4,
            port,
            bind_address,
            network_guard,
            accept_state: AsyncMutex::new(ReconnectAcceptState {
                last_accepted_at: None,
            }),
            discovery: Mutex::new(ResumeDiscovery {
                _registrations: registrations,
                addresses,
            }),
        })
    }

    /// Return the ephemeral TCP port on the explicitly selected address.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Refresh best-effort mDNS advertisements after network interfaces change.
    pub fn refresh_mdns(&self, local_device_id: [u8; 32]) {
        let permitted = self.network_guard.as_ref().is_none_or(|guard| {
            guard.preflight(NetworkUse::Lan).is_ok() && guard.ensure_current().is_ok()
        });
        let addresses = if permitted && !self.bind_address.is_loopback() {
            vec![self.bind_address]
        } else {
            Vec::new()
        };
        let Ok(mut discovery) = self.discovery.lock() else {
            tracing::warn!("LAN resume discovery state is unavailable");
            return;
        };
        if addresses != discovery.addresses {
            if addresses.is_empty() {
                *discovery = ResumeDiscovery {
                    _registrations: Vec::new(),
                    addresses,
                };
            } else {
                let (registrations, registered_addresses) =
                    register_resume_mdns(local_device_id, self.port, self.bind_address);
                // Replace the prior advertisements only after the new set has
                // been registered successfully, preserving discovery on errors.
                if !registered_addresses.is_empty() {
                    *discovery = ResumeDiscovery {
                        _registrations: registrations,
                        addresses: registered_addresses,
                    };
                }
            }
            tracing::info!(
                mdns_services = discovery.addresses.len(),
                "LAN resume discovery refreshed"
            );
        }
    }

    /// Accept one bounded attempt after authorizing its DeviceId selector.
    ///
    /// The callback runs after the exact path and no-query rule are validated,
    /// and before WebRTC or the resume notice is created. Dropping this future
    /// cancels the in-progress socket/negotiation; the listener itself remains
    /// available for later attempts.
    pub async fn accept_authorized<F>(&self, authorize: F) -> Result<ResumeAttempt, LanError>
    where
        F: Fn([u8; 32]) -> bool + Send + Sync + 'static,
    {
        let Some(guard) = self.network_guard.as_ref().cloned() else {
            return self.accept_authorized_inner(authorize).await;
        };
        guard
            .preflight(NetworkUse::Lan)
            .map_err(|_| LanError::NetworkPolicy)?;
        let mut attempt = tokio::select! {
            result = self.accept_authorized_inner(authorize) => result?,
            _ = guard.wait_for_change(Duration::from_millis(250)) => {
                return Err(LanError::NetworkPolicy);
            }
        };
        attempt
            .transport
            .enforce_network_policy(guard)
            .map_err(|_| LanError::NetworkPolicy)?;
        Ok(attempt)
    }

    async fn accept_authorized_inner<F>(&self, authorize: F) -> Result<ResumeAttempt, LanError>
    where
        F: Fn([u8; 32]) -> bool + Send + Sync + 'static,
    {
        let mut state = self.accept_state.lock().await;
        if let Some(last_accepted_at) = state.last_accepted_at {
            sleep_until(last_accepted_at + RECONNECT_ACCEPT_INTERVAL).await;
        }
        let accepted = self.listener_v4.accept().await;
        let (socket, remote) = accepted.map_err(|_| LanError::Signaling)?;
        state.last_accepted_at = Some(Instant::now());
        drop(state);
        tracing::info!(
            remote_family = if remote.is_ipv4() { "ipv4" } else { "ipv6" },
            "LAN resume connection accepted"
        );

        let local = socket.local_addr().map_err(|_| LanError::Signaling)?;
        if !is_supported_address(local.ip())
            || !is_supported_address(remote.ip())
            || local.is_ipv4() != remote.is_ipv4()
        {
            return Err(LanError::InvalidSignaling);
        }
        let endpoint = SignalingEndpoint::DurableResume(Arc::new(authorize));
        let (initiator_device_id, session_id, transport) = timeout(
            SIGNALING_TIMEOUT,
            negotiate_socket(socket, remote, local.ip(), endpoint),
        )
        .await
        .map_err(|_| {
            tracing::warn!(stage = "negotiation_deadline", "LAN negotiation failed");
            LanError::Timeout
        })??;
        let (Some(initiator_device_id), Some(session_id)) = (initiator_device_id, session_id)
        else {
            return Err(LanError::InvalidSignaling);
        };
        Ok(ResumeAttempt {
            initiator_device_id,
            session_id,
            transport,
        })
    }
}

struct ReconnectAcceptState {
    last_accepted_at: Option<Instant>,
}

impl ReconnectHost {
    /// Bind a persistent resume listener to an explicitly selected LAN address.
    ///
    /// The caller announces the returned ephemeral port only inside the
    /// authenticated session hello. This host does not advertise a pairing
    /// service through mDNS.
    pub async fn bind(address: IpAddr) -> Result<Self, LanError> {
        if !is_supported_address(address) {
            return Err(LanError::InvalidAddress);
        }
        let listener = TcpListener::bind(SocketAddr::new(address, 0))
            .await
            .map_err(|_| LanError::Bind)?;
        tracing::info!("LAN reconnect listener ready");
        Ok(Self {
            listener,
            address,
            accept_state: AsyncMutex::new(ReconnectAcceptState {
                last_accepted_at: None,
            }),
        })
    }

    /// Return the ephemeral TCP port used for reconnect signaling.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.listener
            .local_addr()
            .map(|address| address.port())
            .unwrap_or_default()
    }

    /// Accept one reconnect attempt while retaining the listener for later attempts.
    ///
    /// Only one negotiation runs at a time. Accepted TCP attempts are spaced by
    /// at least two seconds; a 30-second deadline then covers WebSocket
    /// signaling, ICE gathering, and DataChannel opening.
    pub async fn accept(&self) -> Result<([u8; 16], WebRtcTransport), LanError> {
        let mut state = self.accept_state.lock().await;
        if let Some(last_accepted_at) = state.last_accepted_at {
            sleep_until(last_accepted_at + RECONNECT_ACCEPT_INTERVAL).await;
        }
        let (socket, remote) = self
            .listener
            .accept()
            .await
            .map_err(|_| LanError::Signaling)?;
        state.last_accepted_at = Some(Instant::now());
        let negotiation =
            negotiate_socket(socket, remote, self.address, SignalingEndpoint::Reconnect);
        match timeout(SIGNALING_TIMEOUT, negotiation).await {
            Ok(Ok((_, Some(session_id), transport))) => Ok((session_id, transport)),
            Ok(Ok((_, None, _))) => Err(LanError::InvalidSession),
            Ok(Err(error)) => Err(error),
            Err(_) => at_stage(Err(LanError::Timeout), "negotiation_deadline"),
        }
    }
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
        &self,
        socket: tokio::net::TcpStream,
        remote: SocketAddr,
    ) -> Result<WebRtcTransport, LanError> {
        negotiate_socket(
            socket,
            remote,
            self.address,
            SignalingEndpoint::Pairing(self.session_id),
        )
        .await
        .map(|(_, _, transport)| transport)
    }
}

#[derive(Clone)]
enum SignalingEndpoint {
    Pairing([u8; 16]),
    Reconnect,
    DurableResume(Arc<dyn Fn([u8; 32]) -> bool + Send + Sync>),
}

async fn negotiate_socket(
    socket: tokio::net::TcpStream,
    remote: SocketAddr,
    address: IpAddr,
    endpoint: SignalingEndpoint,
) -> Result<(Option<[u8; 32]>, Option<[u8; 16]>, WebRtcTransport), LanError> {
    let authorized_device = Arc::new(Mutex::new(None));
    let selected_device = Arc::clone(&authorized_device);
    let endpoint_for_upgrade = endpoint.clone();
    if !is_supported_address(remote.ip()) || remote.is_ipv4() != address.is_ipv4() {
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
                let valid = match &endpoint_for_upgrade {
                    SignalingEndpoint::Pairing(session_id) => {
                        request.uri().path() == signaling_path(session_id)
                            && request.uri().query().is_none()
                    }
                    SignalingEndpoint::Reconnect => {
                        request.uri().path() == RECONNECT_PATH && request.uri().query().is_none()
                    }
                    SignalingEndpoint::DurableResume(authorize) => {
                        if request.uri().query().is_some() {
                            false
                        } else if let Some(device_id) = parse_resume_path(request.uri().path()) {
                            if authorize(device_id) {
                                if let Ok(mut selected) = selected_device.lock() {
                                    *selected = Some(device_id);
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                };
                if valid {
                    Ok(response)
                } else {
                    let mut response = tokio_tungstenite::tungstenite::http::Response::new(Some(
                        "invalid signaling path".to_owned(),
                    ));
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

    let is_resume = matches!(
        endpoint,
        SignalingEndpoint::Reconnect | SignalingEndpoint::DurableResume(_)
    );
    let reconnect_session = if is_resume {
        let session_id = fresh_session_id()?;
        let session_hex = hex(&session_id);
        let resume = serde_json::to_string(&ResumeNotice {
            version: 1,
            kind: SignalType::Resume,
            session: &session_hex,
        })
        .map_err(|_| LanError::Signaling)?;
        if resume.len() > MAX_RESUME_MESSAGE {
            return Err(LanError::InvalidSignaling);
        }
        at_stage(
            websocket
                .send(Message::Text(resume.into()))
                .await
                .map_err(|_| LanError::Signaling),
            "send_resume",
        )?;
        Some(session_id)
    } else {
        None
    };
    let initiator_device_id = authorized_device
        .lock()
        .map_err(|_| LanError::InvalidSignaling)?
        .take();

    let transport = negotiate_offer_answer(
        &mut websocket,
        vec![SocketAddr::new(address, 0)],
        Vec::new(),
        CandidatePolicy::Lan,
        // Preserve the builder's prior default behavior for LAN peers.
        RTCIceTransportPolicy::Unspecified,
    )
    .await?;
    websocket
        .close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "".into(),
        }))
        .await
        .map_err(|_| LanError::Signaling)?;

    Ok((initiator_device_id, reconnect_session, transport))
}

#[derive(Clone, Copy)]
pub(crate) enum CandidatePolicy {
    Lan,
    Internet { allow_loopback: bool },
    InternetRelayOnly { allow_loopback: bool },
}

pub(crate) async fn negotiate_offer_answer<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
    udp_addresses: Vec<SocketAddr>,
    ice_servers: Vec<RTCIceServer>,
    policy: CandidatePolicy,
    ice_policy: RTCIceTransportPolicy,
) -> Result<WebRtcTransport, LanError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let offer = at_stage(receive_offer(websocket).await, "receive_offer")?;
    // Relay-only is our local publication policy. It must not reject a peer's
    // host candidate: a relay-local/host-remote path is valid and does not
    // reveal this device's address. Validate the peer under ordinary Internet
    // signaling rules, then filter only the local SDP sent below.
    let remote_policy = remote_offer_policy(policy);
    at_stage(validate_sdp(&offer.sdp, remote_policy), "offer_sdp")?;
    let offer_started = Instant::now();
    let offer_candidates = CandidateTypeCounts::from_sdp(&offer.sdp);
    tracing::info!(
        host = offer_candidates.host,
        srflx = offer_candidates.srflx,
        prflx = offer_candidates.prflx,
        relay = offer_candidates.relay,
        "WebRTC offer accepted"
    );

    let gathering_plan = GatheringPlan::for_servers(&ice_servers);
    let (data_channel_tx, mut data_channel_rx) = mpsc::channel(1);
    let (gathering_tx, mut gathering_rx) = watch::channel(false);
    let (relay_candidate_tx, mut relay_candidate_rx) = watch::channel(false);
    let (srflx_candidate_tx, mut srflx_candidate_rx) = watch::channel(false);
    let handler = Arc::new(LanPeerConnectionHandler {
        data_channel_tx,
        gathering_tx,
        relay_candidate_tx,
        srflx_candidate_tx,
        candidate_policy: policy,
        received_channels: AtomicUsize::new(0),
        peer_connection: Mutex::new(None),
    });
    let configuration = RTCConfigurationBuilder::default()
        .with_ice_servers(ice_servers)
        .with_ice_transport_policy(ice_policy)
        .build();
    let pc = at_stage(
        PeerConnectionBuilder::<SocketAddr>::new()
            .with_configuration(configuration)
            .with_udp_addrs(udp_addresses)
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

    let gather_reason = match gathering_plan {
        GatheringPlan::Relay => {
            wait_for_turn_gathering(&mut gathering_rx, &mut relay_candidate_rx).await
        }
        GatheringPlan::ServerReflexive => {
            wait_for_stun_gathering(&mut gathering_rx, &mut srflx_candidate_rx).await
        }
        GatheringPlan::HostOnly => {
            if !*gathering_rx.borrow() {
                gathering_rx.changed().await.map_err(|_| LanError::WebRtc)?;
            }
            if !*gathering_rx.borrow() {
                return Err(LanError::WebRtc);
            }
            GatheringStopReason::Complete
        }
    };

    let local_description = pc.local_description().await.ok_or(LanError::WebRtc)?;
    let local_candidates = CandidateTypeCounts::from_sdp(&local_description.sdp);
    tracing::info!(
        reason = gather_reason.as_str(),
        elapsed_ms = elapsed_ms(offer_started),
        host = local_candidates.host,
        srflx = local_candidates.srflx,
        prflx = local_candidates.prflx,
        relay = local_candidates.relay,
        "ICE gathering snapshot selected"
    );
    if ice_policy == RTCIceTransportPolicy::Relay {
        if local_candidates.relay == 0 {
            return at_stage(Err(LanError::WebRtc), "relay_candidate_missing");
        }
        if local_candidates.relay != local_candidates.total() {
            return at_stage(Err(LanError::WebRtc), "relay_candidate_policy");
        }
    }
    let answer_sdp = at_stage(
        data_channel_answer_sdp(&local_description.sdp, policy),
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
    tracing::info!(elapsed_ms = elapsed_ms(offer_started), "WebRTC answer sent");

    let data_channel = at_stage(
        data_channel_rx
            .recv()
            .await
            .unwrap_or(Err(LanError::InvalidDataChannel)),
        "data_channel",
    )?;
    at_stage(wait_for_open(&data_channel).await, "data_channel_open")?;
    tracing::info!(
        elapsed_ms = elapsed_ms(offer_started),
        "WebRTC DataChannel opened"
    );
    Ok(WebRtcTransport::new(pc_guard.disarm(), data_channel))
}

/// Apply only the local relay-only candidate policy to outgoing SDP. A local
/// relay candidate can validly connect to a remote host or server-reflexive
/// candidate, so remote SDP always uses ordinary Internet validation.
fn remote_offer_policy(policy: CandidatePolicy) -> CandidatePolicy {
    match policy {
        CandidatePolicy::InternetRelayOnly { allow_loopback } => {
            CandidatePolicy::Internet { allow_loopback }
        }
        policy => policy,
    }
}

fn at_stage<T>(result: Result<T, LanError>, stage: &'static str) -> Result<T, LanError> {
    result.inspect_err(|error| {
        tracing::warn!(stage, error = %error, "WebRTC negotiation failed");
    })
}

struct MdnsRegistration {
    daemon: ServiceDaemon,
    fullnames: Vec<String>,
}

impl Drop for MdnsRegistration {
    fn drop(&mut self) {
        for fullname in &self.fullnames {
            let _ = self.daemon.unregister(fullname);
        }
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
    Some(MdnsRegistration {
        daemon,
        fullnames: vec![fullname],
    })
}

fn register_resume_mdns(
    local_device_id: [u8; 32],
    port: u16,
    address: IpAddr,
) -> (Vec<MdnsRegistration>, Vec<IpAddr>) {
    if address.is_loopback() {
        return (Vec::new(), Vec::new());
    }
    let Some(daemon) = ServiceDaemon::new().ok() else {
        return (Vec::new(), Vec::new());
    };
    let device = hex_bytes(&local_device_id);
    let mut random = [0_u8; 8];
    if getrandom::fill(&mut random).is_err() {
        let _ = daemon.shutdown();
        return (Vec::new(), Vec::new());
    }
    let suffix = hex_bytes(&random);
    let instance = format!("kryptopass-resume-{suffix}");
    let hostname = format!("kp-resume-{suffix}.local.");
    let properties = [
        ("v", "1"),
        ("transport", "webrtc"),
        ("mode", "resume"),
        ("device", device.as_str()),
    ];
    let Ok(service) = ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        &hostname,
        address,
        port,
        &properties[..],
    ) else {
        let _ = daemon.shutdown();
        return (Vec::new(), Vec::new());
    };
    let fullname = service.get_fullname().to_owned();
    if daemon.register(service).is_err() {
        let _ = daemon.shutdown();
        return (Vec::new(), Vec::new());
    }
    (
        vec![MdnsRegistration {
            daemon,
            fullnames: vec![fullname],
        }],
        vec![address],
    )
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(result, "{byte:02x}");
    }
    result
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

#[derive(Serialize)]
struct ResumeNotice<'a> {
    #[serde(rename = "v")]
    version: u8,
    #[serde(rename = "type")]
    kind: SignalType,
    session: &'a str,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SignalType {
    Resume,
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

fn parse_resume_path(path: &str) -> Option<[u8; 32]> {
    let suffix = path.strip_prefix(RESUME_PATH_PREFIX)?;
    if suffix.len() != 64
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut device_id = [0_u8; 32];
    for (index, byte) in device_id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&suffix[index * 2..index * 2 + 2], 16).ok()?;
    }
    device_id.iter().any(|byte| *byte != 0).then_some(device_id)
}

fn fresh_session_id() -> Result<[u8; 16], LanError> {
    for _ in 0..2 {
        let mut session_id = [0_u8; 16];
        getrandom::fill(&mut session_id).map_err(|_| LanError::Signaling)?;
        if session_id.iter().any(|byte| *byte != 0) {
            return Ok(session_id);
        }
    }
    Err(LanError::InvalidSession)
}

fn hex(session_id: &[u8; 16]) -> String {
    use std::fmt::Write;

    let mut result = String::with_capacity(32);
    for byte in session_id {
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn data_channel_answer_sdp(sdp: &str, policy: CandidatePolicy) -> Result<String, LanError> {
    if sdp.is_empty() || sdp.len() > MAX_SIGNALING_MESSAGE {
        return Err(LanError::InvalidSignaling);
    }
    let sanitized = if is_relay_only(policy) {
        sanitize_relay_only_sdp(sdp)?
    } else {
        sdp.to_owned()
    };
    let mut answer = String::with_capacity(sdp.len());
    for line in sanitized.lines() {
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
    validate_sdp(&answer, policy)?;
    Ok(answer)
}

fn validate_sdp(sdp: &str, policy: CandidatePolicy) -> Result<(), LanError> {
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
            validate_sdp_address_line(line, 2, policy)?;
        } else if lower.starts_with("o=") {
            validate_sdp_address_line(line, 5, policy)?;
        } else if lower.starts_with("a=candidate:") {
            candidates += 1;
            if candidates > MAX_CANDIDATES {
                return Err(LanError::InvalidSignaling);
            }
            validate_candidate(line, policy)?;
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

fn validate_sdp_address_line(
    line: &str,
    address_field: usize,
    policy: CandidatePolicy,
) -> Result<(), LanError> {
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
    // Native WebRTC uses loopback in o= as session metadata. It is not an ICE
    // destination; c= and every candidate still use the transport address policy.
    let origin_placeholder = address_field == 5 && address.is_loopback();
    if is_relay_only(policy) && !address.is_unspecified() {
        return Err(LanError::InvalidSignaling);
    }
    if !address.is_unspecified() && !origin_placeholder && !address_allowed(address, policy) {
        return Err(LanError::InvalidSignaling);
    }
    Ok(())
}

fn validate_candidate(line: &str, policy: CandidatePolicy) -> Result<(), LanError> {
    match policy {
        CandidatePolicy::Lan => validate_lan_candidate(line),
        CandidatePolicy::Internet { allow_loopback } => {
            validate_internet_candidate(line, allow_loopback, false)
        }
        CandidatePolicy::InternetRelayOnly { allow_loopback } => {
            validate_internet_candidate(line, allow_loopback, true)
        }
    }
}

fn validate_lan_candidate(line: &str) -> Result<(), LanError> {
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

fn validate_internet_candidate(
    line: &str,
    allow_loopback: bool,
    relay_only: bool,
) -> Result<(), LanError> {
    if line.len() > 512 {
        return Err(LanError::InvalidSignaling);
    }
    let candidate = line
        .get("a=candidate:".len()..)
        .ok_or(LanError::InvalidSignaling)?;
    let fields: Vec<_> = candidate.split_ascii_whitespace().collect();
    if fields.len() < 8
        || fields[0].is_empty()
        || fields[0].len() > 32
        || !fields[0]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
        || fields[1] != "1"
        || !fields[2].eq_ignore_ascii_case("udp")
        || fields[3].parse::<u32>().is_err()
        || !fields[6].eq_ignore_ascii_case("typ")
    {
        return Err(LanError::InvalidSignaling);
    }
    let candidate_type = fields[7].to_ascii_lowercase();
    if !matches!(
        candidate_type.as_str(),
        "host" | "srflx" | "prflx" | "relay"
    ) {
        return Err(LanError::InvalidSignaling);
    }
    if relay_only && candidate_type != "relay" {
        return Err(LanError::InvalidSignaling);
    }
    let address: IpAddr = fields[4].parse().map_err(|_| LanError::InvalidSignaling)?;
    let port = fields[5]
        .parse::<u16>()
        .map_err(|_| LanError::InvalidSignaling)?;
    let candidate_policy = if relay_only {
        CandidatePolicy::InternetRelayOnly { allow_loopback }
    } else {
        CandidatePolicy::Internet { allow_loopback }
    };
    if !address_allowed(address, candidate_policy) || port == 0 {
        return Err(LanError::InvalidSignaling);
    }
    if fields.len() > 8 && (fields.len() - 8) % 2 != 0 {
        return Err(LanError::InvalidSignaling);
    }

    let mut related_address = None;
    let mut related_port = None;
    let mut seen = std::collections::HashSet::new();
    for extension in fields[8..].chunks_exact(2) {
        let name = extension[0].to_ascii_lowercase();
        let value = extension[1];
        if name.is_empty()
            || name.len() > 32
            || value.is_empty()
            || value.len() > 128
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || !value.bytes().all(|byte| byte.is_ascii_graphic())
            || !seen.insert(name.clone())
        {
            return Err(LanError::InvalidSignaling);
        }
        match name.as_str() {
            "raddr" => {
                let related: IpAddr = value.parse().map_err(|_| LanError::InvalidSignaling)?;
                related_address = Some(related);
            }
            "rport" => {
                related_port = Some(
                    value
                        .parse::<u16>()
                        .map_err(|_| LanError::InvalidSignaling)?,
                );
            }
            "generation" | "network-id" | "network-cost" => {
                if value.parse::<u32>().is_err() {
                    return Err(LanError::InvalidSignaling);
                }
            }
            "ufrag" => {}
            "tcptype" => return Err(LanError::InvalidSignaling),
            _ => {}
        }
    }
    match (related_address, related_port) {
        (None, None) => {}
        (Some(related), Some(related_port)) => {
            // RFC 8839 permits redacting related-address metadata as an unspecified
            // address with port 9. Existing native SDKs also emit port 0. Neither
            // placeholder is the candidate destination, which stays strict above.
            let redacted = candidate_type != "host"
                && related.is_unspecified()
                && matches!(related_port, 0 | 9);
            if !redacted && (related_port == 0 || !address_allowed(related, candidate_policy)) {
                return Err(LanError::InvalidSignaling);
            }
        }
        _ => return Err(LanError::InvalidSignaling),
    }
    Ok(())
}

fn address_allowed(address: IpAddr, policy: CandidatePolicy) -> bool {
    match policy {
        CandidatePolicy::Lan => is_supported_address(address),
        CandidatePolicy::Internet { allow_loopback }
        | CandidatePolicy::InternetRelayOnly { allow_loopback } => {
            is_supported_internet_address(address) || (allow_loopback && address.is_loopback())
        }
    }
}

fn is_relay_only(policy: CandidatePolicy) -> bool {
    matches!(policy, CandidatePolicy::InternetRelayOnly { .. })
}

fn sanitize_relay_only_sdp(sdp: &str) -> Result<String, LanError> {
    if sdp.is_empty() || sdp.len() > MAX_SIGNALING_MESSAGE {
        return Err(LanError::InvalidSignaling);
    }
    let mut sanitized = String::with_capacity(sdp.len());
    for line in sdp.lines() {
        let line = line.trim_end_matches('\r');
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("o=") {
            sanitized.push_str(&sanitize_sdp_address_line(line, 5)?);
        } else if lower.starts_with("c=") {
            sanitized.push_str(&sanitize_sdp_address_line(line, 2)?);
        } else if lower.starts_with("a=candidate:") {
            let fields: Vec<_> = line.split_ascii_whitespace().collect();
            if fields
                .get(7)
                .is_some_and(|kind| kind.eq_ignore_ascii_case("relay"))
            {
                sanitized.push_str(&sanitize_relay_candidate(line)?);
            } else {
                continue;
            }
        } else {
            sanitized.push_str(line);
        }
        sanitized.push_str("\r\n");
    }
    validate_sdp(
        &sanitized,
        CandidatePolicy::InternetRelayOnly {
            allow_loopback: true,
        },
    )?;
    Ok(sanitized)
}

fn sanitize_sdp_address_line(line: &str, address_field: usize) -> Result<String, LanError> {
    let mut fields: Vec<_> = line.split_ascii_whitespace().map(str::to_owned).collect();
    if fields.len() <= address_field || address_field == 0 {
        return Err(LanError::InvalidSignaling);
    }
    fields[address_field - 1] = "IP4".to_owned();
    fields[address_field] = "0.0.0.0".to_owned();
    Ok(fields.join(" "))
}

fn sanitize_relay_candidate(line: &str) -> Result<String, LanError> {
    let candidate = line
        .strip_prefix("a=candidate:")
        .ok_or(LanError::InvalidSignaling)?;
    let fields: Vec<_> = candidate.split_ascii_whitespace().collect();
    if fields.len() < 8 || !fields[7].eq_ignore_ascii_case("relay") {
        return Err(LanError::InvalidSignaling);
    }
    if (fields.len() - 8) % 2 != 0 {
        return Err(LanError::InvalidSignaling);
    }
    let mut redacted = fields[..8]
        .iter()
        .map(|field| (*field).to_owned())
        .collect::<Vec<_>>();
    for extension in fields[8..].chunks_exact(2) {
        let name = extension[0].to_ascii_lowercase();
        let value = match name.as_str() {
            "raddr" => "0.0.0.0",
            "rport" => "9",
            _ => extension[1],
        };
        redacted.push(extension[0].to_owned());
        redacted.push(value.to_owned());
    }
    Ok(format!("a=candidate:{}", redacted.join(" ")))
}

struct LanPeerConnectionHandler {
    data_channel_tx: mpsc::Sender<Result<Arc<dyn DataChannel>, LanError>>,
    gathering_tx: watch::Sender<bool>,
    relay_candidate_tx: watch::Sender<bool>,
    srflx_candidate_tx: watch::Sender<bool>,
    candidate_policy: CandidatePolicy,
    received_channels: AtomicUsize,
    peer_connection: Mutex<Option<Weak<dyn PeerConnection>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for LanPeerConnectionHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if is_usable_candidate(&event, self.candidate_policy, RTCIceCandidateType::Relay) {
            self.relay_candidate_tx.send_replace(true);
        } else if is_usable_candidate(&event, self.candidate_policy, RTCIceCandidateType::Srflx) {
            self.srflx_candidate_tx.send_replace(true);
        }
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GatheringStopReason {
    Complete,
    RelayReady,
    ServerReflexiveReady,
    Deadline,
}

impl GatheringStopReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::RelayReady => "relay_ready",
            Self::ServerReflexiveReady => "srflx_ready",
            Self::Deadline => "deadline",
        }
    }
}

async fn wait_for_turn_gathering(
    gathering_rx: &mut watch::Receiver<bool>,
    relay_candidate_rx: &mut watch::Receiver<bool>,
) -> GatheringStopReason {
    let deadline = tokio::time::sleep(TURN_GATHERING_DEADLINE);
    tokio::pin!(deadline);
    loop {
        if *gathering_rx.borrow() {
            return GatheringStopReason::Complete;
        }
        if *relay_candidate_rx.borrow() {
            return GatheringStopReason::RelayReady;
        }
        tokio::select! {
            result = gathering_rx.changed() => {
                if result.is_err() {
                    return GatheringStopReason::Deadline;
                }
            }
            result = relay_candidate_rx.changed() => {
                if result.is_err() {
                    return GatheringStopReason::Deadline;
                }
            }
            () = &mut deadline => return GatheringStopReason::Deadline,
        }
    }
}

/// Without TURN, a server-reflexive candidate is the useful Internet path, so
/// the snapshot is taken shortly after the first one instead of waiting for
/// COMPLETE behind STUN retransmissions to servers that never answer.
async fn wait_for_stun_gathering(
    gathering_rx: &mut watch::Receiver<bool>,
    srflx_candidate_rx: &mut watch::Receiver<bool>,
) -> GatheringStopReason {
    let deadline = tokio::time::sleep(STUN_GATHERING_DEADLINE);
    tokio::pin!(deadline);
    loop {
        if *gathering_rx.borrow() {
            return GatheringStopReason::Complete;
        }
        if *srflx_candidate_rx.borrow() {
            break;
        }
        tokio::select! {
            result = gathering_rx.changed() => {
                if result.is_err() {
                    return GatheringStopReason::Deadline;
                }
            }
            result = srflx_candidate_rx.changed() => {
                if result.is_err() {
                    return GatheringStopReason::Deadline;
                }
            }
            () = &mut deadline => return GatheringStopReason::Deadline,
        }
    }

    let settle = tokio::time::sleep(SERVER_REFLEXIVE_SETTLE);
    tokio::pin!(settle);
    loop {
        if *gathering_rx.borrow() {
            return GatheringStopReason::Complete;
        }
        tokio::select! {
            result = gathering_rx.changed() => {
                if result.is_err() {
                    return GatheringStopReason::ServerReflexiveReady;
                }
            }
            () = &mut settle => return GatheringStopReason::ServerReflexiveReady,
            () = &mut deadline => return GatheringStopReason::ServerReflexiveReady,
        }
    }
}

/// Which candidate the answer snapshot waits for, derived from the ICE servers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GatheringPlan {
    /// No servers (LAN): host gathering completes almost immediately.
    HostOnly,
    ServerReflexive,
    Relay,
}

impl GatheringPlan {
    fn for_servers(ice_servers: &[RTCIceServer]) -> Self {
        if has_server_scheme(ice_servers, &["turn", "turns"]) {
            Self::Relay
        } else if has_server_scheme(ice_servers, &["stun", "stuns"]) {
            Self::ServerReflexive
        } else {
            Self::HostOnly
        }
    }
}

fn has_server_scheme(ice_servers: &[RTCIceServer], schemes: &[&str]) -> bool {
    ice_servers.iter().any(|server| {
        server.urls.iter().any(|url| {
            url.split_once(':').is_some_and(|(scheme, _)| {
                schemes
                    .iter()
                    .any(|expected| scheme.eq_ignore_ascii_case(expected))
            })
        })
    })
}

fn is_usable_candidate(
    event: &RTCPeerConnectionIceEvent,
    policy: CandidatePolicy,
    typ: RTCIceCandidateType,
) -> bool {
    let candidate = &event.candidate;
    if candidate.typ != typ || candidate.component != 1 || candidate.protocol != RTCIceProtocol::Udp
    {
        return false;
    }
    let Ok(candidate) = candidate.to_json() else {
        return false;
    };
    let line = format!("a={}", candidate.candidate);
    validate_candidate(&line, policy).is_ok()
}

/// Component-one candidate counts by type. Diagnostics carry only these
/// counts, never addresses, ports or SDP.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CandidateTypeCounts {
    host: usize,
    srflx: usize,
    prflx: usize,
    relay: usize,
    other: usize,
}

impl CandidateTypeCounts {
    fn from_sdp(sdp: &str) -> Self {
        sdp.lines()
            .filter_map(|line| line.trim_end_matches('\r').strip_prefix("a=candidate:"))
            .filter(|candidate| candidate.split_ascii_whitespace().nth(1) != Some("2"))
            .fold(Self::default(), |mut counts, candidate| {
                match candidate.split_ascii_whitespace().nth(7) {
                    Some(typ) if typ.eq_ignore_ascii_case("host") => counts.host += 1,
                    Some(typ) if typ.eq_ignore_ascii_case("srflx") => counts.srflx += 1,
                    Some(typ) if typ.eq_ignore_ascii_case("prflx") => counts.prflx += 1,
                    Some(typ) if typ.eq_ignore_ascii_case("relay") => counts.relay += 1,
                    _ => counts.other += 1,
                }
                counts
            })
    }

    const fn total(self) -> usize {
        self.host + self.srflx + self.prflx + self.relay + self.other
    }
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
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

#[cfg(test)]
#[path = "internet_sdp_tests.rs"]
mod internet_sdp_tests;

#[cfg(test)]
#[path = "turn_gathering_tests.rs"]
mod gathering_tests;
