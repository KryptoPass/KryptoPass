#![allow(
    clippy::expect_used,
    reason = "The isolated filesystem fixture uses expect for setup and local assertions."
)]

#[cfg(target_os = "linux")]
mod linux {
    use kryptopass_identity::{DeviceIdentity, SqliteTrustStore};
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt},
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    struct PrivateTempProfile(PathBuf);

    impl PrivateTempProfile {
        fn new() -> Self {
            static NEXT_PROFILE: AtomicU64 = AtomicU64::new(0);
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            loop {
                let nonce = NEXT_PROFILE.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "kryptopass-pairing-reset-{}-{nonce}",
                    std::process::id()
                ));
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

    fn write_private_file(path: &Path, bytes: &[u8]) {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .expect("create isolated private file");
        file.write_all(bytes).expect("write isolated private file");
        file.sync_all().expect("sync isolated private file");
    }

    #[test]
    fn trust_profile_reset_removes_only_its_database_and_sidecars() {
        let profile = PrivateTempProfile::new();
        let local = DeviceIdentity::generate().expect("generate in-memory local identity");
        let peer = DeviceIdentity::generate().expect("generate in-memory peer identity");
        let mut trust = SqliteTrustStore::open_new(profile.path(), local.device_id())
            .expect("create trust database in isolated profile");
        trust
            .authorize_peer(&peer.public_identity(), 1)
            .expect("populate trust database with an isolated peer");
        drop(trust);

        let trust_database = profile.path().join("device-trust.sqlite3");
        let wal = profile.path().join("device-trust.sqlite3-wal");
        let shm = profile.path().join("device-trust.sqlite3-shm");
        let journal = profile.path().join("device-trust.sqlite3-journal");
        write_private_file(&wal, b"test-only WAL sidecar");
        write_private_file(&shm, b"test-only SHM sidecar");
        write_private_file(&journal, b"test-only journal sidecar");

        let openpgp_dir = profile.path().join("openpgp");
        fs::create_dir(&openpgp_dir).expect("create isolated OpenPGP marker directory");
        fs::set_permissions(
            &openpgp_dir,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .expect("protect isolated OpenPGP marker directory");
        let openpgp_marker = openpgp_dir.join("vault-key.sentinel");
        let marker_contents = b"test-only OpenPGP vault marker, not key material";
        write_private_file(&openpgp_marker, marker_contents);

        SqliteTrustStore::reset_profile_files(profile.path())
            .expect("reset only trust database and its fixed SQLite sidecars");

        for target in [&trust_database, &wal, &shm, &journal] {
            assert!(!target.exists(), "trust reset removes {}", target.display());
        }
        assert!(
            profile.path().is_dir(),
            "reset retains the profile directory"
        );
        assert_eq!(
            fs::read(&openpgp_marker).expect("read preserved OpenPGP marker"),
            marker_contents
        );
    }
}
