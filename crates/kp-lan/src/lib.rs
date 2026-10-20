//! LAN and Internet signaling with reliable WebRTC transport.
//!
//! LAN signaling accepts an SDP offer on an explicitly bound local address
//! without external ICE servers. [`RelayHost`] provides outbound WSS signaling
//! with locally configured STUN/TURN for Internet connections. Both expose the
//! negotiated reliable, ordered DataChannel as a
//! [`PeerTransport`](kryptopass_transport::PeerTransport).
//!
//! IPv4 private, link-local, and loopback addresses and IPv6 ULA/loopback
//! addresses are supported. IPv6 link-local addresses are omitted because the
//! public API accepts an `IpAddr` without an interface scope. mDNS is an
//! optional discovery hint; a failure to start or register it does not prevent
//! signaling on the returned TCP port.

mod address;
mod error;
mod host;
mod internet;
mod transport;
mod turn;

pub use address::{InterfaceKind, LanInterface, local_addresses, local_interfaces};
pub use error::LanError;
pub use host::{LanHost, ReconnectHost, ResumeAttempt, ResumeListener};
pub use internet::{
    IceServerConfig, InternetIceConfig, InternetIceTransportPolicy, RelayHost, RelayRoute,
};
pub use transport::{WebRtcCancelHandle, WebRtcTransport};
