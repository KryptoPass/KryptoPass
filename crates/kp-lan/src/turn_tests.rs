use super::*;
use crate::{
    LanError,
    internet::{
        IceServerConfig, InternetIceConfig, InternetIceTransportPolicy, config_from_sources,
    },
};
use serde_json::{Value, json};

const TEST_ORIGIN: &str = "https://signal.kryptopass.org";
const TEST_NOW: u64 = 1_792_340_000;
const TURN_UDP_3478: &str = "turn:turn.cloudflare.com:3478?transport=udp";
const TURN_UDP_443: &str = "turn:turn.cloudflare.com:443?transport=udp";
const TURN_TCP_3478: &str = "turn:turn.cloudflare.com:3478?transport=tcp";
const TURNS_TCP_443: &str = "turns:turn.cloudflare.com:443?transport=tcp";

#[test]
fn signing_message_matches_shared_desktop_android_vector_and_binds_origin() {
    let device = std::array::from_fn(|index| index as u8);
    let nonce = std::array::from_fn(|index| 0xa0 + index as u8);
    let actual = match signing_message(TEST_ORIGIN, &device, 1_792_340_000, &nonce) {
        Ok(message) => message,
        Err(_) => panic!("canonical request rejected a bounded origin"),
    };
    let expected_hex = concat!(
        "4b727970746f50617373205455524e2063726564656e7469616c7320763100",
        "0000001d68747470733a2f2f7369676e616c2e6b727970746f706173732e6f7267",
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        "000000006ad4f020",
        "a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebf"
    );
    assert_eq!(lower_hex(&actual), expected_hex);

    let other_origin =
        match signing_message("https://other.example", &device, 1_792_340_000, &nonce) {
            Ok(message) => message,
            Err(_) => panic!("canonical request rejected a bounded origin"),
        };
    assert_ne!(actual, other_origin);
}

#[test]
fn broker_origin_requires_canonical_https_and_allows_http_only_for_loopback_tests() {
    assert!(validate_origin(TEST_ORIGIN, false).is_ok());
    assert!(validate_origin("https://signal.kryptopass.org:8443", false).is_ok());
    for invalid in [
        "",
        "https://signal.kryptopass.org/",
        "https://signal.kryptopass.org/path",
        "https://user@signal.kryptopass.org",
        "https://signal.kryptopass.org?query=1",
        "https://signal.kryptopass.org#fragment",
        "https://Signal.KryptoPass.org",
        "http://signal.kryptopass.org",
        "http://192.0.2.1:8080",
    ] {
        assert!(validate_origin(invalid, false).is_err());
    }

    assert!(validate_origin("http://127.0.0.1:8080", true).is_ok());
    assert!(validate_origin("http://[::1]:8080", true).is_ok());
    assert!(validate_origin("http://127.0.0.1:8080", false).is_err());
}

#[test]
fn broker_response_accepts_supported_udp_and_filters_supported_tcp_transports() {
    let body = response(
        TEST_NOW + 3600,
        vec![server(
            vec![TURN_TCP_3478, TURN_UDP_443, TURNS_TCP_443, TURN_UDP_3478],
            "temporary-user",
            "temporary-credential",
        )],
    );

    let (servers, cache_until) = match parse_response(&body, TEST_NOW) {
        Ok(parsed) => parsed,
        Err(_) => panic!("valid bounded TURN response was rejected"),
    };
    assert_eq!(servers.len(), 1);
    assert_eq!(
        servers[0].urls(),
        &[TURN_UDP_443.to_owned(), TURN_UDP_3478.to_owned()]
    );
    assert_eq!(cache_until, TEST_NOW + 3600 - CACHE_SKEW_SECONDS);
}

#[test]
fn broker_response_enforces_expiry_window_and_cache_skew() {
    for expiry in [TEST_NOW + 30, TEST_NOW + 3661, u64::MAX] {
        assert!(parse_response(&response(expiry, vec![valid_server()]), TEST_NOW).is_err());
    }

    for expiry in [TEST_NOW + 31, TEST_NOW + 3660] {
        assert!(parse_response(&response(expiry, vec![valid_server()]), TEST_NOW).is_ok());
    }

    assert!(parse_response(&response(1, vec![valid_server()]), u64::MAX - 10).is_err());
}

#[test]
fn broker_response_requires_exact_fields_and_rejects_duplicates() {
    let invalid = [
        r#"{"v":2,"expires_at":1792343600,"ice_servers":[{"urls":["turn:turn.cloudflare.com:3478?transport=udp"],"username":"u","credential":"c"}]}"#,
        r#"{"v":1,"expires_at":1792343600,"ice_servers":[],"extra":true}"#,
        r#"{"v":1,"expires_at":1792343600,"ice_servers":[{"urls":["turn:turn.cloudflare.com:3478?transport=udp"],"username":"u","credential":"c","extra":true}]}"#,
        r#"{"v":1,"v":1,"expires_at":1792343600,"ice_servers":[{"urls":["turn:turn.cloudflare.com:3478?transport=udp"],"username":"u","credential":"c"}]}"#,
        r#"{"v":1,"expires_at":1792343600,"expires_at":1792343600,"ice_servers":[{"urls":["turn:turn.cloudflare.com:3478?transport=udp"],"username":"u","credential":"c"}]}"#,
    ];

    for body in invalid {
        assert!(parse_response(body.as_bytes(), TEST_NOW).is_err());
    }
}

#[test]
fn broker_response_rejects_untrusted_urls_and_bad_credentials() {
    let invalid = [
        server(vec!["turn:attacker.example:3478?transport=udp"], "u", "c"),
        server(
            vec!["turn:turn.cloudflare.com:3479?transport=udp"],
            "u",
            "c",
        ),
        server(vec!["turn:turn.cloudflare.com:3478"], "u", "c"),
        server(
            vec!["turns:turn.cloudflare.com:443?transport=udp"],
            "u",
            "c",
        ),
        server(vec![TURN_UDP_3478], "", "c"),
        server(vec![TURN_UDP_3478], "u", ""),
        server(vec![TURN_UDP_3478], "user\nname", "c"),
        server(vec![TURN_UDP_3478], "u", "credential\u{7f}"),
        server(vec![TURN_UDP_3478], &"u".repeat(513), &"c".repeat(512)),
        server(vec![], "u", "c"),
        server(vec!["stun:stun.cloudflare.com:3478"], "", ""),
    ];

    for invalid_server in invalid {
        assert!(
            parse_response(&response(TEST_NOW + 3600, vec![invalid_server]), TEST_NOW).is_err()
        );
    }

    let too_many_per_server = server(
        vec![
            TURN_UDP_3478,
            TURN_UDP_443,
            TURN_TCP_3478,
            TURNS_TCP_443,
            TURN_UDP_3478,
        ],
        "u",
        "c",
    );
    assert!(
        parse_response(
            &response(TEST_NOW + 3600, vec![too_many_per_server]),
            TEST_NOW
        )
        .is_err()
    );
}

#[test]
fn broker_response_limits_servers_total_urls_and_body_size() {
    let four_servers = (0..MAX_SERVERS).map(|_| valid_server_all_urls()).collect();
    let bytes = response(TEST_NOW + 3600, four_servers);
    let (servers, _) = match parse_response(&bytes, TEST_NOW) {
        Ok(parsed) => parsed,
        Err(_) => panic!("inclusive four server response was rejected"),
    };
    assert_eq!(servers.len(), MAX_SERVERS);
    assert_eq!(
        servers
            .iter()
            .map(|server| server.urls().len())
            .sum::<usize>(),
        MAX_SERVERS * 2
    );

    let five_servers = (0..=MAX_SERVERS).map(|_| valid_server_all_urls()).collect();
    assert!(parse_response(&response(TEST_NOW + 3600, five_servers), TEST_NOW).is_err());
    assert!(parse_response(&vec![b' '; MAX_RESPONSE_BYTES + 1], TEST_NOW).is_err());
}

#[test]
fn response_headers_require_json_and_no_store() {
    use reqwest::header::HeaderValue;

    assert!(is_json_response(Some(&HeaderValue::from_static(
        "application/json"
    ))));
    assert!(is_json_response(Some(&HeaderValue::from_static(
        "application/json; charset=utf-8"
    ))));
    assert!(!is_json_response(Some(&HeaderValue::from_static(
        "text/plain"
    ))));
    assert!(has_no_store(Some(&HeaderValue::from_static(
        "private, no-store"
    ))));
    assert!(!has_no_store(Some(&HeaderValue::from_static("private"))));
}

#[test]
fn manual_ice_configuration_has_priority_over_broker_and_is_validated() {
    let manual = InternetIceConfig::new(vec![IceServerConfig::stun("stun:manual.example:3478")]);
    let (config, policy) =
        match config_from_sources(Some(manual), false, Some(Err(LanError::TurnBroker))) {
            Ok(result) => result,
            Err(_) => panic!("manual configuration did not win over broker failure"),
        };
    assert!(policy == InternetIceTransportPolicy::All);
    assert_eq!(
        config.servers()[0].urls(),
        &["stun:manual.example:3478".to_owned()]
    );

    let invalid_manual = InternetIceConfig::new(vec![IceServerConfig::with_urls(
        vec![
            "stun:manual.example:3478".to_owned(),
            TURN_UDP_3478.to_owned(),
        ],
        "user",
        "credential",
    )]);
    assert!(
        config_from_sources(
            Some(invalid_manual),
            false,
            Some(Ok(vec![IceServerConfig::turn(
                TURN_UDP_3478,
                "user",
                "credential",
            )])),
        )
        .is_err()
    );
}

#[test]
fn normal_mode_falls_back_to_stun_when_broker_is_missing_or_fails() {
    for broker in [None, Some(Err(LanError::TurnBroker))] {
        let (config, policy) = match config_from_sources(None, false, broker) {
            Ok(result) => result,
            Err(_) => panic!("normal mode lost the default STUN path"),
        };
        assert!(policy == InternetIceTransportPolicy::All);
        assert_eq!(config.servers().len(), 1);
        assert_eq!(
            config.servers()[0].urls(),
            &["stun:stun.cloudflare.com:3478".to_owned()]
        );
    }
}

#[test]
fn relay_only_requires_supported_udp_turn_without_fallback() {
    assert!(matches!(
        config_from_sources(None, true, None),
        Err(LanError::TurnBroker)
    ));
    assert!(matches!(
        config_from_sources(None, true, Some(Err(LanError::TurnBroker))),
        Err(LanError::TurnBroker)
    ));

    let manual_stun =
        InternetIceConfig::new(vec![IceServerConfig::stun("stun:manual.example:3478")]);
    assert!(matches!(
        config_from_sources(Some(manual_stun), true, None),
        Err(LanError::InvalidIceConfig)
    ));

    let tcp_turn = IceServerConfig::turn(TURN_TCP_3478, "user", "credential");
    assert!(matches!(
        config_from_sources(None, true, Some(Ok(vec![tcp_turn]))),
        Err(LanError::TurnBroker)
    ));

    let manual_turn = InternetIceConfig::new(vec![IceServerConfig::turn(
        TURN_UDP_3478,
        "user",
        "credential",
    )]);
    let (config, policy) = match config_from_sources(Some(manual_turn), true, None) {
        Ok(result) => result,
        Err(_) => panic!("manual UDP TURN did not satisfy relay-only policy"),
    };
    assert!(policy == InternetIceTransportPolicy::Relay);
    assert_eq!(config.servers()[0].urls(), &[TURN_UDP_3478.to_owned()]);
}

#[test]
fn broker_credentials_are_augmented_with_stun_only_in_normal_mode() {
    let (config, policy) = match config_from_sources(
        None,
        false,
        Some(Ok(vec![IceServerConfig::with_urls(
            vec![TURN_UDP_3478.to_owned(), TURN_UDP_443.to_owned()],
            "temporary-user",
            "temporary-credential",
        )])),
    ) {
        Ok(result) => result,
        Err(_) => panic!("valid broker credentials did not produce ICE config"),
    };

    assert!(policy == InternetIceTransportPolicy::All);
    assert_eq!(
        config.servers()[0].urls(),
        &[
            "stun:stun.cloudflare.com:3478".to_owned(),
            TURN_UDP_3478.to_owned(),
            TURN_UDP_443.to_owned(),
        ]
    );
}

fn valid_server() -> Value {
    server(
        vec![TURN_UDP_3478, TURN_UDP_443],
        "temporary-user",
        "temporary-credential",
    )
}

fn valid_server_all_urls() -> Value {
    server(
        vec![TURN_UDP_3478, TURN_UDP_443, TURN_TCP_3478, TURNS_TCP_443],
        "temporary-user",
        "temporary-credential",
    )
}

fn server(urls: Vec<&str>, username: &str, credential: &str) -> Value {
    json!({
        "urls": urls,
        "username": username,
        "credential": credential,
    })
}

fn response(expires_at: u64, ice_servers: Vec<Value>) -> Vec<u8> {
    match serde_json::to_vec(&json!({
        "v": 1,
        "expires_at": expires_at,
        "ice_servers": ice_servers,
    })) {
        Ok(bytes) => bytes,
        Err(_) => panic!("serialize fixed response fixture"),
    }
}
