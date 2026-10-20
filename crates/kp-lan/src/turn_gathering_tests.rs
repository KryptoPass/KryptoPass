use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use kryptopass_protocol::Channel;
use kryptopass_transport::PeerTransport;
use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::timeout,
};
use tokio_tungstenite::{WebSocketStream, accept_async, connect_async, tungstenite::Message};
use webrtc::{
    data_channel::{DataChannel, DataChannelEvent, RTCDataChannelState},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
        RTCIceCandidateType, RTCIceGatheringState, RTCIceServer, RTCIceTransportPolicy,
        RTCPeerConnectionIceEvent, RTCSessionDescription,
    },
};

use super::{CandidatePolicy, DATA_CHANNEL_LABEL, LanError, negotiate_offer_answer};
use crate::transport::WebRtcTransport;

const CASE_DEADLINE: Duration = Duration::from_secs(25);
const OUTER_DEADLINE: Duration = Duration::from_secs(27);
const ALLOCATE_DEADLINE: Duration = Duration::from_secs(5);
const LOCAL_GATHER_DEADLINE: Duration = Duration::from_secs(5);
const SYNTHETIC_REMOTE_DATA: &[u8] = &[0x00, 0x91, 0xff, b'A', 0x03];
const SYNTHETIC_LOCAL_DATA: &[u8] = &[0x7f, 0x00, 0xfe, b'B', 0x80];

#[derive(Clone, Copy)]
enum Scenario {
    All,
    Relay,
}

struct GatheringHandler {
    host_candidate_tx: mpsc::UnboundedSender<()>,
    complete_tx: mpsc::UnboundedSender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for GatheringHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if event.candidate.typ == RTCIceCandidateType::Host {
            let _ = self.host_candidate_tx.send(());
        }
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.complete_tx.send(());
        }
    }
}

#[derive(Serialize)]
struct OfferMessage<'a> {
    #[serde(rename = "v")]
    version: u8,
    #[serde(rename = "type")]
    kind: &'static str,
    sdp: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnswerMessage {
    #[serde(rename = "v")]
    version: u8,
    #[serde(rename = "type")]
    kind: String,
    sdp: String,
}

struct SilentTurn {
    url: String,
    allocate_rx: oneshot::Receiver<()>,
    observer: JoinHandle<()>,
}

impl Drop for SilentTurn {
    fn drop(&mut self) {
        self.observer.abort();
    }
}

impl SilentTurn {
    async fn bind() -> Result<Self, &'static str> {
        let socket = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .map_err(|_| "could not bind the local silent TURN socket")?;
        let address = socket
            .local_addr()
            .map_err(|_| "could not inspect the local silent TURN socket")?;
        let (allocate_tx, allocate_rx) = oneshot::channel();
        let observer = tokio::spawn(async move {
            let mut datagram = [0_u8; 2048];
            let mut allocate_tx = Some(allocate_tx);
            loop {
                let Ok((size, _)) = socket.recv_from(&mut datagram).await else {
                    return;
                };
                if allocate_tx.is_some() && is_allocate_request(&datagram[..size]) {
                    if let Some(allocate_tx) = allocate_tx.take() {
                        let _ = allocate_tx.send(());
                    }
                }
            }
        });
        Ok(Self {
            url: format!("turn:{address}?transport=udp"),
            allocate_rx,
            observer,
        })
    }

    async fn wait_for_allocate(&mut self) -> Result<(), &'static str> {
        timeout(ALLOCATE_DEADLINE, &mut self.allocate_rx)
            .await
            .map_err(|_| "TURN Allocate did not reach the local silent socket")?
            .map_err(|_| "local TURN Allocate observer stopped")
    }
}

fn is_allocate_request(packet: &[u8]) -> bool {
    if packet.len() < 20 || packet[0] & 0xc0 != 0 {
        return false;
    }
    let message_type = u16::from_be_bytes([packet[0], packet[1]]);
    let magic_cookie = u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);
    let method =
        (message_type & 0x000f) | ((message_type & 0x00e0) >> 1) | ((message_type & 0x3e00) >> 2);
    let class = ((message_type & 0x0100) >> 7) | ((message_type & 0x0010) >> 4);

    magic_cookie == 0x2112_a442 && method == 0x0003 && class == 0
}

#[tokio::test]
async fn silent_turn_does_not_block_all_or_fall_back_from_relay() {
    let result = timeout(OUTER_DEADLINE, async {
        tokio::join!(run_scenario(Scenario::All), run_scenario(Scenario::Relay))
    })
    .await;
    let Ok((all_result, relay_result)) = result else {
        panic!("TURN gathering regression exceeded its total deadline");
    };
    if let Err(stage) = all_result {
        panic!("All policy failed the direct answer and data-channel regression: {stage}");
    }
    if let Err(stage) = relay_result {
        panic!("Relay policy failed to reject direct fallback without a relay: {stage}");
    }
}

async fn run_scenario(scenario: Scenario) -> Result<(), &'static str> {
    let mut silent_turn = SilentTurn::bind().await?;
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .map_err(|_| "could not bind loopback signaling listener")?;
    let signaling_address = listener
        .local_addr()
        .map_err(|_| "could not inspect loopback signaling listener")?;
    let turn_server = RTCIceServer {
        urls: vec![silent_turn.url.clone()],
        username: "synthetic-turn-user".to_owned(),
        credential: "synthetic-turn-credential".to_owned(),
    };
    let ice_policy = match scenario {
        Scenario::All => RTCIceTransportPolicy::All,
        Scenario::Relay => RTCIceTransportPolicy::Relay,
    };
    let mut server_task = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.map_err(|_| LanError::Signaling)?;
        let mut websocket = accept_async(tcp).await.map_err(|_| LanError::Signaling)?;
        negotiate_offer_answer(
            &mut websocket,
            vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)],
            vec![turn_server],
            CandidatePolicy::Internet {
                allow_loopback: true,
            },
            ice_policy,
        )
        .await
    });

    let (peer, channel, offer) = build_local_offer().await?;
    if super::validate_sdp(
        &offer.sdp,
        CandidatePolicy::Internet {
            allow_loopback: true,
        },
    )
    .is_err()
    {
        let has_non_component_one_candidate = offer.sdp.lines().any(|line| {
            line.strip_prefix("a=candidate:")
                .and_then(|candidate| candidate.split_ascii_whitespace().nth(1))
                .is_some_and(|component| component != "1")
        });
        return Err(if has_non_component_one_candidate {
            "local offer includes a non-component-one candidate rejected by the server profile"
        } else {
            "local WebRTC offer does not meet the server SDP profile"
        });
    }
    let signaling_url = format!("ws://{signaling_address}/turn-gathering-test");
    let (mut websocket, _) = connect_async(signaling_url)
        .await
        .map_err(|_| "could not connect the loopback WebSocket client")?;
    send_offer(&mut websocket, &offer.sdp).await?;
    tokio::select! {
        result = silent_turn.wait_for_allocate() => result?,
        result = &mut server_task => {
            match result {
                Ok(Ok(mut transport)) => {
                    let _ = transport.close().await;
                    return Err("server negotiation completed before issuing TURN Allocate");
                }
                Ok(Err(LanError::WebRtc)) => {
                    return Err("server WebRTC negotiation failed before issuing TURN Allocate");
                }
                Ok(Err(LanError::InvalidSignaling)) => {
                    return Err("server rejected the synthetic offer before issuing TURN Allocate");
                }
                Ok(Err(LanError::Signaling)) => {
                    return Err("server signaling failed before issuing TURN Allocate");
                }
                Ok(Err(_)) => return Err("server failed before issuing TURN Allocate"),
                Err(_) => return Err("server task failed before issuing TURN Allocate"),
            }
        }
    }

    let result = match scenario {
        Scenario::All => {
            run_all_assertions(&mut server_task, &mut websocket, peer.clone(), channel).await
        }
        Scenario::Relay => {
            run_relay_assertions(&mut server_task, &mut websocket, &peer, &channel).await
        }
    };

    if !server_task.is_finished() {
        server_task.abort();
    }
    let _ = peer.close().await;
    result
}

async fn build_local_offer() -> Result<
    (
        Arc<dyn PeerConnection>,
        Arc<dyn DataChannel>,
        RTCSessionDescription,
    ),
    &'static str,
> {
    let (host_candidate_tx, mut host_candidate_rx) = mpsc::unbounded_channel();
    let (complete_tx, _complete_rx) = mpsc::unbounded_channel();
    let peer = PeerConnectionBuilder::<SocketAddr>::new()
        .with_configuration(
            RTCConfigurationBuilder::new()
                .with_ice_transport_policy(RTCIceTransportPolicy::All)
                .build(),
        )
        .with_udp_addrs(vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)])
        .with_handler(Arc::new(GatheringHandler {
            host_candidate_tx,
            complete_tx,
        }))
        .build()
        .await
        .map_err(|_| "could not build the local offerer peer connection")?;
    let peer: Arc<dyn PeerConnection> = Arc::new(peer);
    let channel = peer
        .create_data_channel(DATA_CHANNEL_LABEL, None)
        .await
        .map_err(|_| "could not create the local offerer data channel")?;
    let offer = peer
        .create_offer(None)
        .await
        .map_err(|_| "could not create the local WebRTC offer")?;
    peer.set_local_description(offer)
        .await
        .map_err(|_| "could not set the local WebRTC offer")?;
    timeout(LOCAL_GATHER_DEADLINE, host_candidate_rx.recv())
        .await
        .map_err(|_| "local offer did not produce a direct host candidate")?
        .ok_or("local host candidate observer stopped")?;
    let mut offer = peer
        .local_description()
        .await
        .ok_or("local WebRTC offer description is missing")?;
    // Normalize the locally generated SCTP offer before applying the wire profile.
    offer.sdp = super::data_channel_answer_sdp(
        &offer.sdp,
        CandidatePolicy::Internet {
            allow_loopback: true,
        },
    )
    .map_err(|_| "could not normalize the locally generated data-channel offer")?;
    if !has_candidate_type(&offer.sdp, RTCIceCandidateType::Host) {
        return Err("local offer snapshot omitted its observed host candidate");
    }
    Ok((peer, channel, offer))
}

async fn send_offer<S>(websocket: &mut WebSocketStream<S>, sdp: &str) -> Result<(), &'static str>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let offer = serde_json::to_string(&OfferMessage {
        version: 1,
        kind: "offer",
        sdp,
    })
    .map_err(|_| "could not serialize the synthetic WebRTC offer")?;
    websocket
        .send(Message::Text(offer.into()))
        .await
        .map_err(|_| "could not send the synthetic WebRTC offer")
}

async fn receive_answer<S>(
    websocket: &mut WebSocketStream<S>,
) -> Result<AnswerMessage, &'static str>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        match websocket.next().await {
            Some(Ok(Message::Text(text))) => {
                let answer: AnswerMessage =
                    serde_json::from_str(text.as_str()).map_err(|_| "answer message is invalid")?;
                if answer.version != 1 || answer.kind != "answer" {
                    return Err("server did not return an answer snapshot");
                }
                return Ok(answer);
            }
            Some(Ok(Message::Ping(payload))) => websocket
                .send(Message::Pong(payload))
                .await
                .map_err(|_| "could not acknowledge signaling ping")?,
            Some(Ok(Message::Pong(_))) => {}
            Some(Ok(Message::Close(_))) | None => return Err("signaling closed before answer"),
            Some(Ok(Message::Binary(_) | Message::Frame(_))) | Some(Err(_)) => {
                return Err("signaling returned an invalid answer frame");
            }
        }
    }
}

async fn run_all_assertions<S>(
    server_task: &mut JoinHandle<Result<WebRtcTransport, LanError>>,
    websocket: &mut WebSocketStream<S>,
    peer: Arc<dyn PeerConnection>,
    channel: Arc<dyn DataChannel>,
) -> Result<(), &'static str>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let answer = timeout(CASE_DEADLINE, receive_answer(websocket))
        .await
        .map_err(|_| "All policy did not send its answer within the deadline")??;
    if !has_candidate_type(&answer.sdp, RTCIceCandidateType::Host) {
        return Err("All answer snapshot omitted the available direct host candidate");
    }

    peer.set_remote_description(
        RTCSessionDescription::answer(answer.sdp)
            .map_err(|_| "could not parse the synthetic answer snapshot")?,
    )
    .await
    .map_err(|_| "could not install the synthetic answer snapshot")?;
    timeout(CASE_DEADLINE, wait_for_data_channel_open(&channel))
        .await
        .map_err(|_| "All policy did not open the direct data channel in time")??;

    let mut server_transport = timeout(CASE_DEADLINE, &mut *server_task)
        .await
        .map_err(|_| "All server negotiation did not finish after channel open")?
        .map_err(|_| "All server negotiation task failed")?
        .map_err(|_| "All server negotiation rejected the direct path")?;
    assert_selected_host_pair(&peer).await?;

    let mut client_transport = WebRtcTransport::new(peer, channel);
    client_transport
        .send(Channel::Data, SYNTHETIC_REMOTE_DATA)
        .await
        .map_err(|_| "local direct transport could not send its test frame")?;
    let received_at_server = timeout(CASE_DEADLINE, server_transport.receive())
        .await
        .map_err(|_| "server did not receive the local direct test frame")?
        .map_err(|_| "server direct transport receive failed")?;
    if received_at_server.channel != Channel::Data
        || received_at_server.data.as_slice() != SYNTHETIC_REMOTE_DATA
    {
        return Err("server did not receive the expected local direct test frame");
    }

    server_transport
        .send(Channel::Data, SYNTHETIC_LOCAL_DATA)
        .await
        .map_err(|_| "server direct transport could not send its test frame")?;
    let received_at_client = timeout(CASE_DEADLINE, client_transport.receive())
        .await
        .map_err(|_| "local client did not receive the server test frame")?
        .map_err(|_| "local client direct transport receive failed")?;
    if received_at_client.channel != Channel::Data
        || received_at_client.data.as_slice() != SYNTHETIC_LOCAL_DATA
    {
        return Err("local client did not receive the expected server test frame");
    }

    let client_closed = client_transport.close().await.is_ok();
    let server_closed = server_transport.close().await.is_ok();
    if !client_closed || !server_closed {
        return Err("could not close both direct test transports");
    }
    Ok(())
}

async fn run_relay_assertions<S>(
    server_task: &mut JoinHandle<Result<WebRtcTransport, LanError>>,
    websocket: &mut WebSocketStream<S>,
    peer: &Arc<dyn PeerConnection>,
    channel: &Arc<dyn DataChannel>,
) -> Result<(), &'static str>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    timeout(
        CASE_DEADLINE,
        wait_for_relay_rejection(server_task, websocket, peer, channel),
    )
    .await
    .map_err(|_| "Relay policy did not reject the no-relay case before its deadline")?
}

async fn wait_for_relay_rejection<S>(
    server_task: &mut JoinHandle<Result<WebRtcTransport, LanError>>,
    websocket: &mut WebSocketStream<S>,
    peer: &Arc<dyn PeerConnection>,
    channel: &Arc<dyn DataChannel>,
) -> Result<(), &'static str>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut websocket_open = true;
    let mut channel_events_open = true;
    loop {
        tokio::select! {
            joined = &mut *server_task => {
                let result = joined.map_err(|_| "Relay server negotiation task failed")?;
                match result {
                    Err(LanError::WebRtc) => {
                        if matches!(channel.ready_state().await, Ok(RTCDataChannelState::Open)) {
                            return Err("Relay policy opened the direct data channel");
                        }
                        return Ok(());
                    }
                    Err(_) => return Err("Relay policy failed for an unexpected reason"),
                    Ok(mut transport) => {
                        let _ = transport.close().await;
                        return Err("Relay policy accepted a path without a relay candidate");
                    }
                }
            }
            event = channel.poll(), if channel_events_open => {
                match event {
                    Some(DataChannelEvent::OnOpen) => {
                        return Err("Relay policy opened the direct data channel");
                    }
                    Some(DataChannelEvent::OnClose) | None => channel_events_open = false,
                    _ => {}
                }
            }
            message = websocket.next(), if websocket_open => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let answer: AnswerMessage = serde_json::from_str(text.as_str())
                            .map_err(|_| "Relay policy returned an invalid answer")?;
                        if answer.version != 1 || answer.kind != "answer" {
                            return Err("Relay policy returned a non-answer message");
                        }
                        if has_candidate_type(&answer.sdp, RTCIceCandidateType::Host) {
                            return Err("Relay answer contains a direct host candidate");
                        }
                        let description = RTCSessionDescription::answer(answer.sdp)
                            .map_err(|_| "Relay answer snapshot is malformed")?;
                        peer.set_remote_description(description)
                            .await
                            .map_err(|_| "could not install the Relay answer snapshot")?;
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => {
                        websocket_open = false;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        websocket.send(Message::Pong(payload)).await
                            .map_err(|_| "could not acknowledge signaling ping")?;
                    }
                    Some(Ok(Message::Pong(_) | Message::Binary(_) | Message::Frame(_))) => {}
                }
            }
        }
    }
}

async fn wait_for_data_channel_open(channel: &Arc<dyn DataChannel>) -> Result<(), &'static str> {
    loop {
        match channel.poll().await {
            Some(DataChannelEvent::OnOpen) => return Ok(()),
            Some(
                DataChannelEvent::OnClose | DataChannelEvent::OnClosing | DataChannelEvent::OnError,
            )
            | None => return Err("local direct data channel closed before opening"),
            _ => {}
        }
    }
}

async fn assert_selected_host_pair(peer: &Arc<dyn PeerConnection>) -> Result<(), &'static str> {
    let sctp = peer
        .sctp()
        .await
        .ok_or("direct data channel SCTP transport is missing")?;
    let ice = sctp.transport().ice_transport();
    timeout(CASE_DEADLINE, async {
        loop {
            if let Some(pair) = ice
                .get_selected_candidate_pair()
                .await
                .map_err(|_| "could not inspect the selected direct ICE pair")?
            {
                if pair.local().typ == RTCIceCandidateType::Host
                    && pair.remote().typ == RTCIceCandidateType::Host
                {
                    return Ok(());
                }
                return Err("selected direct ICE pair did not use host candidates");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| "no direct ICE pair was selected before the deadline")?
}

fn has_candidate_type(sdp: &str, candidate_type: RTCIceCandidateType) -> bool {
    let expected = match candidate_type {
        RTCIceCandidateType::Host => "host",
        RTCIceCandidateType::Srflx => "srflx",
        RTCIceCandidateType::Prflx => "prflx",
        RTCIceCandidateType::Relay => "relay",
        _ => return false,
    };
    sdp.lines().any(|line| {
        line.starts_with("a=candidate:")
            && line
                .split_ascii_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|field_pair| field_pair == ["typ", expected])
    })
}
