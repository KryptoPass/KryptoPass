//! Opt-in live TURN dataplane smoke test.
//!
//! Feed a fresh broker response on stdin; this test never calls the broker or logs credentials.

use std::{
    io::{Read, stdin},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use bytes::BytesMut;
use serde::Deserialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use webrtc::{
    data_channel::{DataChannel, DataChannelEvent},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
        RTCIceCandidateType, RTCIceGatheringState, RTCIceServer, RTCIceTransportPolicy,
    },
};
use zeroize::{Zeroize, Zeroizing};

const MAX_BUNDLE_BYTES: usize = 16 * 1024;
const TURN_URL: &str = "turn:turn.cloudflare.com:3478?transport=udp";
const DEFAULT_BIND_ADDR: &str = "0.0.0.0:0";
const DEADLINE: Duration = Duration::from_secs(45);
const PAYLOAD_OFFERER_TO_ANSWERER: &[u8] = &[0x00, 0xff, 0x7f, b'K', 0x10, 0x80];
const PAYLOAD_ANSWERER_TO_OFFERER: &[u8] = &[0xfe, 0x00, b'P', 0x81, 0x22, 0xff];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialBundle {
    v: u8,
    expires_at: u64,
    ice_servers: Vec<CredentialServer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialServer {
    urls: Vec<String>,
    username: String,
    credential: String,
}

impl Drop for CredentialServer {
    fn drop(&mut self) {
        self.username.zeroize();
        self.credential.zeroize();
    }
}

struct TurnCredential {
    username: Zeroizing<String>,
    credential: Zeroizing<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Offerer,
    Answerer,
}

enum ChannelEvent {
    Open(Side),
    Message(Side, Vec<u8>),
}

struct SmokeHandler {
    gathering_complete: UnboundedSender<()>,
    incoming_channel: Option<UnboundedSender<Arc<dyn DataChannel>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for SmokeHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gathering_complete.send(());
        }
    }

    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        if let Some(sender) = &self.incoming_channel {
            let _ = sender.send(channel);
        }
    }
}

#[tokio::test]
#[ignore = "requires fresh TURN credentials on stdin and KP_TURN_SMOKE=1"]
async fn relay_only_peers_exchange_binary_data_over_turn() -> Result<(), &'static str> {
    if !matches!(std::env::var("KP_TURN_SMOKE").as_deref(), Ok("1")) {
        return Err("set KP_TURN_SMOKE=1 to enable the live TURN smoke test");
    }

    let credentials = read_credentials()?;
    phase("credentials-ready");
    let bind_addr = read_bind_addr()?;
    phase("bind-address-ready");

    run_smoke(credentials, bind_addr).await
}

fn read_bind_addr() -> Result<SocketAddr, &'static str> {
    let value = match std::env::var("KP_TURN_SMOKE_BIND_ADDR") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => DEFAULT_BIND_ADDR.to_owned(),
        Err(_) => return Err("could not read KP_TURN_SMOKE_BIND_ADDR"),
    };
    let address = value
        .parse::<SocketAddr>()
        .map_err(|_| "KP_TURN_SMOKE_BIND_ADDR must be a literal SocketAddr")?;
    if address.port() != 0 {
        return Err("KP_TURN_SMOKE_BIND_ADDR must use port zero");
    }
    Ok(address)
}

fn phase(name: &'static str) {
    eprintln!("TURN smoke phase: {name}");
}

fn read_credentials() -> Result<TurnCredential, &'static str> {
    let mut input = Vec::new();
    stdin()
        .lock()
        .take((MAX_BUNDLE_BYTES + 1) as u64)
        .read_to_end(&mut input)
        .map_err(|_| "could not read the bounded TURN bundle from stdin")?;
    if input.len() > MAX_BUNDLE_BYTES {
        input.zeroize();
        return Err("TURN bundle exceeds the 16 KiB input limit");
    }

    let parsed_bundle = serde_json::from_slice(&input);
    input.zeroize();
    let mut bundle: CredentialBundle =
        parsed_bundle.map_err(|_| "stdin is not a valid TURN bundle")?;

    if bundle.v != 1 {
        return Err("TURN bundle version is unsupported");
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch")?
        .as_secs();
    if bundle.expires_at <= now.saturating_add(DEADLINE.as_secs()) {
        return Err("TURN bundle expires before the smoke test deadline");
    }

    let mut selected = None;
    for mut server in bundle.ice_servers.drain(..) {
        if server.urls.iter().any(|url| url == TURN_URL)
            && valid_secret(&server.username)
            && valid_secret(&server.credential)
        {
            selected = Some((
                std::mem::take(&mut server.username),
                std::mem::take(&mut server.credential),
            ));
            break;
        }
    }
    let Some((username, credential)) = selected else {
        return Err("TURN bundle has no valid UDP 3478 credential entry");
    };

    Ok(TurnCredential {
        username: Zeroizing::new(username),
        credential: Zeroizing::new(credential),
    })
}

fn valid_secret(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}

async fn run_smoke(credentials: TurnCredential, bind_addr: SocketAddr) -> Result<(), &'static str> {
    let (offer_gather_tx, mut offer_gather_rx) = unbounded_channel();
    let (answer_gather_tx, mut answer_gather_rx) = unbounded_channel();
    let (incoming_channel_tx, mut incoming_channel_rx) = unbounded_channel();
    let (channel_event_tx, mut channel_event_rx) = unbounded_channel();

    phase("build-offerer-start");
    let offerer = build_peer(
        &credentials,
        bind_addr,
        SmokeHandler {
            gathering_complete: offer_gather_tx,
            incoming_channel: None,
        },
    )
    .await?;
    phase("build-offerer-done");
    phase("build-answerer-start");
    let answerer = match build_peer(
        &credentials,
        bind_addr,
        SmokeHandler {
            gathering_complete: answer_gather_tx,
            incoming_channel: Some(incoming_channel_tx),
        },
    )
    .await
    {
        Ok(peer) => peer,
        Err(error) => {
            phase("build-answerer-failed");
            let _ = offerer.close().await;
            return Err(error);
        }
    };
    phase("build-answerer-done");

    phase("dataplane-start");
    let exercise = tokio::time::timeout(
        DEADLINE,
        exchange_data(
            &offerer,
            &answerer,
            &mut offer_gather_rx,
            &mut answer_gather_rx,
            &mut incoming_channel_rx,
            channel_event_tx,
            &mut channel_event_rx,
        ),
    )
    .await
    .map_err(|_| "TURN dataplane smoke test exceeded its 45 second deadline")
    .and_then(|result| result);
    phase("dataplane-finished");

    phase("close-peers-start");
    let close_offerer = tokio::time::timeout(Duration::from_secs(2), offerer.close()).await;
    let close_answerer = tokio::time::timeout(Duration::from_secs(2), answerer.close()).await;
    if close_offerer.is_err() || close_answerer.is_err() {
        phase("close-peers-failed");
        return Err("could not close both peer connections cleanly");
    }
    phase("close-peers-done");

    exercise
}

async fn build_peer(
    credentials: &TurnCredential,
    bind_addr: SocketAddr,
    handler: SmokeHandler,
) -> Result<impl PeerConnection, &'static str> {
    let ice_server = RTCIceServer {
        urls: vec![TURN_URL.to_owned()],
        username: credentials.username.to_string(),
        credential: credentials.credential.to_string(),
    };
    PeerConnectionBuilder::new()
        .with_configuration(
            RTCConfigurationBuilder::new()
                .with_ice_servers(vec![ice_server])
                .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
                .build(),
        )
        .with_handler(Arc::new(handler))
        .with_udp_addrs(vec![bind_addr])
        .with_dedicated_reactor_pool_size(1)
        .build()
        .await
        .map_err(|_| "could not build a relay-only peer connection")
}

async fn exchange_data(
    offerer: &impl PeerConnection,
    answerer: &impl PeerConnection,
    offer_gather_rx: &mut UnboundedReceiver<()>,
    answer_gather_rx: &mut UnboundedReceiver<()>,
    incoming_channel_rx: &mut UnboundedReceiver<Arc<dyn DataChannel>>,
    channel_event_tx: UnboundedSender<ChannelEvent>,
    channel_event_rx: &mut UnboundedReceiver<ChannelEvent>,
) -> Result<(), &'static str> {
    phase("create-offerer-channel-start");
    let offerer_channel = offerer
        .create_data_channel("kp-turn-smoke", None)
        .await
        .map_err(|_| "could not create the offerer data channel")?;
    phase("create-offerer-channel-done");
    forward_events(
        Arc::clone(&offerer_channel),
        Side::Offerer,
        channel_event_tx.clone(),
    );

    phase("create-offer-start");
    let offer = offerer
        .create_offer(None)
        .await
        .map_err(|_| "could not create the offer")?;
    phase("create-offer-done");
    phase("gather-offer-start");
    offerer
        .set_local_description(offer)
        .await
        .map_err(|_| "could not set the offerer local description")?;
    offer_gather_rx
        .recv()
        .await
        .ok_or("offerer ICE gathering stopped")?;
    let offer = offerer
        .local_description()
        .await
        .ok_or("offerer local description is missing")?;
    phase("gather-offer-done");

    phase("create-answer-start");
    answerer
        .set_remote_description(offer)
        .await
        .map_err(|_| "could not set the answerer remote description")?;
    let answer = answerer
        .create_answer(None)
        .await
        .map_err(|_| "could not create the answer")?;
    phase("create-answer-done");
    phase("gather-answer-start");
    answerer
        .set_local_description(answer)
        .await
        .map_err(|_| "could not set the answerer local description")?;
    answer_gather_rx
        .recv()
        .await
        .ok_or("answerer ICE gathering stopped")?;
    let answer = answerer
        .local_description()
        .await
        .ok_or("answerer local description is missing")?;
    phase("gather-answer-done");
    offerer
        .set_remote_description(answer)
        .await
        .map_err(|_| "could not set the offerer remote description")?;

    phase("remote-channel-start");
    let answerer_channel = incoming_channel_rx
        .recv()
        .await
        .ok_or("answerer data channel was not created")?;
    phase("remote-channel-done");
    forward_events(answerer_channel.clone(), Side::Answerer, channel_event_tx);

    phase("open-channels-start");
    wait_for_open_channels(channel_event_rx).await?;
    phase("open-channels-done");
    phase("send-binary-start");
    offerer_channel
        .send(BytesMut::from(PAYLOAD_OFFERER_TO_ANSWERER))
        .await
        .map_err(|_| "offerer could not send binary data")?;
    answerer_channel
        .send(BytesMut::from(PAYLOAD_ANSWERER_TO_OFFERER))
        .await
        .map_err(|_| "answerer could not send binary data")?;
    phase("send-binary-done");
    phase("receive-binary-start");
    wait_for_binary_messages(channel_event_rx).await?;
    phase("receive-binary-done");

    phase("candidate-pair-offerer-start");
    assert_relay_pair(offerer).await?;
    phase("candidate-pair-offerer-relay");
    phase("candidate-pair-answerer-start");
    assert_relay_pair(answerer).await?;
    phase("candidate-pair-answerer-relay");
    Ok(())
}

fn forward_events(
    channel: Arc<dyn DataChannel>,
    side: Side,
    sender: UnboundedSender<ChannelEvent>,
) {
    tokio::spawn(async move {
        while let Some(event) = channel.poll().await {
            let forwarded = match event {
                DataChannelEvent::OnOpen => Some(ChannelEvent::Open(side)),
                DataChannelEvent::OnMessage(message) => {
                    Some(ChannelEvent::Message(side, message.data.to_vec()))
                }
                DataChannelEvent::OnClose => break,
                _ => None,
            };
            if let Some(event) = forwarded
                && sender.send(event).is_err()
            {
                break;
            }
        }
    });
}

async fn wait_for_open_channels(
    receiver: &mut UnboundedReceiver<ChannelEvent>,
) -> Result<(), &'static str> {
    let mut offerer_open = false;
    let mut answerer_open = false;
    while !offerer_open || !answerer_open {
        match receiver
            .recv()
            .await
            .ok_or("data channel event stream stopped before open")?
        {
            ChannelEvent::Open(Side::Offerer) => offerer_open = true,
            ChannelEvent::Open(Side::Answerer) => answerer_open = true,
            ChannelEvent::Message(_, _) => {}
        }
    }
    Ok(())
}

async fn wait_for_binary_messages(
    receiver: &mut UnboundedReceiver<ChannelEvent>,
) -> Result<(), &'static str> {
    let mut offerer_message = None;
    let mut answerer_message = None;
    while offerer_message.is_none() || answerer_message.is_none() {
        match receiver
            .recv()
            .await
            .ok_or("data channel event stream stopped before message delivery")?
        {
            ChannelEvent::Message(Side::Offerer, bytes) => offerer_message = Some(bytes),
            ChannelEvent::Message(Side::Answerer, bytes) => answerer_message = Some(bytes),
            ChannelEvent::Open(_) => {}
        }
    }
    if offerer_message.as_deref() != Some(PAYLOAD_ANSWERER_TO_OFFERER)
        || answerer_message.as_deref() != Some(PAYLOAD_OFFERER_TO_ANSWERER)
    {
        return Err("binary payload did not arrive intact in both directions");
    }
    Ok(())
}

async fn assert_relay_pair(peer: &impl PeerConnection) -> Result<(), &'static str> {
    let sctp = peer
        .sctp()
        .await
        .ok_or("data channel SCTP transport is missing")?;
    let ice = sctp.transport().ice_transport();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(pair) = ice
            .get_selected_candidate_pair()
            .await
            .map_err(|_| "could not inspect the selected ICE pair")?
        {
            return if pair.local().typ == RTCIceCandidateType::Relay
                && pair.remote().typ == RTCIceCandidateType::Relay
            {
                Ok(())
            } else {
                Err("selected ICE candidate pair is not relayed at both endpoints")
            };
        }
        if Instant::now() >= deadline {
            return Err("ICE did not select a candidate pair");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
