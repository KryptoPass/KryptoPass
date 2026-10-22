use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use std::{net::IpAddr, str::FromStr};

use kryptopass_lan::{
    InternetIceConfig, InternetIceTransportPolicy, NetworkPolicyError, NetworkPolicyGuard,
    NetworkPrivacyPolicy, NetworkRouteSnapshot, NetworkUse, RelayHost, RelayRoute,
    RouteSnapshotSource, VpnMode, VpnStatus, internet_udp_bind_addresses,
    select_local_bind_address,
};

struct FakeRouteSnapshot {
    generation: AtomicU64,
    calls: AtomicUsize,
    unavailable: AtomicBool,
}

impl FakeRouteSnapshot {
    fn new(generation: u64) -> Self {
        Self {
            generation: AtomicU64::new(generation),
            calls: AtomicUsize::new(0),
            unavailable: AtomicBool::new(false),
        }
    }
}

impl RouteSnapshotSource for FakeRouteSnapshot {
    fn snapshot(&self) -> Result<NetworkRouteSnapshot, NetworkPolicyError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(NetworkPolicyError::RouteSnapshotUnavailable);
        }
        Ok(NetworkRouteSnapshot::from_generation(
            self.generation.load(Ordering::SeqCst),
        ))
    }
}

#[test]
fn network_privacy_follow_system_pins_one_route_generation_and_fails_closed_on_change() {
    let source = Arc::new(FakeRouteSnapshot::new(7));
    let guard =
        NetworkPolicyGuard::with_snapshot_source(NetworkPrivacyPolicy::default(), source.clone());

    assert_eq!(guard.vpn_status(), VpnStatus::NotRequired);
    assert_eq!(guard.preflight(NetworkUse::Internet), Ok(()));
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);

    source.generation.store(8, Ordering::SeqCst);
    assert_eq!(
        guard.ensure_current(),
        Err(NetworkPolicyError::RouteChanged)
    );
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn network_route_generation_ignores_enumeration_order_but_cancels_on_selected_route_change()
-> Result<(), Box<dyn std::error::Error>> {
    let interfaces = vec![
        ("wlan0".to_owned(), IpAddr::from_str("192.168.10.8")?),
        ("eth0".to_owned(), IpAddr::from_str("10.4.0.8")?),
    ];
    let ipv4_routes = vec![
        vec![
            "eth0".to_owned(),
            "00000000".to_owned(),
            "0100040A".to_owned(),
            "0003".to_owned(),
            "0".to_owned(),
            "0".to_owned(),
            "100".to_owned(),
            "0".to_owned(),
            "1500".to_owned(),
        ],
        vec![
            "wlan0".to_owned(),
            "00000AC0".to_owned(),
            "00000000".to_owned(),
            "0001".to_owned(),
            "0".to_owned(),
            "0".to_owned(),
            "200".to_owned(),
            "0".to_owned(),
            "1500".to_owned(),
        ],
    ];
    let ipv6_routes = vec![vec![
        "00000000000000000000000000000000".to_owned(),
        "00".to_owned(),
        "fe800000000000000000000000000001".to_owned(),
        "00".to_owned(),
        "wlan0".to_owned(),
        "00000000".to_owned(),
        "00000000".to_owned(),
        "00000000".to_owned(),
    ]];
    let resolver_settings = vec![
        "nameserver 192.168.10.1".to_owned(),
        "search lan".to_owned(),
    ];
    let selected_sources = [
        Some(IpAddr::from_str("10.4.0.8")?),
        Some(IpAddr::from_str("fd00::8")?),
    ];

    let canonical = NetworkRouteSnapshot::from_stable_inputs(
        &interfaces,
        &ipv4_routes,
        &ipv6_routes,
        &resolver_settings,
        &selected_sources,
    );
    let reordered = NetworkRouteSnapshot::from_stable_inputs(
        &interfaces.iter().cloned().rev().collect::<Vec<_>>(),
        &ipv4_routes.iter().cloned().rev().collect::<Vec<_>>(),
        &ipv6_routes.iter().cloned().rev().collect::<Vec<_>>(),
        &resolver_settings.iter().cloned().rev().collect::<Vec<_>>(),
        &selected_sources,
    );
    assert_eq!(
        canonical, reordered,
        "enumeration order is not a route change"
    );

    let changed_sources = [Some(IpAddr::from_str("192.168.10.8")?), selected_sources[1]];
    let changed = NetworkRouteSnapshot::from_stable_inputs(
        &interfaces,
        &ipv4_routes,
        &ipv6_routes,
        &resolver_settings,
        &changed_sources,
    );
    assert_ne!(
        canonical, changed,
        "a selected source route change is material"
    );

    let source = Arc::new(FakeRouteSnapshot::new(canonical.generation()));
    let guard =
        NetworkPolicyGuard::with_snapshot_source(NetworkPrivacyPolicy::default(), source.clone());
    assert_eq!(guard.preflight(NetworkUse::Internet), Ok(()));
    source
        .generation
        .store(changed.generation(), Ordering::SeqCst);
    assert_eq!(
        guard.ensure_current(),
        Err(NetworkPolicyError::RouteChanged)
    );
    Ok(())
}

#[test]
fn network_privacy_require_vpn_blocks_before_route_or_internet_side_effects() {
    let source = Arc::new(FakeRouteSnapshot::new(1));
    let guard = NetworkPolicyGuard::with_snapshot_source(
        NetworkPrivacyPolicy {
            vpn_mode: VpnMode::RequireVpn,
            ..NetworkPrivacyPolicy::default()
        },
        source.clone(),
    );

    assert_eq!(guard.vpn_status(), VpnStatus::Unverifiable);
    assert_eq!(
        guard.preflight(NetworkUse::Internet),
        Err(NetworkPolicyError::VpnUnverifiable)
    );
    assert_eq!(
        guard.preflight(NetworkUse::Lan),
        Err(NetworkPolicyError::LanExceptionRequired)
    );
    assert_eq!(
        source.calls.load(Ordering::SeqCst),
        0,
        "strict VPN checks finish before reading a route or starting any network operation"
    );
}

#[tokio::test]
async fn network_privacy_require_vpn_rejects_before_local_relay_socket_is_contacted()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = listener.local_addr()?;
    let route = RelayRoute::new(&format!("ws://{endpoint}"), [0x42; 32])?;
    let source = Arc::new(FakeRouteSnapshot::new(10));
    let guard = NetworkPolicyGuard::with_snapshot_source(
        NetworkPrivacyPolicy {
            vpn_mode: VpnMode::RequireVpn,
            ..NetworkPrivacyPolicy::default()
        },
        source.clone(),
    );

    let result = RelayHost::connect_with_privacy(
        route,
        InternetIceConfig::default(),
        InternetIceTransportPolicy::All,
        guard,
    )
    .await;
    assert!(matches!(
        result,
        Err(kryptopass_lan::LanError::NetworkPolicy)
    ));
    assert_eq!(source.calls.load(Ordering::SeqCst), 0);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err(),
        "strict VPN rejection must occur before the WebSocket socket connects"
    );
    Ok(())
}

#[test]
fn network_privacy_lan_exception_is_opt_in_and_never_unblocks_internet() {
    let source = Arc::new(FakeRouteSnapshot::new(3));
    let guard = NetworkPolicyGuard::with_snapshot_source(
        NetworkPrivacyPolicy {
            vpn_mode: VpnMode::RequireVpn,
            allow_lan_exception: true,
            ..NetworkPrivacyPolicy::default()
        },
        source.clone(),
    );

    assert_eq!(guard.preflight(NetworkUse::Lan), Ok(()));
    assert_eq!(
        guard.preflight(NetworkUse::Internet),
        Err(NetworkPolicyError::VpnUnverifiable)
    );
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);

    let default_source = Arc::new(FakeRouteSnapshot::new(4));
    let default_guard = NetworkPolicyGuard::with_snapshot_source(
        NetworkPrivacyPolicy {
            vpn_mode: VpnMode::RequireVpn,
            ..NetworkPrivacyPolicy::default()
        },
        default_source.clone(),
    );
    assert_eq!(
        default_guard.preflight(NetworkUse::Lan),
        Err(NetworkPolicyError::LanExceptionRequired)
    );
    assert_eq!(default_source.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn relay_only_policy_blocks_manual_lan_before_reading_route_state() {
    let source = Arc::new(FakeRouteSnapshot::new(6));
    let guard = NetworkPolicyGuard::with_snapshot_source(
        NetworkPrivacyPolicy {
            relay_only: true,
            ..NetworkPrivacyPolicy::default()
        },
        source.clone(),
    );

    assert_eq!(
        guard.preflight(NetworkUse::Lan),
        Err(NetworkPolicyError::RelayRequired),
        "TURN-only mode must not open a direct LAN route"
    );
    assert_eq!(
        source.calls.load(Ordering::SeqCst),
        0,
        "the blocked LAN path must not even read a route snapshot"
    );
    assert_eq!(
        guard.preflight(NetworkUse::Internet),
        Ok(()),
        "relay-only Internet use remains available under Follow System"
    );
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn network_privacy_unavailable_route_snapshot_does_not_allow_follow_system_use() {
    let source = Arc::new(FakeRouteSnapshot::new(5));
    source.unavailable.store(true, Ordering::SeqCst);
    let guard =
        NetworkPolicyGuard::with_snapshot_source(NetworkPrivacyPolicy::default(), source.clone());

    assert_eq!(
        guard.preflight(NetworkUse::Internet),
        Err(NetworkPolicyError::RouteSnapshotUnavailable)
    );
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn internet_udp_bind_addresses_contains_only_the_selected_route_address()
-> Result<(), Box<dyn std::error::Error>> {
    for route in ["8.8.8.8", "2001:4860:4860::8888"] {
        let selected = IpAddr::from_str(route)?;
        assert_eq!(
            internet_udp_bind_addresses(selected)?,
            [std::net::SocketAddr::new(selected, 0)],
            "the transport bind list must not enumerate other interfaces"
        );
    }
    Ok(())
}

#[test]
fn local_bind_selection_uses_only_assigned_explicit_or_os_selected_addresses()
-> Result<(), Box<dyn std::error::Error>> {
    let configured_wifi = IpAddr::from_str("192.168.10.8")?;
    let another_wifi = IpAddr::from_str("192.168.10.9")?;
    let old_vpn = IpAddr::from_str("10.2.0.2")?;
    let current_vpn = IpAddr::from_str("10.8.0.2")?;
    let unrelated = IpAddr::from_str("172.16.20.8")?;

    assert_eq!(
        select_local_bind_address(
            Some(configured_wifi),
            &[configured_wifi, another_wifi],
            Some(another_wifi),
        ),
        Some(configured_wifi),
        "a configured local address remains preferred while assigned"
    );
    assert_eq!(
        select_local_bind_address(Some(old_vpn), &[configured_wifi], Some(configured_wifi)),
        Some(configured_wifi),
        "an unavailable old VPN address follows the OS-selected Wi-Fi address"
    );
    assert_eq!(
        select_local_bind_address(Some(configured_wifi), &[current_vpn], Some(current_vpn)),
        Some(current_vpn),
        "an unavailable old Wi-Fi address follows the newly OS-selected VPN address"
    );
    assert_eq!(
        select_local_bind_address(None, &[configured_wifi, another_wifi], None),
        None,
        "without an OS-selected source, the helper does not choose an arbitrary interface"
    );
    assert_eq!(
        select_local_bind_address(None, &[configured_wifi], Some(unrelated)),
        None,
        "an OS-selected address absent from the assigned set is rejected"
    );
    Ok(())
}
