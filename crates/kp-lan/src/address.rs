use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::LanError;
use crate::privacy::route_source_address;

/// Interface category for presenting a local network choice, never authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InterfaceKind {
    Wifi,
    Ethernet,
    Other,
    Virtual,
    Loopback,
}

/// Supported address and its OS interface label for local selection.
#[derive(Clone, Debug)]
pub struct LanInterface {
    pub name: String,
    pub address: IpAddr,
    pub kind: InterfaceKind,
}

/// Lists supported interfaces with physical networks before VPNs and bridges.
/// Names are only hints. No address range is treated as proof of interface type.
pub fn local_interfaces() -> Result<Vec<LanInterface>, LanError> {
    let mut interfaces: Vec<_> = if_addrs::get_if_addrs()
        .map_err(|_| LanError::AddressDiscovery)?
        .into_iter()
        .filter(|interface| is_supported_address(interface.ip()))
        .map(|interface| {
            let address = interface.ip();
            let lower = interface.name.to_ascii_lowercase();
            let kind = if address.is_loopback() {
                InterfaceKind::Loopback
            } else if lower.starts_with("wl") || lower.contains("wi-fi") || lower.contains("wifi") {
                InterfaceKind::Wifi
            } else if [
                "docker",
                "br-",
                "veth",
                "virbr",
                "tun",
                "tap",
                "wg",
                "tailscale",
                "zt",
                "utun",
                "vpn",
            ]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
            {
                InterfaceKind::Virtual
            } else if lower.starts_with("en")
                || lower.starts_with("eth")
                || lower.contains("ethernet")
            {
                InterfaceKind::Ethernet
            } else {
                InterfaceKind::Other
            };
            LanInterface {
                name: interface.name,
                address,
                kind,
            }
        })
        .collect();
    interfaces.sort_by(|left, right| {
        (left.kind, left.address.is_ipv6(), &left.name, left.address).cmp(&(
            right.kind,
            right.address.is_ipv6(),
            &right.name,
            right.address,
        ))
    });
    interfaces.dedup_by(|left, right| left.address == right.address);
    if interfaces.is_empty() {
        return Err(LanError::NoLanAddress);
    }
    Ok(interfaces)
}

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

/// Return one locally assigned address selected by the OS for its default
/// IPv4/IPv6 route. This never chooses a second interface merely because it
/// appears physical or private; callers can bind this address for recovery
/// when the user has not saved an explicit LAN address.
#[must_use]
pub fn os_selected_local_address() -> Option<IpAddr> {
    let interfaces = if_addrs::get_if_addrs().ok()?;
    let selected = [
        route_source_address(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1))).ok(),
        route_source_address(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))).ok(),
    ];
    selected.into_iter().flatten().find(|address| {
        is_supported_address(*address)
            && interfaces
                .iter()
                .any(|interface| interface.ip() == *address)
    })
}

/// Keep an explicit address while it is assigned; otherwise follow only the
/// current OS-selected source. Never pick an arbitrary remaining interface.
#[must_use]
pub fn select_local_bind_address(
    configured: Option<IpAddr>,
    assigned: &[IpAddr],
    system_selected: Option<IpAddr>,
) -> Option<IpAddr> {
    configured
        .filter(|address| assigned.contains(address))
        .or_else(|| system_selected.filter(|address| assigned.contains(address)))
}

pub(crate) fn is_supported_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_supported_v4(address),
        IpAddr::V6(address) => is_supported_v6(address),
    }
}

/// Internet ICE policy: admit routed unicast addresses, including global IPv6,
/// plus private and local-link addresses useful for direct connectivity.
pub(crate) fn is_supported_internet_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_supported_internet_v4(address),
        IpAddr::V6(address) => is_supported_internet_v6(address),
    }
}

/// Bind WebRTC UDP sockets only to the local address selected by the OS for
/// the relay's route. Never add other interfaces as ICE candidates.
pub fn internet_udp_bind_addresses(route_address: IpAddr) -> Result<Vec<SocketAddr>, LanError> {
    if !is_supported_internet_address(route_address) && !route_address.is_loopback() {
        return Err(LanError::InvalidAddress);
    }
    Ok(vec![SocketAddr::new(route_address, 0)])
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

fn is_supported_internet_v4(address: Ipv4Addr) -> bool {
    if address.is_unspecified()
        || address.is_loopback()
        || address.is_multicast()
        || address == Ipv4Addr::BROADCAST
    {
        return false;
    }

    if address.is_private() || address.is_link_local() {
        return true;
    }

    let octets = address.octets();
    let in_range = |network: [u8; 4], prefix: u8| {
        let full_bytes = usize::from(prefix / 8);
        let remaining_bits = prefix % 8;
        octets[..full_bytes] == network[..full_bytes]
            && (remaining_bits == 0
                || (octets[full_bytes] >> (8 - remaining_bits))
                    == (network[full_bytes] >> (8 - remaining_bits)))
    };

    // Exclude special-purpose, shared, documentation, benchmarking, and
    // reserved ranges. RFC1918 and IPv4 link-local were accepted above for
    // direct local paths.
    !(octets[0] == 0
        || in_range([100, 64, 0, 0], 10)
        || in_range([192, 0, 0, 0], 24)
        || in_range([192, 0, 2, 0], 24)
        || in_range([192, 88, 99, 0], 24)
        || in_range([198, 18, 0, 0], 15)
        || in_range([198, 51, 100, 0], 24)
        || in_range([203, 0, 113, 0], 24)
        || octets[0] >= 240)
}

fn is_supported_internet_v6(address: Ipv6Addr) -> bool {
    if address.is_unspecified() || address.is_loopback() || address.is_multicast() {
        return false;
    }
    let segments = address.segments();
    let unique_local = segments[0] & 0xfe00 == 0xfc00;
    if unique_local {
        return true;
    }
    if segments[0] & 0xe000 != 0x2000 {
        return false;
    }

    // Exclude protocol assignments, documentation, and 6to4 transition space.
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    let protocol_assignment = segments[0] == 0x2001 && segments[1] <= 0x01ff;
    let six_to_four = segments[0] == 0x2002;
    let newer_documentation = segments[0] == 0x3fff && segments[1] & 0xfff0 == 0;
    !(documentation || protocol_assignment || six_to_four || newer_documentation)
}
