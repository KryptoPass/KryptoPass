//! Async peer transport contract and bounded in-memory implementation.
//!
//! A transport carries opaque protocol frames. It does not inspect, authenticate,
//! or encrypt their contents. The in-memory implementation uses one bounded
//! FIFO in each direction, retaining send order across [`Channel`] values.
//! Closing either endpoint or cancelling its [`TransportCancelHandle`] closes
//! the whole link and wakes blocked operations.

#[cfg(feature = "loopback-dev")]
mod loopback;
mod memory;
#[cfg(feature = "loopback-dev")]
pub use loopback::LoopbackTransport;

pub use memory::{
    MAX_TRANSPORT_FRAME_SIZE, MAX_TRANSPORT_QUEUE_CAPACITY, MemoryTransport, MemoryTransportConfig,
    TransportCancelHandle, TransportError, TransportFrame,
};

use async_trait::async_trait;
use kryptopass_protocol::Channel;

/// Observed route of an active carrier; never an authorization decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionRoute {
    #[default]
    Unknown,
    Local,
    Direct,
    Turn,
}

/// Address family of the selected path without exposing candidate addresses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionFamily {
    #[default]
    Unknown,
    Ipv4,
    Ipv6,
    Mixed,
}

/// Volatile carrier metadata. It contains no addresses, SDP or credentials.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnectionInfo {
    pub route: ConnectionRoute,
    pub family: ConnectionFamily,
}

/// Asynchronous, ordered transport for opaque peer protocol frames.
///
/// Implementations must preserve the order of successful sends in each
/// direction, including frames carried on different logical channels. A
/// successful [`send`](Self::send) means the frame was accepted by the
/// transport; it does not promise that the peer has received or processed it.
///
/// Dropping a pending operation must be safe. Implementations should report
/// closure, size-limit, and timeout failures without including frame contents
/// in error text or debug output.
#[async_trait]
pub trait PeerTransport: Send {
    /// Read selected-path metadata without consuming frames or changing trust.
    /// Unsupported carriers return unknown; callers bound the observation time.
    async fn connection_info(&mut self) -> ConnectionInfo {
        ConnectionInfo::default()
    }

    /// Enqueue one opaque frame for the peer.
    ///
    /// `data` is copied only when the transport can accept the frame. The
    /// channel is metadata and must not be used to reorder frames.
    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError>;

    /// Receive the next frame in the peer's send order.
    async fn receive(&mut self) -> Result<TransportFrame, TransportError>;

    /// Close the entire link. This operation is idempotent and wakes blocked
    /// sends and receives on both endpoints.
    async fn close(&mut self) -> Result<(), TransportError>;
}
