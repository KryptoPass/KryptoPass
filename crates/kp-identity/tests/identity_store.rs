#![allow(
    clippy::expect_used,
    reason = "Integration tests use expect only for fixture setup and assertions with local context."
)]

use kryptopass_identity::{
    DeviceIdentity, IdentityError, InvitationTokenHash, PublicIdentity, SecureKeyStore,
    SecureStoreError, SensitiveBytes, SignatureError, SqliteTrustStore, StoreIfAbsent,
    TrustStoreError, verify_signature,
};
use rusqlite::Connection;
use sha2::Digest;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone, Default)]
struct FakeSecureKeyStore {
    inner: Arc<Mutex<FakeSecureKeyStoreState>>,
}

#[derive(Default)]
struct FakeSecureKeyStoreState {
    record: Option<SensitiveBytes>,
    fail_load: bool,
    fail_store: bool,
    load_calls: usize,
    store_calls: usize,
}

impl FakeSecureKeyStore {
    fn with_record(record: SensitiveBytes) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeSecureKeyStoreState {
                record: Some(record),
                ..FakeSecureKeyStoreState::default()
            })),
        }
    }

    fn record(&self) -> Option<SensitiveBytes> {
        self.inner.lock().expect("fake store lock").record.clone()
    }

    fn set_load_failure(&self) {
        self.inner.lock().expect("fake store lock").fail_load = true;
    }

    fn set_store_failure(&self) {
        self.inner.lock().expect("fake store lock").fail_store = true;
    }

    fn call_counts(&self) -> (usize, usize) {
        let state = self.inner.lock().expect("fake store lock");
        (state.load_calls, state.store_calls)
    }
}

impl SecureKeyStore for FakeSecureKeyStore {
    fn load(&self) -> Result<Option<SensitiveBytes>, SecureStoreError> {
        let mut state = self.inner.lock().expect("fake store lock");
        state.load_calls += 1;
        if state.fail_load {
            return Err(SecureStoreError::Unavailable);
        }
        Ok(state.record.clone())
    }

    fn store_if_absent(&self, record: &[u8]) -> Result<StoreIfAbsent, SecureStoreError> {
        let mut state = self.inner.lock().expect("fake store lock");
        state.store_calls += 1;
        if state.fail_store {
            return Err(SecureStoreError::BackendFailure);
        }
        if state.record.is_some() {
            return Ok(StoreIfAbsent::AlreadyExists);
        }
        state.record = Some(SensitiveBytes::from(record));
        Ok(StoreIfAbsent::Stored)
    }
}

fn from_hex<const N: usize>(value: &str) -> [u8; N] {
    assert_eq!(value.len(), N * 2, "hex fixture has the expected width");
    let mut result = [0_u8; N];
    for (index, byte) in result.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16).expect("hex fixture is valid");
    }
    result
}

#[test]
fn generated_device_identity_keeps_x25519_and_ed25519_roles_independent() {
    let first = DeviceIdentity::generate().expect("generate first test identity");
    let second = DeviceIdentity::generate().expect("generate second test identity");

    assert_ne!(first.static_public_key(), first.signing_public_key());
    assert_ne!(second.static_public_key(), second.signing_public_key());
    assert_ne!(first.static_public_key(), second.static_public_key());
    assert_ne!(first.signing_public_key(), second.signing_public_key());
    assert_ne!(first.device_id(), second.device_id());
}

#[test]
fn device_id_matches_domain_separated_rfc8032_public_key_vector() {
    // RFC 8032 test 1 public key. The expected device ID is an independently
    // calculated literal for SHA-256(domain || NUL || this 32-byte key).
    let signing_public_key =
        from_hex::<32>("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    let public_identity = PublicIdentity::new(signing_public_key, [0x31_u8; 32])
        .expect("construct public identity for fixed DeviceId vector");

    assert_eq!(
        public_identity.device_id.as_bytes(),
        &from_hex::<32>("d2b4b7adfdb0633ea90962c4595baf0f16e74e44ba651142baf16377550b732f")
    );
}

#[cfg(feature = "test-support")]
#[test]
fn deterministic_seed_import_derives_rfc8032_signing_key() {
    let static_seed = [0x31_u8; 32];
    let signing_seed =
        from_hex::<32>("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
    let identity = DeviceIdentity::from_seed_material(&static_seed, &signing_seed);

    assert_eq!(
        identity.signing_public_key(),
        from_hex::<32>("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
    );
}

#[test]
fn strict_signature_verification_rejects_changed_bytes_keys_and_weak_points() {
    let signer = DeviceIdentity::generate().expect("generate signing identity");
    let other = DeviceIdentity::generate().expect("generate unrelated identity");
    let message = b"kryptopass device statement v1";
    let signature = signer.sign(message);

    assert!(verify_signature(&signer.signing_public_key(), message, &signature).is_ok());

    let changed_message = b"kryptopass device statement v2";
    assert_eq!(
        verify_signature(&signer.signing_public_key(), changed_message, &signature),
        Err(SignatureError::InvalidSignature)
    );
    assert!(verify_signature(&other.signing_public_key(), message, &signature).is_err());

    let mut changed_signature = signature;
    changed_signature[0] ^= 0x80;
    assert_eq!(
        verify_signature(&signer.signing_public_key(), message, &changed_signature),
        Err(SignatureError::InvalidSignature)
    );

    // The compressed Edwards identity point is a canonical small-order point.
    let mut weak_key = [0_u8; 32];
    weak_key[0] = 1;
    assert_eq!(
        verify_signature(&weak_key, message, &signature),
        Err(SignatureError::WeakPublicKey)
    );
}

#[test]
fn secure_store_round_trip_and_create_are_non_overwriting() {
    let store = FakeSecureKeyStore::default();
    let created = DeviceIdentity::create_new(&store).expect("create persisted identity");
    let saved_record = store.record().expect("record stored by fake secure store");
    assert!(!saved_record.is_empty());

    let loaded = DeviceIdentity::load_existing(&store).expect("load persisted identity");
    assert_eq!(loaded.public_identity(), created.public_identity());
    assert_eq!(loaded.static_secret(), created.static_secret());

    assert!(matches!(
        DeviceIdentity::create_new(&store),
        Err(IdentityError::AlreadyExists)
    ));
    assert_eq!(store.record().as_deref(), Some(saved_record.as_slice()));
}

#[test]
fn missing_or_unavailable_secure_store_never_creates_a_fallback() {
    let missing = FakeSecureKeyStore::default();
    assert!(matches!(
        DeviceIdentity::load_existing(&missing),
        Err(IdentityError::Missing)
    ));
    assert_eq!(missing.record(), None);
    assert_eq!(missing.call_counts().1, 0);

    let unavailable = FakeSecureKeyStore::default();
    unavailable.set_load_failure();
    assert!(matches!(
        DeviceIdentity::load_existing(&unavailable),
        Err(IdentityError::SecureStore(SecureStoreError::Unavailable))
    ));
    assert_eq!(unavailable.record(), None);
    assert_eq!(unavailable.call_counts(), (1, 0));
}

#[test]
fn failed_secure_store_write_does_not_return_or_persist_a_fallback_identity() {
    let store = FakeSecureKeyStore::default();
    store.set_store_failure();

    assert!(matches!(
        DeviceIdentity::create_new(&store),
        Err(IdentityError::SecureStore(SecureStoreError::BackendFailure))
    ));
    assert_eq!(store.record(), None);
    assert_eq!(store.call_counts(), (0, 1));

    assert!(matches!(
        DeviceIdentity::load_existing(&store),
        Err(IdentityError::Missing)
    ));
    assert_eq!(store.record(), None);
}

#[test]
fn corrupt_or_unknown_versioned_identity_record_is_never_replaced() {
    let initial = FakeSecureKeyStore::default();
    let _identity = DeviceIdentity::create_new(&initial).expect("create baseline identity");
    let valid_record = initial.record().expect("baseline record exists");

    // The version is covered by the record checksum; update it independently so the
    // parser reaches the explicit version check rather than treating this as corruption.
    let mut unknown_version = valid_record.clone();
    unknown_version.as_mut_slice()[4..6].copy_from_slice(&2_u16.to_be_bytes());
    let checksum_start = 70;
    let digest = sha2::Sha256::digest(
        [
            b"org.kryptopass.device-identity-record.v1\0".as_slice(),
            &unknown_version[..checksum_start],
        ]
        .concat(),
    );
    unknown_version.as_mut_slice()[checksum_start..].copy_from_slice(&digest);
    let unsupported = FakeSecureKeyStore::with_record(unknown_version.clone());
    assert!(matches!(
        DeviceIdentity::load_existing(&unsupported),
        Err(IdentityError::UnsupportedRecordVersion(2))
    ));
    assert_eq!(
        unsupported.record().as_deref(),
        Some(unknown_version.as_slice())
    );
    assert_eq!(unsupported.call_counts().1, 0);

    // A seed bit-flip leaves the record shape and header valid but invalidates its
    // checksum, so a fresh identity must not silently replace the damaged one.
    let mut corrupt_seed = valid_record.clone();
    corrupt_seed.as_mut_slice()[6] ^= 1;
    let corrupt = FakeSecureKeyStore::with_record(corrupt_seed.clone());
    assert!(matches!(
        DeviceIdentity::load_existing(&corrupt),
        Err(IdentityError::CorruptRecord)
    ));
    assert_eq!(corrupt.record().as_deref(), Some(corrupt_seed.as_slice()));
    assert_eq!(corrupt.call_counts().1, 0);
    assert!(matches!(
        DeviceIdentity::create_new(&corrupt),
        Err(IdentityError::AlreadyExists)
    ));
    assert_eq!(corrupt.record().as_deref(), Some(corrupt_seed.as_slice()));
}

#[test]
fn secret_buffer_debug_does_not_reveal_seed_bytes() {
    let secret = [0xa5_u8; 32];
    let protected = SensitiveBytes::from(secret.as_slice());
    let debug = format!("{protected:?}");
    let decimal = format!("{secret:?}");
    let hex = secret
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    assert!(!debug.contains(&decimal));
    assert!(!debug.contains(&hex));
    assert!(debug.contains("redacted") || debug.contains("len: 32"));
}

#[cfg(target_os = "linux")]
struct PrivateTempProfile(PathBuf);

#[cfg(target_os = "linux")]
impl PrivateTempProfile {
    fn new() -> Self {
        use std::os::unix::fs::DirBuilderExt;

        static NEXT_PROFILE: AtomicU64 = AtomicU64::new(0);
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        loop {
            let nonce = NEXT_PROFILE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "kryptopass-identity-{}-{nonce}",
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

    fn database_path(&self) -> PathBuf {
        self.0.join("device-trust.sqlite3")
    }
}

#[cfg(target_os = "linux")]
impl Drop for PrivateTempProfile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(target_os = "linux")]
fn random_peer() -> DeviceIdentity {
    DeviceIdentity::generate().expect("generate isolated test device identity")
}

#[cfg(target_os = "linux")]
#[test]
fn sqlite_trust_is_bound_persistent_and_revocation_survives_restart() {
    use std::os::unix::fs::PermissionsExt;

    let profile = PrivateTempProfile::new();
    let local = random_peer();
    let remote = random_peer();
    let local_id = local.device_id();
    let remote_public = remote.public_identity();

    let mut store = SqliteTrustStore::open_new(profile.path(), local_id)
        .expect("create isolated trust database");
    assert_eq!(store.local_device_id(), local_id);
    assert!(matches!(
        store.lookup_trusted_peer(&remote_public.device_id),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
    store
        .authorize_peer(&remote_public, 1_800_000_000_000)
        .expect("explicitly authorize remote identity");
    assert_eq!(
        store
            .lookup_trusted_peer(&remote_public.device_id)
            .expect("authorized identity is returned"),
        remote_public
    );
    drop(store);

    assert_eq!(
        std::fs::metadata(profile.path())
            .expect("read private profile permissions")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(profile.database_path())
            .expect("read private database permissions")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    let mut reopened = SqliteTrustStore::open_existing(profile.path(), local_id)
        .expect("reopen local trust database");
    assert_eq!(
        reopened
            .lookup_trusted_peer(&remote_public.device_id)
            .expect("trusted peer persisted across restart"),
        remote_public
    );
    reopened
        .revoke_peer(&remote_public.device_id, 1_800_000_000_001)
        .expect("revoke peer");
    assert!(matches!(
        reopened.lookup_trusted_peer(&remote_public.device_id),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
    drop(reopened);

    let reopened =
        SqliteTrustStore::open_existing(profile.path(), local_id).expect("reopen after revocation");
    assert!(matches!(
        reopened.lookup_trusted_peer(&remote_public.device_id),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn forgetting_authorized_peer_allows_a_new_approval_without_changing_other_state() {
    let profile = PrivateTempProfile::new();
    let local_key_store = FakeSecureKeyStore::default();
    let local = DeviceIdentity::create_new(&local_key_store)
        .expect("create local identity in isolated secure-store fixture");
    let local_record = local_key_store
        .record()
        .expect("local identity record is persisted");
    let forgotten_peer = random_peer().public_identity();
    let other_peer = random_peer().public_identity();
    let revoked_peer = random_peer().public_identity();

    let mut trust = SqliteTrustStore::open_new(profile.path(), local.device_id())
        .expect("create isolated trust database");
    trust
        .authorize_peer(&forgotten_peer, 10)
        .expect("authorize peer before forgetting");
    trust
        .authorize_peer(&other_peer, 11)
        .expect("authorize unrelated peer");
    trust
        .authorize_peer(&revoked_peer, 12)
        .expect("authorize peer that will be revoked");
    trust
        .revoke_peer(&revoked_peer.device_id, 13)
        .expect("retain a revocation tombstone");

    trust
        .forget_authorized_peer(&forgotten_peer.device_id)
        .expect("forget only the selected active authorization");
    assert!(matches!(
        trust.lookup_trusted_peer(&forgotten_peer.device_id),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
    assert_eq!(
        trust
            .lookup_trusted_peer(&other_peer.device_id)
            .expect("unrelated peer remains trusted"),
        other_peer
    );
    assert!(matches!(
        trust.forget_authorized_peer(&revoked_peer.device_id),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
    assert!(matches!(
        trust.authorize_peer(&revoked_peer, 14),
        Err(TrustStoreError::PeerRevoked)
    ));
    drop(trust);

    let mut trust = SqliteTrustStore::open_existing(profile.path(), local.device_id())
        .expect("reopen trust database after forgetting");
    assert!(matches!(
        trust.lookup_trusted_peer(&forgotten_peer.device_id),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
    assert_eq!(
        trust
            .lookup_trusted_peer(&other_peer.device_id)
            .expect("unrelated peer remains trusted after restart"),
        other_peer
    );

    // A fresh pairing must authenticate and receive local approval before this
    // core authorization call can restore trust for the same device identity.
    trust
        .authorize_peer(&forgotten_peer, 20)
        .expect("allow a new approved pairing with the same identity");
    assert_eq!(
        trust
            .lookup_trusted_peer(&forgotten_peer.device_id)
            .expect("newly approved peer is trusted again"),
        forgotten_peer
    );
    assert_eq!(local_key_store.record(), Some(local_record));
    assert_eq!(
        DeviceIdentity::load_existing(&local_key_store)
            .expect("forgetting a peer leaves the local identity intact")
            .public_identity(),
        local.public_identity()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn sqlite_rejects_local_identity_mismatch_and_peer_key_substitution() {
    let profile = PrivateTempProfile::new();
    let local = random_peer();
    let other_local = random_peer();
    let remote = random_peer();
    let remote_public = remote.public_identity();
    let mut store = SqliteTrustStore::open_new(profile.path(), local.device_id())
        .expect("create trust database for identity binding test");
    store
        .authorize_peer(&remote_public, 10)
        .expect("authorize initial peer identity");

    let mut substituted_static = remote_public;
    substituted_static.static_public_key[0] ^= 1;
    assert!(matches!(
        store.authorize_peer(&substituted_static, 11),
        Err(TrustStoreError::KeyConflict)
    ));

    let mut substituted_signing = remote_public;
    substituted_signing.signing_public_key[0] ^= 1;
    assert!(matches!(
        store.authorize_peer(&substituted_signing, 12),
        Err(TrustStoreError::InvalidIdentity(_))
    ));
    assert_eq!(
        store
            .lookup_trusted_peer(&remote_public.device_id)
            .expect("failed replacements preserve original keys"),
        remote_public
    );
    drop(store);

    assert!(matches!(
        SqliteTrustStore::open_existing(profile.path(), other_local.device_id()),
        Err(TrustStoreError::IdentityMismatch)
    ));
    assert!(matches!(
        SqliteTrustStore::open_new(profile.path(), other_local.device_id()),
        Err(TrustStoreError::DatabaseExists)
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn sqlite_invitation_ledger_hashes_tokens_and_consumes_once_across_restart() {
    let profile = PrivateTempProfile::new();
    let local = random_peer();
    let invite_id = [0x16_u8; 16];
    let raw_token = [0x42_u8; 32];
    let expired_id = [0x17_u8; 16];
    let expired_token = [0x63_u8; 32];
    let wrong_hash = InvitationTokenHash::from_token(&[0x24_u8; 32]);
    let expected_hash =
        from_hex::<32>("168cc9fdde445e1b93e8cb60de06e380bc43206e6c46124c47556f1e0d72e91b");
    let mut store = SqliteTrustStore::open_new(profile.path(), local.device_id())
        .expect("create trust store for invitation test");

    let stored_hash = store
        .register_invitation(invite_id, &raw_token, 5_000, 1_000)
        .expect("register one-time invite");
    assert_eq!(stored_hash, InvitationTokenHash::from_token(&raw_token));
    assert_eq!(stored_hash.as_bytes(), &expected_hash);
    let hash_debug = format!("{stored_hash:?}");
    assert!(!hash_debug.contains(&format!("{raw_token:?}")));
    assert!(!hash_debug.contains("4242424242424242"));

    let before_invalid = std::fs::read(profile.database_path())
        .expect("snapshot ledger before rejecting zero invite values");
    assert!(
        store
            .register_invitation([0; 16], &[0; 32], 6_000, 1_000)
            .is_err()
    );
    assert_eq!(
        std::fs::read(profile.database_path()).expect("read ledger after zero-value rejection"),
        before_invalid,
        "zero invite ID and bearer token must be rejected before database mutation"
    );
    assert!(matches!(
        store.consume_invitation(&invite_id, &wrong_hash, 1_100),
        Err(TrustStoreError::InvitationUnavailable)
    ));
    store
        .consume_invitation(&invite_id, &stored_hash, 1_100)
        .expect("consume matching invite before expiry");
    assert!(matches!(
        store.register_invitation(invite_id, &raw_token, 6_000, 1_200),
        Err(TrustStoreError::InvitationAlreadyRegistered)
    ));
    store
        .register_invitation(expired_id, &expired_token, 3_000, 1_000)
        .expect("register invite that will expire");
    assert!(matches!(
        store.consume_invitation(
            &expired_id,
            &InvitationTokenHash::from_token(&expired_token),
            3_000
        ),
        Err(TrustStoreError::InvitationUnavailable)
    ));
    assert!(matches!(
        store.register_invitation([0x18_u8; 16], &[0x55; 32], 1_000, 1_000),
        Err(TrustStoreError::InvalidTimestamp)
    ));
    drop(store);

    let db_contents = std::fs::read(profile.database_path()).expect("read invitation database");
    assert!(
        !db_contents
            .windows(raw_token.len())
            .any(|window| window == raw_token)
    );

    let mut reopened = SqliteTrustStore::open_existing(profile.path(), local.device_id())
        .expect("reopen invitation ledger");
    assert!(matches!(
        reopened.consume_invitation(&invite_id, &stored_hash, 1_200),
        Err(TrustStoreError::InvitationUnavailable)
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn sqlite_rejects_corruption_unknown_application_and_schema_without_rewriting() {
    enum DatabaseDamage {
        CorruptBytes,
        ApplicationId,
        SchemaVersion,
    }

    for damage in [
        DatabaseDamage::CorruptBytes,
        DatabaseDamage::ApplicationId,
        DatabaseDamage::SchemaVersion,
    ] {
        let profile = PrivateTempProfile::new();
        let local = random_peer();
        drop(
            SqliteTrustStore::open_new(profile.path(), local.device_id())
                .expect("create database to damage for negative test"),
        );
        let path = profile.database_path();
        match damage {
            DatabaseDamage::CorruptBytes => {
                std::fs::write(&path, b"not a SQLite database")
                    .expect("write controlled database corruption");
            }
            DatabaseDamage::ApplicationId => {
                let connection = Connection::open(&path).expect("open database to change app ID");
                connection
                    .execute_batch("PRAGMA application_id = 287454020;")
                    .expect("write unsupported application ID");
            }
            DatabaseDamage::SchemaVersion => {
                let connection = Connection::open(&path).expect("open database to change version");
                connection
                    .execute_batch("PRAGMA user_version = 2;")
                    .expect("write unsupported schema version");
            }
        }
        let damaged = std::fs::read(&path).expect("read damaged database before open");

        assert!(SqliteTrustStore::open_existing(profile.path(), local.device_id()).is_err());
        assert_eq!(
            std::fs::read(&path).expect("read database after rejected open"),
            damaged,
            "failed open must not rewrite corrupted or unsupported database bytes"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn sqlite_rejects_unsupported_wal_database_without_rewriting_or_dropping_wal() {
    let profile = PrivateTempProfile::new();
    let local = random_peer();
    let remote = random_peer();
    let mut store = SqliteTrustStore::open_new(profile.path(), local.device_id())
        .expect("create database for WAL rejection test");
    store
        .authorize_peer(&remote.public_identity(), 7)
        .expect("store peer before WAL test");
    drop(store);

    let path = profile.database_path();
    let connection = Connection::open(&path).expect("open database for WAL setup");
    connection
        .execute_batch("PRAGMA journal_mode = WAL; PRAGMA application_id = 1431198020;")
        .expect("switch to WAL and set unsupported application ID");
    let before = database_sidecar_snapshot(&path);
    assert!(path.with_extension("sqlite3-wal").exists());

    assert!(matches!(
        SqliteTrustStore::open_existing(profile.path(), local.device_id()),
        Err(TrustStoreError::SchemaMismatch)
    ));
    assert!(
        database_sidecar_snapshot(&path) == before,
        "failed read-only open must not alter the database or WAL bytes"
    );
}

#[cfg(target_os = "linux")]
fn database_sidecar_snapshot(database: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let wal = PathBuf::from(format!("{}-wal", database.display()));
    let shm = PathBuf::from(format!("{}-shm", database.display()));
    [database.to_path_buf(), wal, shm]
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| {
            let bytes = std::fs::read(&path).expect("read SQLite file snapshot");
            (path, bytes)
        })
        .collect()
}

#[cfg(target_os = "linux")]
#[test]
fn sqlite_transaction_failures_roll_back_peer_and_invitation_writes() {
    let profile = PrivateTempProfile::new();
    let local = random_peer();
    let remote = random_peer();
    let remote_public = remote.public_identity();
    let invite_id = [0x91_u8; 16];
    let token = [0x39_u8; 32];
    let token_hash = InvitationTokenHash::from_token(&token);
    let mut store = SqliteTrustStore::open_new(profile.path(), local.device_id())
        .expect("create database for transaction failure tests");
    let raw_connection = Connection::open(profile.database_path())
        .expect("open independent SQL connection for deterministic fault injection");

    raw_connection
        .execute_batch(
            "CREATE TRIGGER reject_peer_insert BEFORE INSERT ON trusted_peers
             BEGIN SELECT RAISE(ABORT, 'injected peer write failure'); END;",
        )
        .expect("install transaction fault trigger");
    assert!(matches!(
        store.authorize_peer(&remote_public, 25),
        Err(TrustStoreError::Database(_))
    ));
    assert!(matches!(
        store.lookup_trusted_peer(&remote_public.device_id),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
    raw_connection
        .execute_batch("DROP TRIGGER reject_peer_insert;")
        .expect("remove peer fault trigger");
    store
        .authorize_peer(&remote_public, 26)
        .expect("retry peer write after rollback");

    raw_connection
        .execute_batch(
            "CREATE TRIGGER reject_invitation_insert BEFORE INSERT ON invitation_ledger
             BEGIN SELECT RAISE(ABORT, 'injected invitation write failure'); END;",
        )
        .expect("install invitation insert fault trigger");
    assert!(matches!(
        store.register_invitation(invite_id, &token, 4_000, 1_000),
        Err(TrustStoreError::Database(_))
    ));
    raw_connection
        .execute_batch("DROP TRIGGER reject_invitation_insert;")
        .expect("remove invitation insert fault trigger");
    store
        .register_invitation(invite_id, &token, 4_000, 1_000)
        .expect("failed insertion left invite ID and digest reusable");

    raw_connection
        .execute_batch(
            "CREATE TRIGGER reject_invitation_consume BEFORE UPDATE ON invitation_ledger
             BEGIN SELECT RAISE(ABORT, 'injected invitation consume failure'); END;",
        )
        .expect("install invitation consume fault trigger");
    assert!(matches!(
        store.consume_invitation(&invite_id, &token_hash, 1_100),
        Err(TrustStoreError::Database(_))
    ));
    raw_connection
        .execute_batch("DROP TRIGGER reject_invitation_consume;")
        .expect("remove invitation consume fault trigger");
    store
        .consume_invitation(&invite_id, &token_hash, 1_100)
        .expect("failed consume rolled back and remains usable");
}

#[cfg(target_os = "linux")]
#[test]
fn trust_store_refuses_self_trust_and_invalid_timestamps() {
    let profile = PrivateTempProfile::new();
    let local = random_peer();
    let local_public = local.public_identity();
    let remote = random_peer();
    let mut store = SqliteTrustStore::open_new(profile.path(), local.device_id())
        .expect("create isolated trust database");

    assert!(matches!(
        store.authorize_peer(&local_public, 10),
        Err(TrustStoreError::SelfTrust)
    ));
    assert!(matches!(
        store.authorize_peer(&remote.public_identity(), -1),
        Err(TrustStoreError::InvalidTimestamp)
    ));
    assert!(matches!(
        store.revoke_peer(&local.device_id(), 10),
        Err(TrustStoreError::SelfTrust)
    ));
    assert!(matches!(
        store.revoke_peer(&remote.device_id(), -1),
        Err(TrustStoreError::InvalidTimestamp)
    ));
    assert!(matches!(
        store.lookup_trusted_peer(&remote.device_id()),
        Err(TrustStoreError::UnknownOrRevokedPeer)
    ));
}
