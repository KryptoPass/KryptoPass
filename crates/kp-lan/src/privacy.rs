//! Network privacy policy checks and route-generation invalidation.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    net::IpAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[cfg(target_os = "linux")]
use std::fs;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Local preference for VPN enforcement.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VpnMode {
    /// Let the operating system choose and apply its current routes.
    #[default]
    FollowSystem,
    /// Require verifiable process-wide VPN coverage before Internet use.
    RequireVpn,
}

/// Policy selected by the local user. Peer invitations cannot change it.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPrivacyPolicy {
    /// Whether all Internet sockets must be covered by a verified VPN.
    pub vpn_mode: VpnMode,
    /// Explicit exception for local discovery and LAN traffic while VPN is required.
    pub allow_lan_exception: bool,
    /// Restrict WebRTC to TURN candidates and omit STUN servers.
    pub relay_only: bool,
}

/// Network operation checked against the local privacy policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkUse {
    /// Internet signaling, broker, DNS, STUN/TURN, or Internet DataChannel.
    Internet,
    /// Explicitly selected local signaling, mDNS, or LAN DataChannel.
    Lan,
}

/// Coarse VPN state suitable for a UI projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VpnStatus {
    /// The user did not require VPN coverage.
    NotRequired,
    /// This process cannot prove complete VPN coverage on the current platform.
    Unverifiable,
}

/// Coarse local route state suitable for a UI projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteStatus {
    /// The operating system currently chooses the route for each socket.
    FollowingSystem,
    /// Policy blocked the requested operation.
    Blocked,
    /// The route snapshot changed during an operation.
    Invalidated,
}

/// Errors intentionally carry no addresses, route table contents, or SDP.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum NetworkPolicyError {
    /// VPN coverage for this process and all required protocols is unverified.
    #[error("VPN coverage cannot be verified for this process")]
    VpnUnverifiable,
    /// LAN use is disabled while VPN is required.
    #[error("local network use requires an explicit exception")]
    LanExceptionRequired,
    /// Local direct networking is disabled by the global relay-only policy.
    #[error("the relay-only policy blocks direct local networking")]
    RelayRequired,
    /// The OS route snapshot changed while the operation was active.
    #[error("the selected network route changed")]
    RouteChanged,
    /// The route snapshot could not be read safely.
    #[error("the current network route could not be verified")]
    RouteSnapshotUnavailable,
    /// The local privacy settings changed during an operation.
    #[error("the local network privacy policy changed")]
    PolicyChanged,
}

/// Opaque route generation. It never exposes interface names or IP addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkRouteSnapshot {
    generation: u64,
}

impl NetworkRouteSnapshot {
    /// Create an opaque snapshot for policy-provider implementations and tests.
    #[must_use]
    pub const fn from_generation(generation: u64) -> Self {
        Self { generation }
    }

    /// Return the opaque generation number.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }

    /// Build a redacted generation from normalized route inputs. Input order
    /// is ignored; callers should include stable route fields only and omit
    /// counters such as packet use or reference counts.
    #[must_use]
    pub fn from_stable_inputs(
        interfaces: &[(String, IpAddr)],
        ipv4_routes: &[Vec<String>],
        ipv6_routes: &[Vec<String>],
        resolver_settings: &[String],
        selected_sources: &[Option<IpAddr>; 2],
    ) -> Self {
        let mut interfaces = interfaces.to_vec();
        interfaces.sort_unstable();
        interfaces.dedup();
        let mut ipv4_routes = ipv4_routes.to_vec();
        ipv4_routes.sort_unstable();
        ipv4_routes.dedup();
        let mut ipv6_routes = ipv6_routes.to_vec();
        ipv6_routes.sort_unstable();
        ipv6_routes.dedup();
        let mut resolver_settings = resolver_settings.to_vec();
        resolver_settings.sort_unstable();
        resolver_settings.dedup();

        let mut hasher = DefaultHasher::new();
        interfaces.hash(&mut hasher);
        ipv4_routes.hash(&mut hasher);
        ipv6_routes.hash(&mut hasher);
        resolver_settings.hash(&mut hasher);
        selected_sources.hash(&mut hasher);
        Self::from_generation(hasher.finish())
    }
}

/// Supplies a redacted snapshot of the system's selected route and local route inputs.
pub trait RouteSnapshotSource: Send + Sync {
    /// Read the current route generation without opening an Internet connection.
    fn snapshot(&self) -> Result<NetworkRouteSnapshot, NetworkPolicyError>;
}

/// Captures one allowed route generation and rejects later changes.
#[derive(Clone)]
pub struct NetworkPolicyGuard {
    policy: NetworkPrivacyPolicy,
    source: Arc<dyn RouteSnapshotSource>,
    baseline: Arc<Mutex<Option<NetworkRouteSnapshot>>>,
    invalidated: Arc<AtomicBool>,
}

impl std::fmt::Debug for NetworkPolicyGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkPolicyGuard")
            .field("policy", &self.policy)
            .field("route", &"[redacted]")
            .finish()
    }
}

impl NetworkPolicyGuard {
    /// Build a guard backed by the current OS route tables and interfaces.
    #[must_use]
    pub fn new(policy: NetworkPrivacyPolicy) -> Self {
        Self::with_snapshot_source(policy, Arc::new(SystemRouteSnapshot))
    }

    /// Build a guard with an injectable source, useful for route-change tests.
    #[must_use]
    pub fn with_snapshot_source(
        policy: NetworkPrivacyPolicy,
        source: Arc<dyn RouteSnapshotSource>,
    ) -> Self {
        Self {
            policy,
            source,
            baseline: Arc::new(Mutex::new(None)),
            invalidated: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Return the immutable local policy.
    #[must_use]
    pub const fn policy(&self) -> NetworkPrivacyPolicy {
        self.policy
    }

    /// Return the visible VPN state without inferring it from interface names.
    #[must_use]
    pub const fn vpn_status(&self) -> VpnStatus {
        match self.policy.vpn_mode {
            VpnMode::FollowSystem => VpnStatus::NotRequired,
            VpnMode::RequireVpn => VpnStatus::Unverifiable,
        }
    }

    /// Validate permission and pin the first permitted operation to its route.
    /// Strict VPN checks happen before the route source is read and before any
    /// DNS, broker, WSS, STUN, TURN, LAN, or mDNS operation can begin.
    pub fn preflight(&self, use_kind: NetworkUse) -> Result<(), NetworkPolicyError> {
        if self.policy.relay_only && use_kind == NetworkUse::Lan {
            return Err(NetworkPolicyError::RelayRequired);
        }
        match (self.policy.vpn_mode, use_kind) {
            (VpnMode::RequireVpn, NetworkUse::Internet) => {
                return Err(NetworkPolicyError::VpnUnverifiable);
            }
            (VpnMode::RequireVpn, NetworkUse::Lan) if !self.policy.allow_lan_exception => {
                return Err(NetworkPolicyError::LanExceptionRequired);
            }
            _ => {}
        }
        self.ensure_current()
    }

    /// Reject a route change after the first permitted operation.
    pub fn ensure_current(&self) -> Result<(), NetworkPolicyError> {
        if self.invalidated.load(Ordering::Acquire) {
            return Err(NetworkPolicyError::PolicyChanged);
        }
        let current = self.source.snapshot()?;
        let mut baseline = self
            .baseline
            .lock()
            .map_err(|_| NetworkPolicyError::RouteSnapshotUnavailable)?;
        match *baseline {
            Some(previous) if previous != current => Err(NetworkPolicyError::RouteChanged),
            Some(_) => Ok(()),
            None => {
                *baseline = Some(current);
                Ok(())
            }
        }
    }

    /// Invalidate all guard clones after a persisted policy change.
    pub fn invalidate(&self) {
        self.invalidated.store(true, Ordering::Release);
    }

    /// Wait until the route snapshot changes or can no longer be read.
    pub async fn wait_for_change(&self, interval: Duration) -> NetworkPolicyError {
        let poll = interval.max(Duration::from_millis(50));
        loop {
            tokio::time::sleep(poll).await;
            if let Err(error) = self.ensure_current() {
                return error;
            }
        }
    }

    /// Return whether the user requested TURN-only ICE.
    #[must_use]
    pub const fn relay_only(&self) -> bool {
        self.policy.relay_only
    }
}

struct SystemRouteSnapshot;

impl RouteSnapshotSource for SystemRouteSnapshot {
    fn snapshot(&self) -> Result<NetworkRouteSnapshot, NetworkPolicyError> {
        let interfaces =
            if_addrs::get_if_addrs().map_err(|_| NetworkPolicyError::RouteSnapshotUnavailable)?;
        let interface_addresses: Vec<_> = interfaces
            .into_iter()
            .map(|interface| {
                let ip = interface.ip();
                (interface.name, ip)
            })
            .collect();
        let mut ipv4_routes = Vec::new();
        let mut ipv6_routes = Vec::new();
        let mut resolvers = Vec::new();

        // Linux exposes route and resolver configuration without privileged
        // APIs. Parse and sort stable fields only; /proc counters change during
        // ordinary traffic and must not churn an established session guard.
        #[cfg(target_os = "linux")]
        read_linux_routes(&mut ipv4_routes, &mut ipv6_routes)?;
        #[cfg(target_os = "linux")]
        read_resolver_config(&mut resolvers)?;

        // Also capture the source selected by the OS for IPv4 and IPv6 default
        // route lookups. UDP connect performs a local route selection only.
        let selected_sources = [
            route_source_address(IpAddr::V4(std::net::Ipv4Addr::new(198, 51, 100, 1))).ok(),
            route_source_address(IpAddr::V6(std::net::Ipv6Addr::new(
                0x2001, 0xdb8, 0, 0, 0, 0, 0, 1,
            )))
            .ok(),
        ];

        Ok(NetworkRouteSnapshot::from_stable_inputs(
            &interface_addresses,
            &ipv4_routes,
            &ipv6_routes,
            &resolvers,
            &selected_sources,
        ))
    }
}

#[cfg(target_os = "linux")]
fn read_linux_routes(
    ipv4_routes: &mut Vec<Vec<String>>,
    ipv6_routes: &mut Vec<Vec<String>>,
) -> Result<(), NetworkPolicyError> {
    let v4_contents = fs::read_to_string("/proc/net/route")
        .map_err(|_| NetworkPolicyError::RouteSnapshotUnavailable)?;
    for line in v4_contents.lines().skip(1) {
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        if fields.is_empty() {
            continue;
        }
        if fields.len() < 11 {
            return Err(NetworkPolicyError::RouteSnapshotUnavailable);
        }
        // Interface, destination, gateway, flags, metric, mask, and MTU are
        // stable routing inputs. RefCnt, Use, Window, and IRTT are volatile or
        // unrelated to route selection.
        ipv4_routes.push(
            [0, 1, 2, 3, 6, 7, 8]
                .into_iter()
                .map(|index| fields[index].to_owned())
                .collect(),
        );
    }
    let v6_contents = fs::read_to_string("/proc/net/ipv6_route")
        .map_err(|_| NetworkPolicyError::RouteSnapshotUnavailable)?;
    for line in v6_contents.lines() {
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        if fields.is_empty() {
            continue;
        }
        if fields.len() < 10 {
            return Err(NetworkPolicyError::RouteSnapshotUnavailable);
        }
        // RefCnt and Use are omitted; route row ordering is not stable.
        ipv6_routes.push(
            [0, 1, 2, 3, 4, 5, 8, 9]
                .into_iter()
                .map(|index| fields[index].to_owned())
                .collect(),
        );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_resolver_config(resolvers: &mut Vec<String>) -> Result<(), NetworkPolicyError> {
    let contents = fs::read_to_string("/etc/resolv.conf")
        .map_err(|_| NetworkPolicyError::RouteSnapshotUnavailable)?;
    resolvers.extend(contents.lines().filter_map(|line| {
        let line = line.split('#').next()?.trim();
        let (kind, value) = line.split_once(char::is_whitespace)?;
        (kind == "nameserver" || kind == "search" || kind == "domain").then(|| {
            format!(
                "{kind} {}",
                value.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
    }));
    Ok(())
}

/// Return the local source address chosen by the OS for a route destination.
pub(crate) fn route_source_address(destination: IpAddr) -> Result<IpAddr, NetworkPolicyError> {
    let socket = std::net::UdpSocket::bind(if destination.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })
    .map_err(|_| NetworkPolicyError::RouteSnapshotUnavailable)?;
    socket
        .connect((destination, 9))
        .map_err(|_| NetworkPolicyError::RouteSnapshotUnavailable)?;
    socket
        .local_addr()
        .map(|address| address.ip())
        .map_err(|_| NetworkPolicyError::RouteSnapshotUnavailable)
}
