#![allow(
    clippy::expect_used,
    reason = "The isolated filesystem fixture uses expect for setup and local assertions."
)]

#[cfg(target_os = "linux")]
mod linux {
    use kryptopass_identity::{DeviceIdentity, SqliteTrustStore};
    use kryptopass_lan::RelayRoute;
    use kryptopass_peers::{PeerError, PeerHost};
    use std::{
        fs::{self, DirBuilder},
        os::unix::fs::DirBuilderExt,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    const ROOM: [u8; 32] = [0x3c; 32];

    struct PrivateTempProfile(PathBuf);

    impl PrivateTempProfile {
        fn new() -> Self {
            static NEXT_PROFILE: AtomicU64 = AtomicU64::new(0);
            loop {
                let nonce = NEXT_PROFILE.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "kryptopass-internet-routes-{}-{nonce}",
                    std::process::id()
                ));
                let mut builder = DirBuilder::new();
                builder.mode(0o700);
                match builder.create(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create private temporary profile: {error}"),
                }
            }
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for PrivateTempProfile {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn unknown_peer_cannot_create_a_relay_route_or_authorization() {
        let profile = PrivateTempProfile::new();
        let host = PeerHost::ephemeral_test_profile(profile.path())
            .expect("create isolated in-memory identity and trust profile");
        let unknown_peer = DeviceIdentity::generate().expect("generate in-memory peer identity");
        let route = route();

        assert!(matches!(
            host.set_internet_route(unknown_peer.device_id(), &route),
            Err(PeerError::Authentication)
        ));
        assert!(host.trusted_peers().expect("read trust list").is_empty());
        assert!(host.internet_routes().expect("read route list").is_empty());
        assert!(
            !profile.path().join("internet-routes-v1.json").exists(),
            "rejecting an unknown peer must not create route metadata"
        );
    }

    #[test]
    fn trusted_peer_route_is_persisted_and_bound_to_the_local_identity() {
        let profile = PrivateTempProfile::new();
        let host = PeerHost::ephemeral_test_profile(profile.path())
            .expect("create isolated in-memory identity and trust profile");
        let peer = DeviceIdentity::generate().expect("generate in-memory peer identity");
        authorize_peer(profile.path(), &host, &peer);
        let route = route();

        host.set_internet_route(peer.device_id(), &route)
            .expect("save route for an authorized peer");

        let routes = host.internet_routes().expect("reload saved public routes");
        let restored = routes
            .get(&peer.device_id())
            .expect("authorized peer route survives a metadata reload");
        assert_eq!(restored.relay_origin(), "wss://signal.kryptopass.org");
        assert_eq!(restored.room(), ROOM);
        assert!(host.lookup(peer.device_id()).is_ok());

        let path = profile.path().join("internet-routes-v1.json");
        let mut value: serde_json::Value = serde_json::from_slice(
            &fs::read(&path).expect("read public route metadata for binding check"),
        )
        .expect("route metadata is valid JSON");
        let local_id = host.public_identity().device_id.to_string();
        assert_eq!(
            value["local"].as_str(),
            Some(local_id.as_str()),
            "routing metadata records the identity that owns the trust store"
        );

        let other_local = DeviceIdentity::generate().expect("generate another in-memory identity");
        value["local"] = serde_json::Value::String(other_local.device_id().to_string());
        fs::write(
            &path,
            serde_json::to_vec(&value).expect("serialize re-bound metadata"),
        )
        .expect("write route metadata with a mismatched local identity");
        assert!(matches!(host.internet_routes(), Err(PeerError::Profile)));

        fs::write(&path, b"{").expect("corrupt route metadata for the fail-closed check");
        assert!(matches!(host.internet_routes(), Err(PeerError::Profile)));
    }

    #[test]
    fn forgetting_peer_removes_its_route_and_trust() {
        let profile = PrivateTempProfile::new();
        let host = PeerHost::ephemeral_test_profile(profile.path())
            .expect("create isolated in-memory identity and trust profile");
        let peer = DeviceIdentity::generate().expect("generate in-memory peer identity");
        authorize_peer(profile.path(), &host, &peer);
        host.set_internet_route(peer.device_id(), &route())
            .expect("save route for an authorized peer");

        host.forget_peer(peer.device_id())
            .expect("forgetting removes trust and routing metadata");

        assert!(host.trusted_peers().expect("reload trust list").is_empty());
        assert!(
            host.internet_routes()
                .expect("reload public route list")
                .is_empty()
        );
        assert!(matches!(
            host.lookup(peer.device_id()),
            Err(PeerError::Authentication)
        ));
        assert!(matches!(
            host.set_internet_route(peer.device_id(), &route()),
            Err(PeerError::Authentication)
        ));
    }

    fn authorize_peer(profile: &Path, host: &PeerHost, peer: &DeviceIdentity) {
        let mut trust = SqliteTrustStore::open_existing(profile, host.public_identity().device_id)
            .expect("open isolated trust database bound to the host identity");
        trust
            .authorize_peer(&peer.public_identity(), 1)
            .expect("authorize generated public peer identity");
    }

    fn route() -> RelayRoute {
        RelayRoute::new("wss://signal.kryptopass.org", ROOM)
            .expect("construct public metadata route without connecting")
    }
}
