use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::LanError;

/// Return unique, supported local LAN addresses in stable sorted order.
///
/// IPv4 private, link-local, and loopback addresses and IPv6 ULA/loopback
/// addresses are included. IPv6 link-local addresses are excluded because an
/// [`IpAddr`] does not carry the interface scope needed to connect to them.
pub fn local_addresses() -> Result<Vec<IpAddr>, LanError> {
    let interfaces = if_addrs::get_if_addrs().map_err(|_| LanError::AddressDiscovery)?;
    let mut addresses: Vec<_> = interfaces
        .into_iter()
        .map(|interface| interface.ip())
        .filter(|address| is_supported_address(*address))
        .collect();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() {
        return Err(LanError::NoLanAddress);
    }
    Ok(addresses)
}

pub(crate) fn is_supported_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_supported_v4(address),
        IpAddr::V6(address) => is_supported_v6(address),
    }
}

fn is_supported_v4(address: Ipv4Addr) -> bool {
    !address.is_unspecified()
        && !address.is_multicast()
        && address != Ipv4Addr::BROADCAST
        && (address.is_private() || address.is_link_local() || address.is_loopback())
}

fn is_supported_v6(address: Ipv6Addr) -> bool {
    !address.is_unspecified()
        && !address.is_multicast()
        && (address.is_loopback() || address.segments()[0] & 0xfe00 == 0xfc00)
}
