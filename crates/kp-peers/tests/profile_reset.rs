#![allow(
    clippy::expect_used,
    reason = "The isolated filesystem fixture uses expect for setup and local assertions."
)]

#[cfg(target_os = "linux")]
mod linux {
    use kryptopass_identity::{DeviceIdentity, SqliteTrustStore, SystemSecretKeyStore};
    use kryptopass_peers::{PeerError, PeerHost};
    use std::{
        env,
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        path::{Path, PathBuf},
        process::{Child, Command, Output, Stdio},
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::{Duration, Instant},
    };

    const PAIRING_PROFILE_DIRECTORY: &str = "emulator-pairing-lab-v1";
    const PROFILE_RESET_MARKER: &str = ".pairing-profile-reset-v1";
    const PROFILE_RESET_MARKER_CONTENT: &[u8] = b"KryptoPass pairing profile reset v1\n";
    const SYSTEM_PROFILE_KEYRING_ID: &str = "emulator-pairing-lab-v1";
    const CHILD_ENV: &str = "KP_PROFILE_RESET_ISOLATED_CHILD";
    const CHILD_ROOT_ENV: &str = "KP_PROFILE_RESET_ISOLATED_ROOT";
    const CHILD_JUST_ENV: &str = "KP_PROFILE_RESET_ISOLATED_JUST";
    const CHILD_KEYRING_ENV: &str = "KP_PROFILE_RESET_ISOLATED_KEYRING";
    const PARENT_BUS_PRESENT_ENV: &str = "KP_PROFILE_RESET_PARENT_BUS_PRESENT";
    const PARENT_BUS_ENV: &str = "KP_PROFILE_RESET_PARENT_BUS";
    const PARENT_HOME_PRESENT_ENV: &str = "KP_PROFILE_RESET_PARENT_HOME_PRESENT";
    const PARENT_HOME_ENV: &str = "KP_PROFILE_RESET_PARENT_HOME";

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

    fn write_private_executable(path: &Path, bytes: &[u8]) {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(path)
            .expect("create isolated command stub");
        file.write_all(bytes).expect("write isolated command stub");
        file.sync_all().expect("sync isolated command stub");
    }

    fn ensure_private_directory(path: &Path) {
        fs::create_dir_all(path).expect("create isolated private directory");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("restrict isolated directory to its owner");
    }

    fn require_executable(name: &str) -> PathBuf {
        let search_path = env::var_os("PATH").unwrap_or_default();
        for directory in env::split_paths(&search_path) {
            let candidate = directory.join(name);
            let Ok(metadata) = fs::metadata(&candidate) else {
                continue;
            };
            if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
                return candidate;
            }
        }
        panic!(
            "dev-reset integration requires `{name}` on PATH; install the missing test dependency"
        );
    }

    fn isolated_xdg_directory(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        ensure_private_directory(&path);
        path
    }

    fn isolated_path_with_pgrep_stub(root: &Path) -> std::ffi::OsString {
        let bin = root.join("test-bin");
        ensure_private_directory(&bin);
        // The recipe must not inspect processes outside this isolated XDG profile.
        write_private_executable(&bin.join("pgrep"), b"#!/bin/sh\nexit 1\n");
        let original = env::var_os("PATH").unwrap_or_default();
        let paths = std::iter::once(bin).chain(env::split_paths(&original));
        env::join_paths(paths).expect("compose isolated recipe PATH")
    }

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("resolve workspace root for just dev-reset")
    }

    fn evaluate_just_variable(just: &Path, variable: &str) -> String {
        let output = Command::new(just)
            .args(["--evaluate", variable])
            .current_dir(repository_root())
            .output()
            .unwrap_or_else(|error| panic!("evaluate just variable `{variable}`: {error}"));
        assert!(
            output.status.success(),
            "just --evaluate {variable} failed: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let value = String::from_utf8(output.stdout)
            .unwrap_or_else(|error| panic!("just variable `{variable}` is not UTF-8: {error}"));
        assert!(
            !value.is_empty() && !value.contains('\n') && !value.contains('\r'),
            "just variable `{variable}` must be a non-empty single line"
        );
        value
    }

    fn wait_for_command(mut child: Child, label: &str) -> Output {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if child
                .try_wait()
                .unwrap_or_else(|error| panic!("wait for {label}: {error}"))
                .is_some()
            {
                return child
                    .wait_with_output()
                    .unwrap_or_else(|error| panic!("collect {label} output: {error}"));
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let output = child
                    .wait_with_output()
                    .unwrap_or_else(|error| panic!("stop timed-out {label}: {error}"));
                panic!(
                    "{label} did not exit within 10 seconds; stdout={}, stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn run_isolated_secret_service(daemon: &Path, control_directory: &Path) {
        ensure_private_directory(control_directory);
        let mut child = Command::new(daemon)
            .args(["--unlock", "--components=secrets", "--control-directory"])
            .arg(control_directory)
            .arg("--daemonize")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("start isolated gnome-keyring-daemon: {error}"));
        child
            .stdin
            .take()
            .expect("keyring unlock stdin is piped")
            .write_all(b"\n")
            .expect("send the synthetic empty keyring passphrase");
        let output = wait_for_command(child, "isolated gnome-keyring-daemon --unlock");
        assert!(
            output.status.success(),
            "isolated GNOME Secret Service failed to start: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    fn run_isolated_driver() {
        assert!(
            env::var_os(CHILD_ENV).is_none(),
            "outer driver must not inherit the isolated-child flag"
        );
        let dbus_run_session = require_executable("dbus-run-session");
        let just = require_executable("just");
        let keyring_daemon = require_executable("gnome-keyring-daemon");
        let root = PrivateTempProfile::new();
        let data = isolated_xdg_directory(root.path(), "data");
        let config = isolated_xdg_directory(root.path(), "config");
        let state = isolated_xdg_directory(root.path(), "state");
        let cache = isolated_xdg_directory(root.path(), "cache");
        let runtime = isolated_xdg_directory(root.path(), "runtime");
        let test_binary = env::current_exe().expect("resolve current profile_reset test binary");
        let parent_bus = env::var_os("DBUS_SESSION_BUS_ADDRESS");
        let parent_home = env::var_os("HOME");

        let mut command = Command::new(dbus_run_session);
        command
            .arg("--")
            .arg(test_binary)
            .args([
                "--exact",
                "linux::dev_reset_preserves_keyring_identity_and_rejects_missing_trust_without_marker",
                "--ignored",
                "--nocapture",
            ])
            .current_dir(repository_root())
            .env(CHILD_ENV, "1")
            .env(CHILD_ROOT_ENV, root.path())
            .env(CHILD_JUST_ENV, just)
            .env(CHILD_KEYRING_ENV, keyring_daemon)
            .env("XDG_DATA_HOME", &data)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_STATE_HOME", &state)
            .env("XDG_CACHE_HOME", &cache)
            .env("XDG_RUNTIME_DIR", &runtime);
        if let Some(bus) = parent_bus {
            command
                .env(PARENT_BUS_PRESENT_ENV, "1")
                .env(PARENT_BUS_ENV, bus);
        } else {
            command.env(PARENT_BUS_PRESENT_ENV, "0");
        }
        if let Some(home) = parent_home {
            command
                .env(PARENT_HOME_PRESENT_ENV, "1")
                .env(PARENT_HOME_ENV, home);
        } else {
            command.env(PARENT_HOME_PRESENT_ENV, "0");
        }
        let output = command.output().unwrap_or_else(|error| {
            panic!("launch profile reset test in a private D-Bus session: {error}")
        });
        assert!(
            output.status.success(),
            "isolated profile reset integration failed: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    fn assert_isolated_child_environment() -> PathBuf {
        assert_eq!(
            env::var(CHILD_ENV).expect("isolated child flag is set"),
            "1"
        );
        let root = PathBuf::from(
            env::var_os(CHILD_ROOT_ENV).expect("isolated driver provides a private root"),
        );
        let bus = env::var_os("DBUS_SESSION_BUS_ADDRESS")
            .expect("dbus-run-session provides its private session bus");
        assert!(!bus.is_empty(), "private D-Bus address is non-empty");
        match env::var(PARENT_BUS_PRESENT_ENV).as_deref() {
            Ok("1") => assert_ne!(
                env::var_os(PARENT_BUS_ENV).as_ref(),
                Some(&bus),
                "test must use a D-Bus address different from the parent session"
            ),
            Ok("0") => assert!(
                env::var_os(PARENT_BUS_ENV).is_none(),
                "parent D-Bus address is absent"
            ),
            _ => panic!("isolated driver did not describe the parent D-Bus environment"),
        }
        match env::var(PARENT_HOME_PRESENT_ENV).as_deref() {
            Ok("1") => assert_eq!(
                env::var_os("HOME"),
                env::var_os(PARENT_HOME_ENV),
                "the isolated test must leave HOME unchanged"
            ),
            Ok("0") => assert!(
                env::var_os("HOME").is_none(),
                "the isolated test must not introduce HOME"
            ),
            _ => panic!("isolated driver did not preserve the parent HOME state"),
        }
        for variable in [
            "XDG_DATA_HOME",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "XDG_CACHE_HOME",
            "XDG_RUNTIME_DIR",
        ] {
            let directory = PathBuf::from(
                env::var_os(variable).unwrap_or_else(|| panic!("isolated {variable} is missing")),
            );
            assert!(
                directory.starts_with(&root),
                "{variable} stays under the private root"
            );
            assert_eq!(
                fs::metadata(&directory)
                    .expect("isolated XDG directory exists")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700,
                "{variable} is private"
            );
        }
        root
    }

    fn find_archived_profiles(root: &Path, found: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(root).expect("read isolated dev-reset backup");
        for entry in entries {
            let path = entry.expect("read isolated backup entry").path();
            let metadata = fs::symlink_metadata(&path).expect("inspect isolated backup entry");
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                continue;
            }
            if path
                .file_name()
                .is_some_and(|name| name == PAIRING_PROFILE_DIRECTORY)
                && path.join("device-trust.sqlite3").is_file()
            {
                found.push(path.clone());
            }
            find_archived_profiles(&path, found);
        }
    }

    struct IsolatedKeyringProfileCleanup {
        directory: PathBuf,
        armed: bool,
    }

    impl IsolatedKeyringProfileCleanup {
        fn new(directory: PathBuf) -> Self {
            Self {
                directory,
                armed: true,
            }
        }

        fn reset(&mut self) -> Result<(), PeerError> {
            let parent = self.directory.parent().ok_or(PeerError::Profile)?;
            ensure_private_directory(parent);
            ensure_private_directory(&self.directory);
            PeerHost::reset_system_profile(&self.directory)?;
            self.armed = false;
            Ok(())
        }
    }

    impl Drop for IsolatedKeyringProfileCleanup {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }
            let parent = self
                .directory
                .parent()
                .expect("isolated profile has a parent");
            ensure_private_directory(parent);
            ensure_private_directory(&self.directory);
            if PeerHost::reset_system_profile(&self.directory).is_err() {
                eprintln!("best-effort cleanup of the isolated test keyring profile failed");
            }
        }
    }

    fn run_isolated_dev_reset_case(root: &Path) {
        assert_isolated_child_environment();
        let just = PathBuf::from(env::var_os(CHILD_JUST_ENV).expect("driver supplies just"));
        let app_id = evaluate_just_variable(&just, "app_id");
        let app_org = evaluate_just_variable(&just, "app_org");
        let backup_prefix = format!("{app_org}-reset-");

        let daemon = PathBuf::from(
            env::var_os(CHILD_KEYRING_ENV).expect("driver supplies the keyring daemon path"),
        );
        let control_directory = PathBuf::from(
            env::var_os("XDG_RUNTIME_DIR").expect("isolated runtime directory exists"),
        )
        .join("gnome-keyring-control");
        run_isolated_secret_service(&daemon, &control_directory);

        let data_home =
            PathBuf::from(env::var_os("XDG_DATA_HOME").expect("isolated data home exists"));
        let app_data = data_home.join(app_id);
        ensure_private_directory(&app_data);
        let profile_directory = app_data.join(PAIRING_PROFILE_DIRECTORY);
        let mut cleanup = IsolatedKeyringProfileCleanup::new(profile_directory.clone());

        let host = PeerHost::open_system_profile(&profile_directory)
            .expect("create the initial identity and trust DB in the isolated keyring/profile");
        let original_identity = host.public_identity();
        let peer = DeviceIdentity::generate().expect("generate an isolated trusted peer");
        let peer_public = peer.public_identity();
        let mut trust =
            SqliteTrustStore::open_existing(&profile_directory, original_identity.device_id)
                .expect("open the real temporary trust database");
        trust
            .authorize_peer(&peer_public, 1)
            .expect("authorize an isolated peer before dev-reset");
        drop(trust);
        drop(peer);
        drop(host);

        let isolated_path = isolated_path_with_pgrep_stub(root);
        let reset = Command::new(just)
            .arg("dev-reset")
            .current_dir(repository_root())
            .env("PATH", isolated_path)
            .output()
            .expect("invoke the real just dev-reset recipe");
        assert!(
            reset.status.success(),
            "just dev-reset failed: stdout={}, stderr={}",
            String::from_utf8_lossy(&reset.stdout),
            String::from_utf8_lossy(&reset.stderr),
        );

        let backups = fs::read_dir(&data_home)
            .expect("read isolated data home after dev-reset")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(&backup_prefix))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            backups.len(),
            1,
            "dev-reset leaves one isolated data backup"
        );
        let mut archived_profiles = Vec::new();
        find_archived_profiles(&backups[0], &mut archived_profiles);
        assert_eq!(
            archived_profiles.len(),
            1,
            "the reset backup retains the original pairing profile"
        );
        let archived_trust =
            SqliteTrustStore::open_existing(&archived_profiles[0], original_identity.device_id)
                .expect("the archived real SQLite trust DB remains readable");
        let archived_peers = archived_trust
            .list_trusted_peers()
            .expect("read archived trust records");
        assert_eq!(archived_peers.len(), 1);
        assert_eq!(archived_peers[0].device_id, peer_public.device_id);
        drop(archived_trust);

        assert_eq!(
            fs::metadata(&profile_directory)
                .expect("dev-reset creates a new profile directory")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let reset_marker = profile_directory.join(PROFILE_RESET_MARKER);
        assert_eq!(
            fs::read(&reset_marker).expect("dev-reset writes the reset-intent marker"),
            PROFILE_RESET_MARKER_CONTENT
        );
        assert_eq!(
            fs::metadata(&reset_marker)
                .expect("inspect private reset marker")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(
            PeerHost::system_profile_needs_pairing(&profile_directory)
                .expect("the reset profile is valid and awaits explicit pairing"),
            "a valid reset marker means the UI should wait for the first explicit pairing"
        );
        assert_eq!(
            fs::read(&reset_marker).expect("status inspection leaves reset intent intact"),
            PROFILE_RESET_MARKER_CONTENT
        );
        assert!(
            !profile_directory.join("device-trust.sqlite3").exists(),
            "neither dev-reset nor the read-only status query creates a trust DB"
        );
        let unexpected_sidecar = profile_directory.join("device-trust.sqlite3-wal");
        write_private_file(&unexpected_sidecar, b"unreconciled isolated WAL sidecar");
        assert!(matches!(
            PeerHost::system_profile_needs_pairing(&profile_directory),
            Err(PeerError::Profile)
        ));
        fs::remove_file(&unexpected_sidecar).expect("remove the isolated sidecar probe");
        assert!(
            PeerHost::system_profile_needs_pairing(&profile_directory)
                .expect("marker-only reset profile remains valid after sidecar rejection")
        );

        let corrupt_marker_profile = root.join("corrupt-reset-marker-profile");
        ensure_private_directory(&corrupt_marker_profile);
        write_private_file(
            &corrupt_marker_profile.join(PROFILE_RESET_MARKER),
            b"KryptoPass pairing profile reset v0\n",
        );
        assert!(matches!(
            PeerHost::system_profile_needs_pairing(&corrupt_marker_profile),
            Err(PeerError::Profile)
        ));

        let reset_host = PeerHost::open_system_profile(&profile_directory)
            .expect("explicit pairing consumes the reset marker using the retained identity");
        assert_eq!(
            reset_host.public_identity().device_id,
            original_identity.device_id
        );
        assert!(
            reset_host
                .trusted_peers()
                .expect("read reset trust DB")
                .is_empty()
        );
        assert!(
            !reset_marker.exists(),
            "successful explicit initialization consumes reset intent"
        );
        drop(reset_host);
        assert!(
            !PeerHost::system_profile_needs_pairing(&profile_directory)
                .expect("the initialized profile no longer needs first pairing")
        );

        let mut retained_trust =
            SqliteTrustStore::open_existing(&profile_directory, original_identity.device_id)
                .expect("open the new real trust DB before testing a stale marker");
        retained_trust
            .authorize_peer(&peer_public, 1)
            .expect("authorize one peer in the post-reset profile");
        drop(retained_trust);
        write_private_file(&reset_marker, PROFILE_RESET_MARKER_CONTENT);
        assert!(
            !PeerHost::system_profile_needs_pairing(&profile_directory)
                .expect("a marker beside an existing trust DB is not a fresh profile")
        );
        assert_eq!(
            fs::read(&reset_marker).expect("classification leaves the marker untouched"),
            PROFILE_RESET_MARKER_CONTENT
        );
        let retained_profile = PeerHost::open_existing_system_profile(&profile_directory)
            .expect("the existing-profile path still opens the retained trust DB");
        let retained_peers = retained_profile
            .trusted_peers()
            .expect("read peers through the existing-profile path");
        assert_eq!(retained_peers.len(), 1);
        assert_eq!(retained_peers[0].device_id, peer_public.device_id);
        drop(retained_profile);
        fs::remove_file(&reset_marker).expect("remove the isolated stale-marker probe");

        SqliteTrustStore::reset_profile_files(&profile_directory)
            .expect("remove only the new trust DB to exercise missing trust without reset intent");
        assert!(!reset_marker.exists());
        assert!(
            !profile_directory.join("device-trust.sqlite3").exists(),
            "the profile has no trust DB after the isolated test operation"
        );
        assert!(
            !PeerHost::system_profile_needs_pairing(&profile_directory)
                .expect("missing trust without reset intent is not a fresh pairing profile"),
            "trust loss without reset intent must not be classified as a new profile"
        );
        let reopen_error = match PeerHost::open_system_profile(&profile_directory) {
            Ok(host) => {
                drop(host);
                panic!(
                    "missing trust without reset marker must not initialize a replacement profile"
                );
            }
            Err(error) => error,
        };
        assert!(matches!(reopen_error, PeerError::Profile));

        let keyring = SystemSecretKeyStore::new(&profile_directory, SYSTEM_PROFILE_KEYRING_ID)
            .expect("reopen the same isolated keyring account");
        let retained_identity = DeviceIdentity::load_existing(&keyring)
            .expect("the retained system identity remains readable after trust loss");
        assert_eq!(retained_identity.device_id(), original_identity.device_id);
        drop(retained_identity);
        cleanup
            .reset()
            .expect("remove only the isolated test identity and pairing artifacts");
    }

    #[test]
    #[ignore = "Requires dbus-run-session and an isolated GNOME Secret Service"]
    fn dev_reset_preserves_keyring_identity_and_rejects_missing_trust_without_marker() {
        if env::var_os(CHILD_ENV).is_some() {
            let root = PathBuf::from(
                env::var_os(CHILD_ROOT_ENV).expect("isolated driver provides a private root"),
            );
            run_isolated_dev_reset_case(&root);
        } else {
            run_isolated_driver();
        }
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
