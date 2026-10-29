//! The trust store in a private SQLite file inside the pairing profile.

use kryptopass_identity::{
    DeviceId, InvitationTokenHash, PublicIdentity, TrustStore, TrustStoreError,
};
use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::path::{Path, PathBuf};

const DATABASE_FILENAME: &str = "device-trust.sqlite3";
const APPLICATION_ID: i64 = 0x4b50_4944;
const SCHEMA_VERSION: i64 = 1;
const MAX_TRUSTED_PEERS: i64 = 1024;
const MAX_INVITATIONS: i64 = 4096;
const MAX_DATABASE_SIZE: u64 = 16 * 1024 * 1024;

/// Durable trust and single-use invitation state for one local device identity.
pub struct SqliteTrustStore {
    connection: Connection,
    profile_dir: PathBuf,
    local_device_id: DeviceId,
}

impl SqliteTrustStore {
    /// Creates a fresh private trust database bound to `local_device_id`.
    ///
    /// The profile directory must already exist, be owned by the current user,
    /// and have mode `0700`. This method never opens or resets an existing DB.
    pub fn open_new(
        profile_dir: impl AsRef<Path>,
        local_device_id: DeviceId,
    ) -> Result<Self, TrustStoreError> {
        let profile_dir = profile_dir.as_ref().to_path_buf();
        validate_profile_dir(&profile_dir)?;
        let path = profile_dir.join(DATABASE_FILENAME);
        create_private_database_file(&path)?;
        let mut connection = open_connection(&path)?;
        configure_connection(&connection)?;
        initialize_schema(&mut connection, local_device_id)?;
        validate_database_file(&path)?;
        Ok(Self {
            connection,
            profile_dir,
            local_device_id,
        })
    }

    /// Opens an existing private database and verifies its local identity binding.
    pub fn open_existing(
        profile_dir: impl AsRef<Path>,
        expected_local_device_id: DeviceId,
    ) -> Result<Self, TrustStoreError> {
        let profile_dir = profile_dir.as_ref().to_path_buf();
        validate_profile_dir(&profile_dir)?;
        let path = profile_dir.join(DATABASE_FILENAME);
        validate_database_file(&path)?;
        validate_database_header_and_sidecars(&path)?;
        let readonly = open_readonly_connection(&path)?;
        configure_readonly_connection(&readonly)?;
        validate_schema(&readonly, expected_local_device_id)?;
        drop(readonly);

        let connection = open_connection(&path)?;
        validate_schema(&connection, expected_local_device_id)?;
        configure_connection(&connection)?;
        Ok(Self {
            connection,
            profile_dir,
            local_device_id: expected_local_device_id,
        })
    }

    /// Validates the exact pairing trust-database paths before a profile reset.
    ///
    /// Only the fixed trust database and its SQLite sidecars are considered;
    /// this never scans or removes other profile files. Missing files are
    /// allowed so a partially completed reset can be retried.
    pub fn validate_reset_targets(profile_dir: impl AsRef<Path>) -> Result<(), TrustStoreError> {
        validate_profile_dir(profile_dir.as_ref())?;
        for path in reset_database_paths(profile_dir.as_ref()) {
            validate_reset_file(&path)?;
        }
        Ok(())
    }

    /// Removes only the fixed pairing trust database and SQLite sidecars.
    ///
    /// The caller must first stop sessions and serialize profile operations.
    /// The profile directory is retained. Files are prevalidated before the
    /// first unlink; if a later unlink fails, the caller receives an error and
    /// may retry after resolving the filesystem failure.
    pub fn reset_profile_files(profile_dir: impl AsRef<Path>) -> Result<(), TrustStoreError> {
        let profile_dir = profile_dir.as_ref();
        Self::validate_reset_targets(profile_dir)?;
        let paths = reset_database_paths(profile_dir);
        for path in paths.iter().skip(1).chain(paths.first()) {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(TrustStoreError::InvalidProfile),
            }
        }
        for path in reset_database_paths(profile_dir) {
            validate_reset_file(&path)?;
            if path.exists() {
                return Err(TrustStoreError::InvalidProfile);
            }
        }
        Ok(())
    }

    /// Returns the profile directory used to open this database.
    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }
}

impl TrustStore for SqliteTrustStore {
    /// Returns the identity to which this database is bound.
    fn local_device_id(&self) -> DeviceId {
        self.local_device_id
    }

    /// Persists explicit authorization for an authenticated peer.
    ///
    /// Call only after the pairing engine has authenticated the remote signing
    /// key and the local user has approved the peer. The transaction commits
    /// all peer fields together; it never replaces a key for an existing ID.
    fn authorize_peer(
        &mut self,
        peer: &PublicIdentity,
        approved_at_ms: i64,
    ) -> Result<(), TrustStoreError> {
        if approved_at_ms < 0 {
            return Err(TrustStoreError::InvalidTimestamp);
        }
        peer.validate().map_err(TrustStoreError::InvalidIdentity)?;
        if peer.device_id == self.local_device_id {
            return Err(TrustStoreError::SelfTrust);
        }

        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .sql()?;
        let existing = tx
            .query_row(
                "SELECT signing_public_key, static_public_key, state
                 FROM trusted_peers WHERE device_id = ?1",
                [peer.device_id.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .sql()?;

        if let Some((signing, static_key, state)) = existing {
            if signing.as_slice() != peer.signing_public_key
                || static_key.as_slice() != peer.static_public_key
            {
                return Err(TrustStoreError::KeyConflict);
            }
            if state != 1 {
                return Err(TrustStoreError::PeerRevoked);
            }
            tx.commit().sql()?;
            return Ok(());
        }

        let peer_count: i64 = tx
            .query_row("SELECT count(*) FROM trusted_peers", [], |row| row.get(0))
            .sql()?;
        if peer_count >= MAX_TRUSTED_PEERS {
            return Err(TrustStoreError::PeerLimitReached);
        }
        tx.execute(
            "INSERT INTO trusted_peers
             (device_id, signing_public_key, static_public_key, state, authorized_at_ms)
             VALUES (?1, ?2, ?3, 1, ?4)",
            params![
                peer.device_id.as_bytes().as_slice(),
                peer.signing_public_key.as_slice(),
                peer.static_public_key.as_slice(),
                approved_at_ms,
            ],
        )
        .sql()?;
        tx.commit().sql()?;
        Ok(())
    }

    /// Returns an authorized peer or fails closed for unknown and revoked IDs.
    fn lookup_trusted_peer(&self, device_id: &DeviceId) -> Result<PublicIdentity, TrustStoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT signing_public_key, static_public_key, state
                 FROM trusted_peers WHERE device_id = ?1",
                [device_id.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .sql()?
            .ok_or(TrustStoreError::UnknownOrRevokedPeer)?;
        if row.2 != 1 {
            return Err(TrustStoreError::UnknownOrRevokedPeer);
        }
        let signing_public_key = array32(&row.0).ok_or(TrustStoreError::CorruptDatabase)?;
        let static_public_key = array32(&row.1).ok_or(TrustStoreError::CorruptDatabase)?;
        let peer = PublicIdentity::new(signing_public_key, static_public_key)
            .map_err(TrustStoreError::InvalidIdentity)?;
        if peer.device_id != *device_id {
            return Err(TrustStoreError::CorruptDatabase);
        }
        Ok(peer)
    }

    /// Lists every currently authorized peer in stable device-ID order.
    ///
    /// The operation reads at most one row beyond the configured maximum and
    /// fails if the database contains more than 1024 authorized peers. It
    /// never returns a silently truncated list. Every stored ID and key is
    /// validated, including the prohibition on trusting the local device.
    fn list_trusted_peers(&self) -> Result<Vec<PublicIdentity>, TrustStoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT device_id, signing_public_key, static_public_key
             FROM trusted_peers WHERE state = 1
             ORDER BY device_id LIMIT ?1",
            )
            .sql()?;
        let mut rows = statement.query([MAX_TRUSTED_PEERS + 1]).sql()?;
        let mut peers = Vec::new();
        while let Some(row) = rows.next().sql()? {
            if peers.len() == MAX_TRUSTED_PEERS as usize {
                return Err(TrustStoreError::TrustedPeerListTooLarge);
            }
            let device_id = array32(&row.get::<_, Vec<u8>>(0).sql()?)
                .ok_or(TrustStoreError::CorruptDatabase)?;
            let signing_public_key = array32(&row.get::<_, Vec<u8>>(1).sql()?)
                .ok_or(TrustStoreError::CorruptDatabase)?;
            let static_public_key = array32(&row.get::<_, Vec<u8>>(2).sql()?)
                .ok_or(TrustStoreError::CorruptDatabase)?;
            let peer = PublicIdentity::new(signing_public_key, static_public_key)
                .map_err(TrustStoreError::InvalidIdentity)?;
            if peer.device_id != DeviceId::from_bytes(device_id) {
                return Err(TrustStoreError::CorruptDatabase);
            }
            if peer.device_id == self.local_device_id {
                return Err(TrustStoreError::SelfTrust);
            }
            peers.push(peer);
        }
        Ok(peers)
    }

    /// Permanently revokes an authorized peer without deleting its key history.
    fn revoke_peer(
        &mut self,
        device_id: &DeviceId,
        revoked_at_ms: i64,
    ) -> Result<(), TrustStoreError> {
        if revoked_at_ms < 0 {
            return Err(TrustStoreError::InvalidTimestamp);
        }
        if *device_id == self.local_device_id {
            return Err(TrustStoreError::SelfTrust);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .sql()?;
        let changed = tx
            .execute(
                "UPDATE trusted_peers SET state = 2, revoked_at_ms = ?2
             WHERE device_id = ?1 AND state = 1",
                params![device_id.as_bytes().as_slice(), revoked_at_ms],
            )
            .sql()?;
        if changed != 1 {
            return Err(TrustStoreError::UnknownOrRevokedPeer);
        }
        tx.commit().sql()?;
        Ok(())
    }

    /// Removes an active authorization so the peer can be explicitly paired again.
    ///
    /// This operation never removes a revoked row. A caller must close any
    /// session and serialize profile operations before invoking it. The row is
    /// deleted transactionally; absent and revoked peers return
    /// [`TrustStoreError::UnknownOrRevokedPeer`].
    fn forget_authorized_peer(&mut self, device_id: &DeviceId) -> Result<(), TrustStoreError> {
        if *device_id == self.local_device_id {
            return Err(TrustStoreError::SelfTrust);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .sql()?;
        let removed = tx
            .execute(
                "DELETE FROM trusted_peers WHERE device_id = ?1 AND state = 1",
                [device_id.as_bytes().as_slice()],
            )
            .sql()?;
        if removed != 1 {
            return Err(TrustStoreError::UnknownOrRevokedPeer);
        }
        tx.commit().sql()?;
        Ok(())
    }

    /// Stores a one-time invitation digest without persisting the raw token.
    ///
    /// Expired rows are pruned in the same transaction so the ledger remains
    /// bounded. `now_ms` and `expires_at_ms` are Unix epoch milliseconds.
    fn register_invitation(
        &mut self,
        invite_id: [u8; 16],
        raw_token: &[u8; 32],
        expires_at_ms: i64,
        now_ms: i64,
    ) -> Result<InvitationTokenHash, TrustStoreError> {
        if invite_id.iter().all(|byte| *byte == 0) || raw_token.iter().all(|byte| *byte == 0) {
            return Err(TrustStoreError::InvalidInvitation);
        }
        if now_ms < 0 || expires_at_ms <= now_ms {
            return Err(TrustStoreError::InvalidTimestamp);
        }
        let token_hash = InvitationTokenHash::from_token(raw_token);
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .sql()?;
        tx.execute(
            "DELETE FROM invitation_ledger WHERE expires_at_ms <= ?1",
            [now_ms],
        )
        .sql()?;
        let existing: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM invitation_ledger
             WHERE invite_id = ?1 OR token_hash = ?2)",
                params![invite_id.as_slice(), token_hash.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .sql()?;
        if existing {
            return Err(TrustStoreError::InvitationAlreadyRegistered);
        }
        let invite_count: i64 = tx
            .query_row("SELECT count(*) FROM invitation_ledger", [], |row| {
                row.get(0)
            })
            .sql()?;
        if invite_count >= MAX_INVITATIONS {
            return Err(TrustStoreError::InvitationLimitReached);
        }
        tx.execute(
            "INSERT INTO invitation_ledger
             (invite_id, token_hash, expires_at_ms) VALUES (?1, ?2, ?3)",
            params![
                invite_id.as_slice(),
                token_hash.as_bytes().as_slice(),
                expires_at_ms
            ],
        )
        .sql()?;
        tx.commit().sql()?;
        Ok(token_hash)
    }

    /// Atomically consumes a registered invitation once and before expiry.
    fn consume_invitation(
        &mut self,
        invite_id: &[u8; 16],
        token_hash: &InvitationTokenHash,
        now_ms: i64,
    ) -> Result<(), TrustStoreError> {
        if now_ms < 0 {
            return Err(TrustStoreError::InvalidTimestamp);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .sql()?;
        let changed = tx
            .execute(
                "UPDATE invitation_ledger SET consumed_at_ms = ?3
             WHERE invite_id = ?1 AND token_hash = ?2
             AND consumed_at_ms IS NULL AND expires_at_ms > ?3",
                params![
                    invite_id.as_slice(),
                    token_hash.as_bytes().as_slice(),
                    now_ms
                ],
            )
            .sql()?;
        if changed != 1 {
            return Err(TrustStoreError::InvitationUnavailable);
        }
        tx.commit().sql()?;
        Ok(())
    }
}

/// SQLite failures as trust errors: corruption is told apart, the rest is a
/// storage failure without the SQL values.
trait Sql<T> {
    fn sql(self) -> Result<T, TrustStoreError>;
}

impl<T> Sql<T> for rusqlite::Result<T> {
    fn sql(self) -> Result<T, TrustStoreError> {
        self.map_err(|error| match error.sqlite_error_code() {
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => {
                TrustStoreError::CorruptDatabase
            }
            _ => TrustStoreError::Storage,
        })
    }
}

fn initialize_schema(
    connection: &mut Connection,
    local_device_id: DeviceId,
) -> Result<(), TrustStoreError> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .sql()?;
    tx.execute_batch(&format!(
        r#"PRAGMA application_id = {APPLICATION_ID};
           PRAGMA user_version = {SCHEMA_VERSION};
           CREATE TABLE metadata (
               singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
               local_device_id BLOB NOT NULL CHECK(length(local_device_id) = 32)
           ) STRICT;
           CREATE TABLE trusted_peers (
               device_id BLOB PRIMARY KEY CHECK(length(device_id) = 32),
               signing_public_key BLOB NOT NULL CHECK(length(signing_public_key) = 32),
               static_public_key BLOB NOT NULL CHECK(length(static_public_key) = 32),
               state INTEGER NOT NULL CHECK(state IN (1, 2)),
               authorized_at_ms INTEGER NOT NULL CHECK(authorized_at_ms >= 0),
               revoked_at_ms INTEGER CHECK(revoked_at_ms IS NULL OR revoked_at_ms >= 0),
               CHECK((state = 1 AND revoked_at_ms IS NULL) OR
                     (state = 2 AND revoked_at_ms IS NOT NULL))
           ) STRICT, WITHOUT ROWID;
           CREATE TABLE invitation_ledger (
               invite_id BLOB PRIMARY KEY CHECK(length(invite_id) = 16),
               token_hash BLOB NOT NULL UNIQUE CHECK(length(token_hash) = 32),
               expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0),
               consumed_at_ms INTEGER CHECK(consumed_at_ms IS NULL OR consumed_at_ms >= 0)
           ) STRICT, WITHOUT ROWID;"#
    ))
    .sql()?;
    tx.execute(
        "INSERT INTO metadata(singleton, local_device_id) VALUES (1, ?1)",
        [local_device_id.as_bytes().as_slice()],
    )
    .sql()?;
    tx.commit().sql()?;
    Ok(())
}

fn validate_schema(
    connection: &Connection,
    expected_local_device_id: DeviceId,
) -> Result<(), TrustStoreError> {
    let application_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .sql()?;
    let schema_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .sql()?;
    if application_id != APPLICATION_ID || schema_version != SCHEMA_VERSION {
        return Err(TrustStoreError::SchemaMismatch);
    }
    let quick_check: Vec<String> = connection
        .prepare("PRAGMA quick_check")
        .sql()?
        .query_map([], |row| row.get(0))
        .sql()?
        .collect::<Result<_, _>>()
        .sql()?;
    if quick_check.as_slice() != ["ok"] {
        return Err(TrustStoreError::CorruptDatabase);
    }
    let user_objects: Vec<String> =
        {
            let mut statement = connection.prepare(
            "SELECT name FROM sqlite_schema WHERE type IN ('table', 'index', 'view', 'trigger')
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        ).sql()?;
            statement
                .query_map([], |row| row.get(0))
                .sql()?
                .collect::<Result<_, _>>()
                .sql()?
        };
    if user_objects.iter().map(String::as_str).ne([
        "invitation_ledger",
        "metadata",
        "trusted_peers",
    ]) {
        return Err(TrustStoreError::SchemaMismatch);
    }
    let stored_local_id: Vec<u8> = connection
        .query_row(
            "SELECT local_device_id FROM metadata WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .sql()?;
    if stored_local_id.as_slice() != expected_local_device_id.as_bytes() {
        return Err(TrustStoreError::IdentityMismatch);
    }
    Ok(())
}

fn configure_connection(connection: &Connection) -> Result<(), TrustStoreError> {
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .sql()?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = DELETE;
         PRAGMA synchronous = FULL;
         PRAGMA secure_delete = ON;
         PRAGMA trusted_schema = OFF;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;",
        )
        .sql()?;
    Ok(())
}

fn configure_readonly_connection(connection: &Connection) -> Result<(), TrustStoreError> {
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .sql()?;
    connection
        .execute_batch("PRAGMA query_only = ON; PRAGMA trusted_schema = OFF;")
        .sql()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn open_connection(path: &Path) -> Result<Connection, TrustStoreError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .sql()
}

#[cfg(not(target_os = "linux"))]
fn open_connection(_path: &Path) -> Result<Connection, TrustStoreError> {
    Err(TrustStoreError::UnsupportedPlatform)
}

#[cfg(target_os = "linux")]
fn open_readonly_connection(path: &Path) -> Result<Connection, TrustStoreError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .sql()
}

#[cfg(not(target_os = "linux"))]
fn open_readonly_connection(_path: &Path) -> Result<Connection, TrustStoreError> {
    Err(TrustStoreError::UnsupportedPlatform)
}

#[cfg(target_os = "linux")]
fn validate_profile_dir(path: &Path) -> Result<(), TrustStoreError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::symlink_metadata(path).map_err(|_| TrustStoreError::InvalidProfile)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(TrustStoreError::InvalidProfile);
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn validate_profile_dir(_path: &Path) -> Result<(), TrustStoreError> {
    Err(TrustStoreError::UnsupportedPlatform)
}

#[cfg(target_os = "linux")]
fn create_private_database_file(path: &Path) -> Result<(), TrustStoreError> {
    match rustix::fs::open(
        path,
        rustix::fs::OFlags::RDWR
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    ) {
        Ok(_) => Ok(()),
        Err(error) if error == rustix::io::Errno::EXIST => Err(TrustStoreError::DatabaseExists),
        Err(error) if error == rustix::io::Errno::NOENT => Err(TrustStoreError::InvalidProfile),
        Err(_) => Err(TrustStoreError::InvalidProfile),
    }
}

#[cfg(not(target_os = "linux"))]
fn create_private_database_file(_path: &Path) -> Result<(), TrustStoreError> {
    Err(TrustStoreError::UnsupportedPlatform)
}

#[cfg(target_os = "linux")]
fn validate_database_file(path: &Path) -> Result<(), TrustStoreError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(TrustStoreError::MissingDatabase);
        }
        Err(_) => return Err(TrustStoreError::InvalidProfile),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.nlink() != 1
        || metadata.mode() & 0o777 != 0o600
        || metadata.len() > MAX_DATABASE_SIZE
    {
        return Err(TrustStoreError::InvalidProfile);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_database_header_and_sidecars(path: &Path) -> Result<(), TrustStoreError> {
    use std::io::Read;

    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )
    .map_err(|_| TrustStoreError::InvalidProfile)?;
    let mut file = std::fs::File::from(fd);
    let mut header = [0_u8; 100];
    file.read_exact(&mut header)
        .map_err(|_| TrustStoreError::CorruptDatabase)?;
    if &header[..16] != b"SQLite format 3\0" {
        return Err(TrustStoreError::CorruptDatabase);
    }
    if header[18] != 1
        || header[19] != 1
        || u32::from_be_bytes([header[60], header[61], header[62], header[63]])
            != SCHEMA_VERSION as u32
        || u32::from_be_bytes([header[68], header[69], header[70], header[71]])
            != APPLICATION_ID as u32
    {
        return Err(TrustStoreError::SchemaMismatch);
    }

    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar_name = path.as_os_str().to_os_string();
        sidecar_name.push(suffix);
        match std::fs::symlink_metadata(PathBuf::from(sidecar_name)) {
            Ok(_) => return Err(TrustStoreError::SchemaMismatch),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(TrustStoreError::InvalidProfile),
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn validate_database_file(_path: &Path) -> Result<(), TrustStoreError> {
    Err(TrustStoreError::UnsupportedPlatform)
}

#[cfg(not(target_os = "linux"))]
fn validate_database_header_and_sidecars(_path: &Path) -> Result<(), TrustStoreError> {
    Err(TrustStoreError::UnsupportedPlatform)
}

fn array32(value: &[u8]) -> Option<[u8; 32]> {
    value.try_into().ok()
}

fn reset_database_paths(profile_dir: &Path) -> Vec<PathBuf> {
    let database = profile_dir.join(DATABASE_FILENAME);
    let mut wal = database.as_os_str().to_os_string();
    wal.push("-wal");
    let mut shm = database.as_os_str().to_os_string();
    shm.push("-shm");
    let mut journal = database.as_os_str().to_os_string();
    journal.push("-journal");
    vec![
        database,
        PathBuf::from(wal),
        PathBuf::from(shm),
        PathBuf::from(journal),
    ]
}

#[cfg(target_os = "linux")]
fn validate_reset_file(path: &Path) -> Result<(), TrustStoreError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(TrustStoreError::InvalidProfile),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.nlink() != 1
        || metadata.mode() & 0o777 != 0o600
        || metadata.len() > MAX_DATABASE_SIZE
    {
        return Err(TrustStoreError::InvalidProfile);
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn validate_reset_file(_path: &Path) -> Result<(), TrustStoreError> {
    Err(TrustStoreError::UnsupportedPlatform)
}
