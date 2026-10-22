use thiserror::Error;

/// LAN signaling and WebRTC failures.
///
/// Error messages intentionally omit peer-controlled SDP, WebSocket payloads,
/// and frame contents.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum LanError {
    /// The requested bind address is not an accepted LAN address.
    #[error("address is outside the supported LAN address ranges")]
    InvalidAddress,
    /// No supported local address was found.
    #[error("no supported LAN address is available")]
    NoLanAddress,
    /// An interface enumeration operation failed.
    #[error("local interface enumeration failed")]
    AddressDiscovery,
    /// The session identifier must not be all zero.
    #[error("session identifier is invalid")]
    InvalidSession,
    /// The signaling listener could not be bound.
    #[error("could not bind the LAN signaling listener")]
    Bind,
    /// Signaling, SDP, or data-channel negotiation exceeded its deadline.
    #[error("LAN negotiation timed out")]
    Timeout,
    /// A WebSocket, signaling message, or SDP description was rejected.
    #[error("LAN signaling message is invalid")]
    InvalidSignaling,
    /// A signaling or SDP operation failed.
    #[error("LAN signaling operation failed")]
    Signaling,
    /// WebRTC peer-connection setup failed.
    #[error("WebRTC peer connection failed")]
    WebRtc,
    /// The remote peer offered an unexpected data channel.
    #[error("WebRTC data channel does not match the required profile")]
    InvalidDataChannel,
    /// A relay origin or route is malformed or uses an unsupported scheme.
    #[error("relay route is invalid")]
    InvalidRelayRoute,
    /// ICE server configuration is malformed or exceeds its limits.
    #[error("Internet ICE configuration is invalid")]
    InvalidIceConfig,
    /// TURN broker credentials could not be authenticated or validated.
    #[error("TURN credentials are unavailable")]
    TurnBroker,
    /// The remote device selector was rejected by the caller's authorization policy.
    #[error("remote device is not authorized for this relay session")]
    UnauthorizedDevice,
    /// The local network privacy policy blocked this operation.
    #[error("the local network privacy policy blocked the connection")]
    NetworkPolicy,
}
