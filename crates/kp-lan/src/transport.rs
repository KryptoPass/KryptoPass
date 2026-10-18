use std::{fmt, sync::Arc, time::Duration};

use bytes::BytesMut;
use kryptopass_protocol::{Channel, MAX_FRAME_SIZE};
use kryptopass_transport::{PeerTransport, TransportError, TransportFrame};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::timeout,
};
use webrtc::{
    data_channel::{DataChannel, DataChannelEvent},
    peer_connection::PeerConnection,
};

const DATACHANNEL_MESSAGE_MAX: usize = 16_384;
const FRAGMENT_HEADER_SIZE: usize = 9;
const FRAGMENT_DATA_MAX: usize = DATACHANNEL_MESSAGE_MAX - FRAGMENT_HEADER_SIZE;
const MAX_FRAGMENTS: usize = 5;
const INBOUND_FRAME_QUEUE_CAPACITY: usize = 32;
const DATA_CHANNEL_OUTSTANDING_LIMIT: usize = 256 * 1024;
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(120);
const CLOSE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const CLOSE_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Link-wide cancellation handle for a [`WebRtcTransport`].
#[derive(Clone)]
pub struct WebRtcCancelHandle {
    state: watch::Sender<TransportState>,
}

impl WebRtcCancelHandle {
    /// Cancel the connection and wake blocked send/receive operations.
    pub fn cancel(&self) {
        self.state.send_replace(TransportState::Aborted);
    }

    /// Return whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        *self.state.borrow() == TransportState::Aborted
    }
}

impl fmt::Debug for WebRtcCancelHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebRtcCancelHandle")
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

/// Reliable, ordered WebRTC DataChannel exposed as opaque protocol frames.
///
/// The data channel framing is private to this adapter. One incomplete frame
/// may be reassembled at a time, completed inbound frames are queued at most
/// 32 deep, and violations close the peer connection.
pub struct WebRtcTransport {
    connection: Option<Arc<dyn PeerConnection>>,
    channel: Arc<dyn DataChannel>,
    inbound: mpsc::Receiver<TransportFrame>,
    state_tx: watch::Sender<TransportState>,
    state_rx: watch::Receiver<TransportState>,
    pump: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransportState {
    Open,
    RemoteEof,
    Aborted,
}

impl WebRtcTransport {
    pub(crate) fn new(connection: Arc<dyn PeerConnection>, channel: Arc<dyn DataChannel>) -> Self {
        let (inbound_tx, inbound) = mpsc::channel(INBOUND_FRAME_QUEUE_CAPACITY);
        let (state_tx, state_rx) = watch::channel(TransportState::Open);
        let pump_state_rx = state_rx.clone();
        let pump_state_tx = state_tx.clone();
        let pc_for_pump = Arc::clone(&connection);
        let dc_for_pump = Arc::clone(&channel);
        let pump = tokio::spawn(async move {
            receive_pump(
                pc_for_pump,
                dc_for_pump,
                inbound_tx,
                pump_state_rx,
                pump_state_tx,
            )
            .await;
        });
        Self {
            connection: Some(connection),
            channel,
            inbound,
            state_tx,
            state_rx,
            pump: Some(pump),
        }
    }

    /// Get a clonable handle that closes the full peer connection.
    pub fn cancel_handle(&self) -> WebRtcCancelHandle {
        WebRtcCancelHandle {
            state: self.state_tx.clone(),
        }
    }

    async fn close_inner(&mut self) {
        self.state_tx.send_replace(TransportState::Aborted);
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
        if let Some(connection) = self.connection.take() {
            let _ = self.channel.close().await;
            let _ = connection.close().await;
        }
    }

    async fn close_gracefully(&mut self) {
        let channel = Arc::clone(&self.channel);
        let state = self.state_rx.clone();
        let drain = async move {
            loop {
                match channel.outstanding_bytes().await {
                    Ok(0) | Err(_) => return,
                    Ok(_) => tokio::time::sleep(CLOSE_DRAIN_POLL_INTERVAL).await,
                }
            }
        };

        tokio::select! {
            _ = wait_until_aborted(state) => {},
            _ = timeout(CLOSE_DRAIN_TIMEOUT, drain) => {},
        }
        self.close_inner().await;
    }
}

#[async_trait::async_trait]
impl PeerTransport for WebRtcTransport {
    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        if !self.is_open() {
            return Err(TransportError::Closed);
        }
        if data.is_empty() {
            return Err(TransportError::InvalidConfig {
                setting: "empty_frame",
            });
        }
        if data.len() > MAX_FRAME_SIZE {
            return Err(TransportError::FrameTooLarge {
                actual: data.len(),
                maximum: MAX_FRAME_SIZE,
            });
        }

        let lane = match channel {
            Channel::Control => 0_u8,
            Channel::Data => 1_u8,
        };
        let channel = Arc::clone(&self.channel);
        let state = self.state_rx.clone();
        let send_data = async move {
            let mut offset = 0_usize;
            while offset < data.len() {
                let chunk_len = (data.len() - offset).min(FRAGMENT_DATA_MAX);
                let mut message = BytesMut::with_capacity(FRAGMENT_HEADER_SIZE + chunk_len);
                message.extend_from_slice(&[lane]);
                message.extend_from_slice(&(data.len() as u32).to_be_bytes());
                message.extend_from_slice(&(offset as u32).to_be_bytes());
                message.extend_from_slice(&data[offset..offset + chunk_len]);
                loop {
                    let outstanding = channel
                        .outstanding_bytes()
                        .await
                        .map_err(|_| TransportError::Closed)?;
                    if outstanding
                        .checked_add(message.len())
                        .is_some_and(|size| size <= DATA_CHANNEL_OUTSTANDING_LIMIT)
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                channel
                    .send(message)
                    .await
                    .map_err(|_| TransportError::Closed)?;
                offset += chunk_len;
            }
            Ok::<(), TransportError>(())
        };

        let send_result = tokio::select! {
            _ = wait_until_terminated(state) => Err(TransportError::Closed),
            result = timeout(SEND_TIMEOUT, send_data) => match result {
                Ok(result) => result,
                Err(_) => Err(TransportError::SendTimedOut),
            },
        };
        match send_result {
            Ok(()) => Ok(()),
            Err(error) => {
                self.close_inner().await;
                Err(error)
            }
        }
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        if *self.state_rx.borrow() == TransportState::Aborted {
            return Err(TransportError::Closed);
        }
        let state = self.state_rx.clone();
        let result = tokio::select! {
            biased;
            _ = wait_until_aborted(state) => Err(TransportError::Closed),
            result = timeout(RECEIVE_TIMEOUT, self.inbound.recv()) => match result {
                Ok(Some(frame)) => Ok(frame),
                Ok(None) => Err(TransportError::Closed),
                Err(_) => Err(TransportError::ReceiveTimedOut),
            },
        };
        match result {
            Ok(frame) if *self.state_rx.borrow() != TransportState::Aborted => Ok(frame),
            Err(error) => {
                self.close_inner().await;
                Err(error)
            }
            Ok(_) => {
                self.close_inner().await;
                Err(TransportError::Closed)
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.close_gracefully().await;
        Ok(())
    }
}

impl WebRtcTransport {
    fn is_open(&self) -> bool {
        *self.state_rx.borrow() == TransportState::Open
    }
}

impl Drop for WebRtcTransport {
    fn drop(&mut self) {
        self.state_tx.send_replace(TransportState::Aborted);
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
        if let Some(connection) = self.connection.take() {
            let channel = Arc::clone(&self.channel);
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = channel.close().await;
                    let _ = connection.close().await;
                });
            }
        }
    }
}

async fn wait_until_aborted(mut state: watch::Receiver<TransportState>) {
    if *state.borrow() == TransportState::Aborted {
        return;
    }
    while state.changed().await.is_ok() {
        if *state.borrow() == TransportState::Aborted {
            return;
        }
    }
}

async fn wait_until_terminated(mut state: watch::Receiver<TransportState>) {
    if *state.borrow() != TransportState::Open {
        return;
    }
    while state.changed().await.is_ok() {
        if *state.borrow() != TransportState::Open {
            return;
        }
    }
}

struct IncompleteFrame {
    lane: u8,
    total: usize,
    next_offset: usize,
    fragments: usize,
    bytes: Vec<u8>,
}

async fn receive_pump(
    connection: Arc<dyn PeerConnection>,
    channel: Arc<dyn DataChannel>,
    inbound: mpsc::Sender<TransportFrame>,
    state: watch::Receiver<TransportState>,
    state_tx: watch::Sender<TransportState>,
) {
    let mut incomplete: Option<IncompleteFrame> = None;
    loop {
        let event = tokio::select! {
            biased;
            _ = wait_until_terminated(state.clone()) => break,
            event = channel.poll() => event,
        };
        match event {
            Some(DataChannelEvent::OnMessage(message)) if !message.is_string => {
                let result = process_fragment(&mut incomplete, &message.data);
                match result {
                    Ok(Some(frame)) => match inbound.try_send(frame) {
                        Ok(()) => {}
                        Err(_) => {
                            abort_receive_pump(inbound, &state_tx, &channel, &connection).await;
                            return;
                        }
                    },
                    Ok(None) => {}
                    Err(()) => {
                        abort_receive_pump(inbound, &state_tx, &channel, &connection).await;
                        return;
                    }
                }
            }
            Some(DataChannelEvent::OnMessage(_)) | Some(DataChannelEvent::OnError) => {
                abort_receive_pump(inbound, &state_tx, &channel, &connection).await;
                return;
            }
            Some(DataChannelEvent::OnClose) | None => {
                finish_remote_eof(inbound, &state_tx, &channel, &connection).await;
                return;
            }
            Some(DataChannelEvent::OnClosing) => {}
            Some(DataChannelEvent::OnOpen)
            | Some(DataChannelEvent::OnBufferedAmountLow)
            | Some(DataChannelEvent::OnBufferedAmountHigh) => {}
            _ => {}
        }
    }
    abort_receive_pump(inbound, &state_tx, &channel, &connection).await;
}

async fn finish_remote_eof(
    inbound: mpsc::Sender<TransportFrame>,
    state_tx: &watch::Sender<TransportState>,
    channel: &Arc<dyn DataChannel>,
    connection: &Arc<dyn PeerConnection>,
) {
    // Closing the queue first ensures a receiver awakened by RemoteEof can
    // drain every complete frame queued before the ordered DataChannel EOF.
    drop(inbound);
    state_tx.send_if_modified(|state| {
        if *state == TransportState::Open {
            *state = TransportState::RemoteEof;
            true
        } else {
            false
        }
    });
    let _ = channel.close().await;
    let _ = connection.close().await;
}

async fn abort_receive_pump(
    inbound: mpsc::Sender<TransportFrame>,
    state_tx: &watch::Sender<TransportState>,
    channel: &Arc<dyn DataChannel>,
    connection: &Arc<dyn PeerConnection>,
) {
    state_tx.send_replace(TransportState::Aborted);
    drop(inbound);
    let _ = channel.close().await;
    let _ = connection.close().await;
}

fn process_fragment(
    incomplete: &mut Option<IncompleteFrame>,
    message: &[u8],
) -> Result<Option<TransportFrame>, ()> {
    if !(FRAGMENT_HEADER_SIZE + 1..=DATACHANNEL_MESSAGE_MAX).contains(&message.len()) {
        return Err(());
    }
    let lane = message[0];
    if lane > 1 {
        return Err(());
    }
    let total = u32::from_be_bytes(message[1..5].try_into().map_err(|_| ())?) as usize;
    let offset = u32::from_be_bytes(message[5..9].try_into().map_err(|_| ())?) as usize;
    let payload = &message[FRAGMENT_HEADER_SIZE..];
    if total == 0 || total > MAX_FRAME_SIZE || offset >= total {
        return Err(());
    }
    let expected = (total - offset).min(FRAGMENT_DATA_MAX);
    if payload.len() != expected || offset.checked_add(payload.len()).is_none() {
        return Err(());
    }

    if offset == 0 {
        if incomplete.is_some() || total < 1 {
            return Err(());
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(total).map_err(|_| ())?;
        bytes.extend_from_slice(payload);
        if bytes.len() == total {
            return Ok(Some(TransportFrame {
                channel: if lane == 0 {
                    Channel::Control
                } else {
                    Channel::Data
                },
                data: bytes,
            }));
        }
        *incomplete = Some(IncompleteFrame {
            lane,
            total,
            next_offset: payload.len(),
            fragments: 1,
            bytes,
        });
        return Ok(None);
    }

    let frame = incomplete.as_mut().ok_or(())?;
    if frame.lane != lane
        || frame.total != total
        || frame.next_offset != offset
        || frame.fragments >= MAX_FRAGMENTS
    {
        return Err(());
    }
    frame.bytes.extend_from_slice(payload);
    frame.next_offset = frame.next_offset.checked_add(payload.len()).ok_or(())?;
    frame.fragments += 1;
    if frame.next_offset == frame.total {
        let frame = incomplete.take().ok_or(())?;
        return Ok(Some(TransportFrame {
            channel: if lane == 0 {
                Channel::Control
            } else {
                Channel::Data
            },
            data: frame.bytes,
        }));
    }
    Ok(None)
}
