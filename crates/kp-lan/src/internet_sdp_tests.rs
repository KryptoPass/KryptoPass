use super::{CandidatePolicy, validate_sdp};

const DATA_CHANNEL_MEDIA: &str = "m=application 9 UDP/DTLS/SCTP webrtc-datachannel";
const INTERNET_HOST_CANDIDATE: &str = "a=candidate:1 1 UDP 2130706431 8.8.8.8 5000 typ host";
const PRIVATE_LAN_CANDIDATE: &str = "a=candidate:1 1 UDP 2130706431 192.168.1.20 5000 typ host";

#[test]
fn internet_accepts_loopback_and_unspecified_origin_metadata_with_udp_candidate() {
    let policy = CandidatePolicy::Internet {
        allow_loopback: false,
    };
    let accepted_origins = ["IP4 127.0.0.1", "IP6 ::1", "IP4 0.0.0.0", "IP6 ::"];

    for origin in accepted_origins {
        let offer = sdp(
            origin,
            "IP4 0.0.0.0",
            DATA_CHANNEL_MEDIA,
            INTERNET_HOST_CANDIDATE,
        );
        assert!(
            validate_sdp(&offer, policy).is_ok(),
            "Internet SDP should allow session-origin metadata {origin:?}"
        );
    }
}

#[test]
fn internet_accepts_redacted_unspecified_related_addresses_on_srflx_candidates() {
    let policy = CandidatePolicy::Internet {
        allow_loopback: false,
    };
    let redacted_related_addresses = [("0.0.0.0", 0), ("::", 9)];

    for (address, port) in redacted_related_addresses {
        let candidate = format!(
            "a=candidate:2 1 UDP 1686052607 8.8.8.8 5001 typ srflx raddr {address} rport {port}"
        );
        let offer = sdp(
            "IP4 127.0.0.1",
            "IP4 0.0.0.0",
            DATA_CHANNEL_MEDIA,
            &candidate,
        );
        assert!(
            validate_sdp(&offer, policy).is_ok(),
            "redacted srflx metadata {address}:{port} should be accepted"
        );
    }
}

#[test]
fn internet_rejects_loopback_tcp_multicast_and_documentation_candidates() {
    let policy = CandidatePolicy::Internet {
        allow_loopback: false,
    };
    let invalid_candidates = [
        "a=candidate:1 1 UDP 2130706431 127.0.0.1 5000 typ host",
        "a=candidate:1 1 UDP 2130706431 224.0.0.1 5000 typ host",
        "a=candidate:1 1 UDP 2130706431 203.0.113.9 5000 typ host",
        "a=candidate:1 1 UDP 2130706431 2001:db8::1 5000 typ host",
        "a=candidate:1 1 TCP 2130706431 8.8.8.8 5000 typ host tcptype active",
    ];

    for candidate in invalid_candidates {
        let offer = sdp(
            "IP4 127.0.0.1",
            "IP4 0.0.0.0",
            DATA_CHANNEL_MEDIA,
            candidate,
        );
        assert!(
            validate_sdp(&offer, policy).is_err(),
            "Internet SDP should reject candidate {candidate:?}"
        );
    }
}

#[test]
fn internet_rejects_multicast_origins_loopback_connections_and_non_data_media() {
    let policy = CandidatePolicy::Internet {
        allow_loopback: false,
    };
    let multicast_origin = sdp(
        "IP4 224.0.0.1",
        "IP4 0.0.0.0",
        DATA_CHANNEL_MEDIA,
        INTERNET_HOST_CANDIDATE,
    );
    assert!(validate_sdp(&multicast_origin, policy).is_err());

    let loopback_connection = sdp(
        "IP4 127.0.0.1",
        "IP4 127.0.0.1",
        DATA_CHANNEL_MEDIA,
        INTERNET_HOST_CANDIDATE,
    );
    assert!(validate_sdp(&loopback_connection, policy).is_err());

    for media in [
        "m=audio 9 UDP/TLS/RTP/SAVPF 111",
        "m=video 9 UDP/TLS/RTP/SAVPF 96",
    ] {
        let offer = sdp(
            "IP4 127.0.0.1",
            "IP4 0.0.0.0",
            media,
            INTERNET_HOST_CANDIDATE,
        );
        assert!(
            validate_sdp(&offer, policy).is_err(),
            "Internet transport should reject non-DataChannel media {media:?}"
        );
    }
}

#[test]
fn lan_candidate_policy_still_accepts_private_hosts_and_rejects_public_hosts() {
    let policy = CandidatePolicy::Lan;
    let private_offer = sdp(
        "IP4 192.168.1.10",
        "IP4 192.168.1.10",
        DATA_CHANNEL_MEDIA,
        PRIVATE_LAN_CANDIDATE,
    );
    assert!(validate_sdp(&private_offer, policy).is_ok());

    let public_offer = sdp(
        "IP4 192.168.1.10",
        "IP4 192.168.1.10",
        DATA_CHANNEL_MEDIA,
        INTERNET_HOST_CANDIDATE,
    );
    assert!(validate_sdp(&public_offer, policy).is_err());
}

fn sdp(origin: &str, connection: &str, media: &str, candidate: &str) -> String {
    format!(
        "v=0\r\no=- 0 0 IN {origin}\r\ns=-\r\nt=0 0\r\nc=IN {connection}\r\n{media}\r\n{candidate}\r\n"
    )
}
