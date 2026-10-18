//! LAN-only signaling and reliable WebRTC transport.
//!
//! This crate creates no STUN/TURN configuration and does not contact a
//! signaling service. It accepts one WebSocket SDP offer on an explicitly
//! bound local address, returns a fully gathered SDP answer, and exposes the
//! negotiated reliable/ordered data channel as a
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
mod transport;

pub use address::local_addresses;
pub use error::LanError;
pub use host::LanHost;
pub use transport::{WebRtcCancelHandle, WebRtcTransport};
