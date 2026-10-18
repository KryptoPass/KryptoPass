use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use kryptopass_protocol::{Channel, MAX_FRAME_SIZE};
use thiserror::Error;
use tokio::sync::{mpsc, watch};

use crate::PeerTransport;

/// Largest frame accepted by this crate, matching the protocol envelope cap.
pub const MAX_TRANSPORT_FRAME_SIZE: usize = MAX_FRAME_SIZE;

/// Largest queue capacity accepted by [`MemoryTransportConfig`].
pub const MAX_TRANSPORT_QUEUE_CAPACITY: usize = 64;

/// Bounds and operation deadlines for a pair of in-memory transports.
///
/// There is one FIFO queue in each direction. `queue_capacity` limits queued
/// frames per direction, and `max_frame_size` limits each frame's byte length.
/// A blocked send does not copy its input until a queue slot is available.
/// Queue capacities above [`MAX_TRANSPORT_QUEUE_CAPACITY`] are rejected.
/// Non-zero timeouts require a Tokio runtime with its time driver enabled; a
/// zero timeout is rejected, and `None` disables that operation's deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryTransportConfig {
    /// Maximum number of frames queued in either direction.
    pub queue_capacity: usize,
    /// Maximum accepted frame size in bytes.
    pub max_frame_size: usize,
    /// Maximum time a send may wait for queue capacity. `None` waits until
    /// capacity is available or the link closes.
    pub send_timeout: Option<Duration>,
    /// Maximum time a receive may wait for a frame. `None` waits until a frame
    /// arrives or the link closes.
    pub receive_timeout: Option<Duration>,
}

impl Default for MemoryTransportConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 16,
            max_frame_size: MAX_TRANSPORT_FRAME_SIZE,
            send_timeout: Some(Duration::from_secs(30)),
            receive_timeout: Some(Duration::from_secs(30)),
        }
    }
}

impl MemoryTransportConfig {
    fn validate(&self) -> Result<(), TransportError> {
        if !(1..=MAX_TRANSPORT_QUEUE_CAPACITY).contains(&self.queue_capacity) {
            return Err(TransportError::InvalidConfig {
                setting: "queue_capacity",
            });
        }
        if !(1..=MAX_TRANSPORT_FRAME_SIZE).contains(&self.max_frame_size) {
            return Err(TransportError::InvalidConfig {
                setting: "max_frame_size",
            });
        }
        if self.send_timeout.is_some_and(|timeout| timeout.is_zero()) {
            return Err(TransportError::InvalidConfig {
                setting: "send_timeout",
            });
        }
        if self
            .receive_timeout
            .is_some_and(|timeout| timeout.is_zero())
        {
            return Err(TransportError::InvalidConfig {
                setting: "receive_timeout",
            });
        }
        Ok(())
    }
}

/// Opaque frame received from a peer.
///
/// The byte vector is intentionally omitted from debug output because a
/// transport frame may contain sensitive pairing or synchronization data.
pub struct TransportFrame {
    /// Logical protocol channel attached to this frame.
    pub channel: Channel,
    /// Opaque encoded frame bytes.
    pub data: Vec<u8>,
}

impl fmt::Debug for TransportFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportFrame")
            .field("channel", &self.channel)
            .field("data_len", &self.data.len())
            .finish()
    }
}

/// Failures returned by the transport contract.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TransportError {
    /// The link has been closed or cancelled.
    #[error("transport link is closed")]
    Closed,
    /// The frame is larger than the configured maximum.
    #[error("frame size {actual} exceeds maximum {maximum}")]
    FrameTooLarge { actual: usize, maximum: usize },
    /// A transport bound or deadline is invalid.
    #[error("invalid transport setting: {setting}")]
    InvalidConfig { setting: &'static str },
    /// A send did not obtain queue capacity before its deadline.
    #[error("transport send timed out")]
    SendTimedOut,
    /// A receive did not obtain a frame before its deadline.
    #[error("transport receive timed out")]
    ReceiveTimedOut,
}

/// A clonable handle that can cancel both endpoints of a memory link.
///
/// Cancellation is link-wide and permanent. It is useful when a task needs to
/// wake a send or receive future that currently borrows its
/// [`MemoryTransport`]. Cancellation has the same effects as closing either
/// endpoint.
#[derive(Clone)]
pub struct TransportCancelHandle {
    link: Arc<LinkState>,
}

impl TransportCancelHandle {
    /// Close the link and wake blocked operations on both endpoints.
    pub fn cancel(&self) {
        self.link.close();
    }

    /// Return whether the link has been closed or cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.link.is_closed()
    }
}

impl fmt::Debug for TransportCancelHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportCancelHandle")
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

/// One endpoint of a bounded in-memory peer link.
///
/// The endpoint has one outbound and one inbound FIFO. Channel values are
/// carried as metadata in those queues, so interleaved control and data frames
/// retain one total send order per direction. Calling [`PeerTransport::close`]
/// or dropping either endpoint closes both ends and wakes pending operations.
pub struct MemoryTransport {
    outbound: mpsc::Sender<TransportFrame>,
    inbound: mpsc::Receiver<TransportFrame>,
    link: Arc<LinkState>,
    close_rx: watch::Receiver<bool>,
    config: MemoryTransportConfig,
}

impl MemoryTransport {
    /// Create two connected endpoints with the same queue bounds and deadlines.
    ///
    /// Returns [`TransportError::InvalidConfig`] if a capacity is zero or
    /// exceeds its hard limit, or if either configured timeout is zero.
    pub fn pair(config: MemoryTransportConfig) -> Result<(Self, Self), TransportError> {
        config.validate()?;

        let (a_to_b_tx, a_to_b_rx) = mpsc::channel(config.queue_capacity);
        let (b_to_a_tx, b_to_a_rx) = mpsc::channel(config.queue_capacity);
        let (close_tx, close_rx) = watch::channel(false);
        let link = Arc::new(LinkState {
            closed: AtomicBool::new(false),
            gate: Mutex::new(()),
            close_tx,
        });

        let a = Self {
            outbound: a_to_b_tx,
            inbound: b_to_a_rx,
            link: Arc::clone(&link),
            close_rx: close_rx.clone(),
            config: config.clone(),
        };
        let b = Self {
            outbound: b_to_a_tx,
            inbound: a_to_b_rx,
            link,
            close_rx,
            config,
        };

        Ok((a, b))
    }

    /// Return a handle that can close the whole link from another task.
    pub fn cancel_handle(&self) -> TransportCancelHandle {
        TransportCancelHandle {
            link: Arc::clone(&self.link),
        }
    }

    async fn send_inner(&self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        if data.len() > self.config.max_frame_size {
            return Err(TransportError::FrameTooLarge {
                actual: data.len(),
                maximum: self.config.max_frame_size,
            });
        }
        if self.link.is_closed() {
            return Err(TransportError::Closed);
        }

        let mut close_rx = self.close_rx.clone();
        if self.link.is_closed() {
            return Err(TransportError::Closed);
        }

        let permit = tokio::select! {
            biased;
            _ = close_rx.changed() => return Err(TransportError::Closed),
            permit = self.outbound.reserve() => permit.map_err(|_| TransportError::Closed)?,
        };

        let frame = TransportFrame {
            channel,
            data: data.to_vec(),
        };
        let _gate = lock_unpoisoned(&self.link.gate);
        if self.link.is_closed() {
            return Err(TransportError::Closed);
        }
        permit.send(frame);
        Ok(())
    }

    async fn receive_inner(&mut self) -> Result<TransportFrame, TransportError> {
        if self.link.is_closed() {
            return Err(TransportError::Closed);
        }

        let mut close_rx = self.close_rx.clone();
        if self.link.is_closed() {
            return Err(TransportError::Closed);
        }

        let frame = tokio::select! {
            biased;
            _ = close_rx.changed() => return Err(TransportError::Closed),
            frame = self.inbound.recv() => frame.ok_or(TransportError::Closed)?,
        };

        let _gate = lock_unpoisoned(&self.link.gate);
        if self.link.is_closed() {
            return Err(TransportError::Closed);
        }
        Ok(frame)
    }
}

#[async_trait::async_trait]
impl PeerTransport for MemoryTransport {
    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        match self.config.send_timeout {
            Some(deadline) => tokio::time::timeout(deadline, self.send_inner(channel, data))
                .await
                .map_err(|_| TransportError::SendTimedOut)?,
            None => self.send_inner(channel, data).await,
        }
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        match self.config.receive_timeout {
            Some(deadline) => tokio::time::timeout(deadline, self.receive_inner())
                .await
                .map_err(|_| TransportError::ReceiveTimedOut)?,
            None => self.receive_inner().await,
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.link.close();
        Ok(())
    }
}

impl Drop for MemoryTransport {
    fn drop(&mut self) {
        self.link.close();
    }
}

struct LinkState {
    closed: AtomicBool,
    gate: Mutex<()>,
    close_tx: watch::Sender<bool>,
}

impl LinkState {
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn close(&self) {
        let _gate = lock_unpoisoned(&self.gate);
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.close_tx.send_replace(true);
        }
    }
}

fn lock_unpoisoned(mutex: &Mutex<()>) -> MutexGuard<'_, ()> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
