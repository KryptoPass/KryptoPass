use crate::{
    Digest, Id, Operation, Revision, RevisionHead, SYNC_MAX_BATCH_BYTES, SYNC_MAX_CIPHERTEXT_BYTES,
    SYNC_MAX_HEADS, SYNC_MAX_LEDGER_CIPHERTEXT_BYTES, SYNC_MAX_LEDGER_REVISIONS, SYNC_MAX_PARENTS,
    SYNC_MAX_REVISIONS_PER_BATCH, SyncError, TrustSource, encode_batch, fresh_id,
};
use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::path::Path;
use tracing::{debug, info, warn};

const APPLICATION_ID: i64 = 0x4b50_5359; // "KPSY"
const SCHEMA_VERSION: i64 = 1;
const PAGE_SIZE: i64 = 4096;
const MAX_DB_PAGES: i64 = 24_576;
const MAX_DB_BYTES: u64 = PAGE_SIZE as u64 * MAX_DB_PAGES as u64;
const META_VAULT_ID: &str = "vault_id";

/// Result of applying one atomic revision batch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InsertOutcome {
    /// Number of new immutable revisions committed.
    pub inserted: usize,
    /// Number of exact revisions that were already present or repeated in the batch.
    pub duplicates: usize,
}

/// Public causal metadata used for bounded graph comparison during reconciliation.
pub(crate) struct RevisionMeta {
    pub(crate) record_id: Id,
    pub(crate) revision_id: Id,
    pub(crate) revision_hash: Digest,
    pub(crate) parent_revision_ids: Vec<Id>,
    pub(crate) ciphertext_len: usize,
}

/// A single-vault, transactional store for signed immutable ciphertext revisions.
///
/// The database contains ciphertext and public causal metadata only. It never
/// opens or modifies the application's mutable credential vault.
pub struct Ledger {
    connection: Connection,
    vault_id: Id,
}

impl Ledger {
    /// Creates a new ledger at `path`, refusing to replace any existing path.
    /// On Unix the database is created with owner-only permissions (0600).
    pub fn create(path: impl AsRef<Path>, vault_id: Id) -> Result<Self, SyncError> {
        validate_vault_id(vault_id)?;
        let path = path.as_ref();
        ensure_private_parent(path)?;
        ensure_no_sidecars(path)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path).map_err(|_| SyncError::Storage)?;
        drop(file);

        let initialized = (|| {
            ensure_private_file(path)?;
            let connection = open_connection(path)?;
            initialize_connection(connection, vault_id)
        })();
        match initialized {
            Ok(ledger) => {
                info!(revisions = 0, "Sync ledger created");
                Ok(ledger)
            }
            Err(error) => {
                let _ = fs::remove_file(path);
                let _ = remove_sidecars(path);
                warn!(error = %error, "Sync ledger could not be created");
                Err(error)
            }
        }
    }

    /// Creates an isolated in-memory ledger for offline operation and protocol tests.
    pub fn in_memory(vault_id: Id) -> Result<Self, SyncError> {
        validate_vault_id(vault_id)?;
        let connection = Connection::open_in_memory().map_err(map_sqlite_error)?;
        let ledger = initialize_connection(connection, vault_id)?;
        debug!("Sync in-memory ledger created");
        Ok(ledger)
    }

    /// Opens an existing private ledger and verifies all stored history against trust.
    /// This method never creates a file, identity, or trust record.
    pub fn open_existing(
        path: impl AsRef<Path>,
        vault_id: Id,
        trust: &dyn TrustSource,
    ) -> Result<Self, SyncError> {
        match Self::open_existing_inner(path, vault_id, trust) {
            Ok(ledger) => Ok(ledger),
            Err(error) => {
                warn!(error = %error, "Sync ledger could not be opened");
                Err(error)
            }
        }
    }

    fn open_existing_inner(
        path: impl AsRef<Path>,
        vault_id: Id,
        trust: &dyn TrustSource,
    ) -> Result<Self, SyncError> {
        validate_vault_id(vault_id)?;
        let path = path.as_ref();
        ensure_private_file(path)?;
        ensure_private_sidecars(path)?;
        let connection = open_connection(path)?;
        configure_connection(&connection)?;
        verify_database_bounds(&connection)?;
        verify_schema(&connection)?;
        let stored_vault_id = read_meta_id(&connection, META_VAULT_ID)?;
        if stored_vault_id != vault_id {
            return Err(SyncError::VaultMismatch);
        }
        let ledger = Self {
            connection,
            vault_id,
        };
        let history = ledger.history_index()?;
        validate_history_graph(vault_id, &history)?;
        for meta in &history {
            let revision = ledger
                .load_revision(meta.revision_id)?
                .ok_or(SyncError::InvalidLedger)?;
            validate_revision_shape(&revision, vault_id).map_err(|_| SyncError::InvalidLedger)?;
            authorize_stored_revision(&revision, trust).map_err(|error| match error {
                SyncError::Storage => SyncError::Storage,
                SyncError::UnknownAuthor => SyncError::UnknownAuthor,
                SyncError::InvalidSignature => SyncError::InvalidSignature,
                _ => SyncError::InvalidLedger,
            })?;
        }
        ledger.validate_limits()?;
        debug!(revisions = history.len(), "Sync ledger opened");
        Ok(ledger)
    }

    /// Returns this ledger's bound vault identifier.
    pub const fn vault_id(&self) -> Id {
        self.vault_id
    }

    /// Returns the bounded, deterministic snapshot of all record heads.
    pub fn manifest(&self) -> Result<Vec<RevisionHead>, SyncError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT r.record_id, r.revision_id, r.revision_hash \
                 FROM revisions AS r \
                 WHERE NOT EXISTS (SELECT 1 FROM parent_edges AS p WHERE p.parent_id = r.revision_id) \
                 ORDER BY r.record_id, r.revision_id",
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(map_sqlite_error)?;
        let mut heads = Vec::new();
        for row in rows {
            let (record_id, revision_id, revision_hash) = row.map_err(map_sqlite_error)?;
            heads.push(RevisionHead {
                record_id: fixed::<16>(&record_id).ok_or(SyncError::InvalidLedger)?,
                revision_id: fixed::<16>(&revision_id).ok_or(SyncError::InvalidLedger)?,
                revision_hash: fixed::<32>(&revision_hash).ok_or(SyncError::InvalidLedger)?,
            });
        }
        Ok(heads)
    }

    /// Returns all current heads for one record, including parallel conflict heads.
    pub fn heads(&self, record_id: Id) -> Result<Vec<RevisionHead>, SyncError> {
        if is_zero_id(record_id) {
            return Err(SyncError::InvalidRevision);
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT r.record_id, r.revision_id, r.revision_hash \
                 FROM revisions AS r \
                 WHERE r.record_id = ?1 \
                   AND NOT EXISTS (SELECT 1 FROM parent_edges AS p WHERE p.parent_id = r.revision_id) \
                 ORDER BY r.revision_id",
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map(params![record_id.as_slice()], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(map_sqlite_error)?;
        let mut heads = Vec::new();
        for row in rows {
            let (stored_record, revision_id, revision_hash) = row.map_err(map_sqlite_error)?;
            heads.push(RevisionHead {
                record_id: fixed::<16>(&stored_record).ok_or(SyncError::InvalidLedger)?,
                revision_id: fixed::<16>(&revision_id).ok_or(SyncError::InvalidLedger)?,
                revision_hash: fixed::<32>(&revision_hash).ok_or(SyncError::InvalidLedger)?,
            });
            if heads.len() > SYNC_MAX_HEADS {
                return Err(SyncError::LimitExceeded);
            }
        }
        Ok(heads)
    }

    /// Loads one immutable revision by identifier.
    pub fn get(&self, revision_id: Id) -> Result<Option<Revision>, SyncError> {
        if is_zero_id(revision_id) {
            return Err(SyncError::InvalidRevision);
        }
        self.load_revision(revision_id)
    }

    /// Returns the number of stored immutable revisions.
    pub fn revision_count(&self) -> Result<usize, SyncError> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM revisions", [], |row| row.get(0))
            .map_err(map_sqlite_error)?;
        usize::try_from(count).map_err(|_| SyncError::InvalidLedger)
    }

    /// Validates and atomically inserts a bounded batch of remote revisions.
    /// Exact replays are idempotent; any invalid graph edge rejects the full batch.
    pub fn insert_batch(
        &mut self,
        revisions: &[Revision],
        trust: &dyn TrustSource,
    ) -> Result<InsertOutcome, SyncError> {
        match self.insert_batch_inner(revisions, trust) {
            Ok(outcome) => {
                if !revisions.is_empty() {
                    debug!(
                        revisions = revisions.len(),
                        inserted = outcome.inserted,
                        duplicates = outcome.duplicates,
                        "Sync revision batch committed"
                    );
                }
                Ok(outcome)
            }
            Err(error) => {
                warn!(
                    revisions = revisions.len(),
                    error = %error,
                    "Sync revision batch rejected"
                );
                Err(error)
            }
        }
    }

    fn insert_batch_inner(
        &mut self,
        revisions: &[Revision],
        trust: &dyn TrustSource,
    ) -> Result<InsertOutcome, SyncError> {
        if revisions.is_empty() {
            return Ok(InsertOutcome::default());
        }
        if revisions.len() > SYNC_MAX_REVISIONS_PER_BATCH {
            return Err(SyncError::LimitExceeded);
        }
        let encoded = encode_batch(self.vault_id, revisions)?;
        if encoded.len() > SYNC_MAX_BATCH_BYTES {
            return Err(SyncError::LimitExceeded);
        }

        let mut validated = BTreeMap::<Id, Revision>::new();
        for revision in revisions {
            validate_revision_shape(revision, self.vault_id)?;
            authorize_revision(revision, trust)?;
            if let Some(previous) = validated.get(&revision.revision_id) {
                if !same_revision(previous, revision)? {
                    return Err(SyncError::RevisionIdCollision);
                }
            } else {
                validated.insert(revision.revision_id, revision.clone());
            }
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let mut new_revisions = BTreeMap::<Id, Revision>::new();
        let mut duplicate_ids = BTreeSet::<Id>::new();
        for (revision_id, revision) in &validated {
            if let Some(stored) = load_revision_from(&transaction, self.vault_id, *revision_id)? {
                if !same_revision(&stored, revision)? {
                    return Err(SyncError::RevisionIdCollision);
                }
                duplicate_ids.insert(*revision_id);
            } else {
                new_revisions.insert(*revision_id, revision.clone());
            }
        }

        validate_parent_targets(&transaction, &validated)?;
        let order = topological_new_order(&new_revisions)?;
        check_projected_ledger_limits(&transaction, &new_revisions)?;
        for revision_id in order {
            let revision = new_revisions
                .get(&revision_id)
                .ok_or(SyncError::InvalidLedger)?;
            insert_revision(&transaction, revision, self.vault_id)?;
        }

        validate_final_heads(&transaction)?;
        validate_database_pages(&transaction)?;
        let inserted = new_revisions.len();
        let duplicates = revisions.len().saturating_sub(inserted);
        transaction.commit().map_err(map_sqlite_error)?;
        debug_assert_eq!(duplicates, revisions.len() - inserted);
        debug_assert!(duplicate_ids.len() <= duplicates);
        Ok(InsertOutcome {
            inserted,
            duplicates,
        })
    }

    /// Signs and commits a local revision only when the current heads match exactly.
    pub fn append_local(
        &mut self,
        identity: &DeviceIdentity,
        record_id: Id,
        expected_heads: &[Id],
        operation: Operation,
        ciphertext: Vec<u8>,
        trust: &dyn TrustSource,
    ) -> Result<Revision, SyncError> {
        let ciphertext_len = ciphertext.len();
        match self.append_local_inner(
            identity,
            record_id,
            expected_heads,
            operation,
            ciphertext,
            trust,
        ) {
            Ok(revision) => {
                debug!(
                    operation = ?operation,
                    ciphertext_bytes = ciphertext_len,
                    "Sync local revision committed"
                );
                Ok(revision)
            }
            Err(error) if error == SyncError::HeadConflict => {
                debug!(
                    operation = ?operation,
                    reason = "head_conflict",
                    "Sync local revision rejected"
                );
                Err(error)
            }
            Err(error) => {
                warn!(
                    operation = ?operation,
                    error = %error,
                    "Sync local revision rejected"
                );
                Err(error)
            }
        }
    }

    fn append_local_inner(
        &mut self,
        identity: &DeviceIdentity,
        record_id: Id,
        expected_heads: &[Id],
        operation: Operation,
        ciphertext: Vec<u8>,
        trust: &dyn TrustSource,
    ) -> Result<Revision, SyncError> {
        if is_zero_id(record_id) {
            return Err(SyncError::InvalidRevision);
        }
        let expected = normalize_expected_heads(expected_heads)?;
        if expected.len() > SYNC_MAX_HEADS {
            return Err(SyncError::LimitExceeded);
        }
        validate_operation_ciphertext(operation, &ciphertext)?;
        let public_identity = identity.public_identity();
        public_identity
            .validate()
            .map_err(|_| SyncError::InvalidSignature)?;
        let trusted = trust
            .lookup(identity.device_id())?
            .ok_or(SyncError::UnknownAuthor)?;
        if trusted != public_identity {
            return Err(SyncError::InvalidSignature);
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let current_heads = current_head_ids(&transaction, record_id)?;
        if current_heads != expected {
            return Err(SyncError::HeadConflict);
        }

        let mut revision_id = fresh_id()?;
        for _ in 0..4 {
            let exists = revision_exists(&transaction, revision_id)?;
            if !exists {
                break;
            }
            revision_id = fresh_id()?;
        }
        if revision_exists(&transaction, revision_id)? {
            return Err(SyncError::Randomness);
        }

        let revision = Revision::sign(
            self.vault_id,
            record_id,
            revision_id,
            expected.clone(),
            operation,
            ciphertext,
            identity,
        )?;
        validate_revision_shape(&revision, self.vault_id)?;
        authorize_revision(&revision, trust)?;
        let mut new_revisions = BTreeMap::new();
        new_revisions.insert(revision_id, revision.clone());
        check_projected_ledger_limits(&transaction, &new_revisions)?;
        insert_revision(&transaction, &revision, self.vault_id)?;
        validate_final_heads(&transaction)?;
        validate_database_pages(&transaction)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(revision)
    }

    pub(crate) fn history_index(&self) -> Result<Vec<RevisionMeta>, SyncError> {
        // Parents and revision metadata must describe the same committed snapshot
        // while another local handle appends a revision for the next Sync round.
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(map_sqlite_error)?;
        let history = history_index_from(&transaction)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(history)
    }

    fn load_revision(&self, revision_id: Id) -> Result<Option<Revision>, SyncError> {
        load_revision_from(&self.connection, self.vault_id, revision_id)
    }

    fn validate_limits(&self) -> Result<(), SyncError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(map_sqlite_error)?;
        validate_projected_counts(&transaction, 0, 0)?;
        validate_final_heads(&transaction)?;
        validate_database_pages(&transaction)?;
        transaction.rollback().map_err(map_sqlite_error)?;
        Ok(())
    }
}

fn initialize_connection(connection: Connection, vault_id: Id) -> Result<Ledger, SyncError> {
    configure_connection(&connection)?;
    connection
        .pragma_update(None, "page_size", PAGE_SIZE)
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "application_id", APPLICATION_ID)
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(map_sqlite_error)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(map_sqlite_error)?;
    transaction
        .execute_batch(
            "CREATE TABLE ledger_meta(\
                 key TEXT PRIMARY KEY NOT NULL,\
                 value BLOB NOT NULL\
             ) WITHOUT ROWID;\
             CREATE TABLE revisions(\
                 revision_id BLOB PRIMARY KEY NOT NULL CHECK(length(revision_id) = 16),\
                 record_id BLOB NOT NULL CHECK(length(record_id) = 16),\
                 author_device_id BLOB NOT NULL CHECK(length(author_device_id) = 32),\
                 operation INTEGER NOT NULL CHECK(operation IN (1, 2)),\
                 ciphertext BLOB NOT NULL CHECK((operation = 1 AND length(ciphertext) BETWEEN 1 AND 2097152) OR (operation = 2 AND length(ciphertext) = 0)),\
                 content_hash BLOB NOT NULL CHECK(length(content_hash) = 32),\
                 signature BLOB NOT NULL CHECK(length(signature) = 64),\
                 revision_hash BLOB NOT NULL CHECK(length(revision_hash) = 32)\
             ) WITHOUT ROWID;\
             CREATE INDEX revisions_by_record ON revisions(record_id, revision_id);\
             CREATE TABLE parent_edges(\
                 child_id BLOB NOT NULL CHECK(length(child_id) = 16) REFERENCES revisions(revision_id) ON DELETE RESTRICT,\
                 parent_id BLOB NOT NULL CHECK(length(parent_id) = 16) REFERENCES revisions(revision_id) ON DELETE RESTRICT,\
                 PRIMARY KEY(child_id, parent_id),\
                 CHECK(child_id != parent_id)\
             ) WITHOUT ROWID;\
             CREATE INDEX parent_edges_by_parent ON parent_edges(parent_id);",
        )
        .map_err(map_sqlite_error)?;
    transaction
        .execute(
            "INSERT INTO ledger_meta(key, value) VALUES (?1, ?2)",
            params![META_VAULT_ID, vault_id.as_slice()],
        )
        .map_err(map_sqlite_error)?;
    validate_database_pages(&transaction)?;
    transaction.commit().map_err(map_sqlite_error)?;
    Ok(Ledger {
        connection,
        vault_id,
    })
}

fn configure_connection(connection: &Connection) -> Result<(), SyncError> {
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "trusted_schema", "OFF")
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "temp_store", "MEMORY")
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "journal_mode", "DELETE")
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "max_page_count", MAX_DB_PAGES)
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn open_connection(path: &Path) -> Result<Connection, SyncError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(map_sqlite_error)
}

fn verify_schema(connection: &Connection) -> Result<(), SyncError> {
    let application_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    let schema_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    let page_size: i64 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    if application_id != APPLICATION_ID
        || schema_version != SCHEMA_VERSION
        || page_size != PAGE_SIZE
    {
        return Err(SyncError::InvalidLedger);
    }

    const EXPECTED: [(&str, &str, &str, &str); 5] = [
        (
            "table",
            "ledger_meta",
            "ledger_meta",
            "CREATE TABLE ledger_meta(key TEXT PRIMARY KEY NOT NULL,value BLOB NOT NULL) WITHOUT ROWID",
        ),
        (
            "table",
            "revisions",
            "revisions",
            "CREATE TABLE revisions(revision_id BLOB PRIMARY KEY NOT NULL CHECK(length(revision_id) = 16),record_id BLOB NOT NULL CHECK(length(record_id) = 16),author_device_id BLOB NOT NULL CHECK(length(author_device_id) = 32),operation INTEGER NOT NULL CHECK(operation IN (1, 2)),ciphertext BLOB NOT NULL CHECK((operation = 1 AND length(ciphertext) BETWEEN 1 AND 2097152) OR (operation = 2 AND length(ciphertext) = 0)),content_hash BLOB NOT NULL CHECK(length(content_hash) = 32),signature BLOB NOT NULL CHECK(length(signature) = 64),revision_hash BLOB NOT NULL CHECK(length(revision_hash) = 32)) WITHOUT ROWID",
        ),
        (
            "index",
            "revisions_by_record",
            "revisions",
            "CREATE INDEX revisions_by_record ON revisions(record_id, revision_id)",
        ),
        (
            "table",
            "parent_edges",
            "parent_edges",
            "CREATE TABLE parent_edges(child_id BLOB NOT NULL CHECK(length(child_id) = 16) REFERENCES revisions(revision_id) ON DELETE RESTRICT,parent_id BLOB NOT NULL CHECK(length(parent_id) = 16) REFERENCES revisions(revision_id) ON DELETE RESTRICT,PRIMARY KEY(child_id, parent_id),CHECK(child_id != parent_id)) WITHOUT ROWID",
        ),
        (
            "index",
            "parent_edges_by_parent",
            "parent_edges",
            "CREATE INDEX parent_edges_by_parent ON parent_edges(parent_id)",
        ),
    ];
    let schema_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(map_sqlite_error)?;
    if schema_count != EXPECTED.len() as i64 {
        return Err(SyncError::InvalidLedger);
    }
    let mut statement = connection
        .prepare(
            "SELECT type, name, tbl_name, sql FROM sqlite_schema \
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement.query([]).map_err(map_sqlite_error)?;
    let mut actual = Vec::<(String, String, String, String)>::new();
    actual
        .try_reserve_exact(EXPECTED.len())
        .map_err(|_| SyncError::Allocation)?;
    while let Some(row) = rows.next().map_err(map_sqlite_error)? {
        let object_type = bounded_text(row.get_ref(0).map_err(map_sqlite_error)?, 16)?.to_owned();
        let name = bounded_text(row.get_ref(1).map_err(map_sqlite_error)?, 128)?.to_owned();
        let table_name = bounded_text(row.get_ref(2).map_err(map_sqlite_error)?, 128)?.to_owned();
        let sql = bounded_text(row.get_ref(3).map_err(map_sqlite_error)?, 2048)?;
        actual.push((object_type, name, table_name, normalize_schema_sql(sql)));
    }
    for (kind, name, table_name, sql) in EXPECTED {
        if !actual
            .iter()
            .any(|(actual_kind, actual_name, actual_table, actual_sql)| {
                actual_kind == kind
                    && actual_name == name
                    && actual_table == table_name
                    && actual_sql.as_str() == normalize_schema_sql(sql)
            })
        {
            return Err(SyncError::InvalidLedger);
        }
    }

    let check_is_ok: bool = connection
        .query_row("PRAGMA quick_check(1)", [], |row| {
            let value = row.get_ref(0)?;
            Ok(matches!(value, rusqlite::types::ValueRef::Text(b"ok")))
        })
        .map_err(map_sqlite_error)?;
    if !check_is_ok {
        return Err(SyncError::InvalidLedger);
    }
    let mut foreign_keys = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(map_sqlite_error)?;
    let mut foreign_key_rows = foreign_keys.query([]).map_err(map_sqlite_error)?;
    if foreign_key_rows.next().map_err(map_sqlite_error)?.is_some() {
        return Err(SyncError::InvalidLedger);
    }
    Ok(())
}

fn bounded_text<'a>(
    value: rusqlite::types::ValueRef<'a>,
    max_len: usize,
) -> Result<&'a str, SyncError> {
    match value {
        rusqlite::types::ValueRef::Text(bytes) if bytes.len() <= max_len => {
            std::str::from_utf8(bytes).map_err(|_| SyncError::InvalidLedger)
        }
        _ => Err(SyncError::InvalidLedger),
    }
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != ';')
        .flat_map(char::to_lowercase)
        .collect()
}

fn verify_database_bounds(connection: &Connection) -> Result<(), SyncError> {
    let page_count: i64 = connection
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    let max_page_count: i64 = connection
        .pragma_query_value(None, "max_page_count", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    if !(0..=MAX_DB_PAGES).contains(&page_count) || max_page_count > MAX_DB_PAGES {
        return Err(SyncError::LimitExceeded);
    }
    Ok(())
}

fn validate_database_pages(connection: &Connection) -> Result<(), SyncError> {
    let page_count: i64 = connection
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    if !(0..=MAX_DB_PAGES).contains(&page_count) {
        return Err(SyncError::LimitExceeded);
    }
    Ok(())
}

fn read_meta_id(connection: &Connection, key: &str) -> Result<Id, SyncError> {
    let mut statement = connection
        .prepare("SELECT value FROM ledger_meta WHERE key = ?1")
        .map_err(map_sqlite_error)?;
    let mut rows = statement.query(params![key]).map_err(map_sqlite_error)?;
    let row = rows
        .next()
        .map_err(map_sqlite_error)?
        .ok_or(SyncError::InvalidLedger)?;
    fixed_blob::<16>(row.get_ref(0).map_err(map_sqlite_error)?)
}

fn ensure_private_file(path: &Path) -> Result<(), SyncError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| SyncError::Storage)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_DB_BYTES {
        return Err(SyncError::InvalidLedger);
    }
    ensure_private_parent(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(SyncError::InvalidLedger);
        }
    }
    Ok(())
}

fn ensure_private_parent(path: &Path) -> Result<(), SyncError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let metadata = fs::symlink_metadata(parent).map_err(|_| SyncError::Storage)?;
    if !metadata.file_type().is_dir() {
        return Err(SyncError::InvalidLedger);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(SyncError::InvalidLedger);
        }
    }
    Ok(())
}

fn ensure_private_sidecars(path: &Path) -> Result<(), SyncError> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let sidecar = append_suffix(path, suffix)?;
        match fs::symlink_metadata(&sidecar) {
            Ok(metadata) => {
                if !metadata.file_type().is_file() || metadata.len() > MAX_DB_BYTES {
                    return Err(SyncError::InvalidLedger);
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if metadata.permissions().mode() & 0o077 != 0 {
                        return Err(SyncError::InvalidLedger);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(SyncError::Storage),
        }
    }
    Ok(())
}

fn ensure_no_sidecars(path: &Path) -> Result<(), SyncError> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let sidecar = append_suffix(path, suffix)?;
        match fs::symlink_metadata(sidecar) {
            Ok(_) => return Err(SyncError::InvalidLedger),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(SyncError::Storage),
        }
    }
    Ok(())
}

fn remove_sidecars(path: &Path) -> Result<(), SyncError> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let sidecar = append_suffix(path, suffix)?;
        match fs::remove_file(sidecar) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(SyncError::Storage),
        }
    }
    Ok(())
}

fn append_suffix(path: &Path, suffix: &str) -> Result<std::path::PathBuf, SyncError> {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    Ok(value.into())
}

fn validate_vault_id(vault_id: Id) -> Result<(), SyncError> {
    if is_zero_id(vault_id) {
        Err(SyncError::InvalidRevision)
    } else {
        Ok(())
    }
}

fn is_zero_id(id: Id) -> bool {
    id.iter().all(|byte| *byte == 0)
}

fn fixed<const N: usize>(value: &[u8]) -> Option<[u8; N]> {
    value.try_into().ok()
}

fn fixed_blob<const N: usize>(value: rusqlite::types::ValueRef<'_>) -> Result<[u8; N], SyncError> {
    match value {
        rusqlite::types::ValueRef::Blob(bytes) if bytes.len() == N => {
            bytes.try_into().map_err(|_| SyncError::InvalidLedger)
        }
        _ => Err(SyncError::InvalidLedger),
    }
}

fn copy_bounded_blob(
    value: rusqlite::types::ValueRef<'_>,
    max_len: usize,
) -> Result<Vec<u8>, SyncError> {
    match value {
        rusqlite::types::ValueRef::Blob(bytes) if bytes.len() <= max_len => {
            let mut owned = Vec::new();
            owned
                .try_reserve_exact(bytes.len())
                .map_err(|_| SyncError::Allocation)?;
            owned.extend_from_slice(bytes);
            Ok(owned)
        }
        rusqlite::types::ValueRef::Blob(_) => Err(SyncError::LimitExceeded),
        _ => Err(SyncError::InvalidLedger),
    }
}

fn validate_revision_shape(revision: &Revision, vault_id: Id) -> Result<(), SyncError> {
    if revision.vault_id != vault_id {
        return Err(SyncError::VaultMismatch);
    }
    if is_zero_id(revision.vault_id)
        || is_zero_id(revision.record_id)
        || is_zero_id(revision.revision_id)
        || revision
            .author_device_id
            .as_bytes()
            .iter()
            .all(|byte| *byte == 0)
    {
        return Err(SyncError::InvalidRevision);
    }
    if revision.parent_revision_ids.len() > SYNC_MAX_PARENTS {
        return Err(SyncError::LimitExceeded);
    }
    let mut previous = None;
    for parent in &revision.parent_revision_ids {
        if is_zero_id(*parent) {
            return Err(SyncError::InvalidRevision);
        }
        if *parent == revision.revision_id {
            return Err(SyncError::SelfParent);
        }
        if previous.is_some_and(|previous| previous >= *parent) {
            return Err(SyncError::InvalidRevision);
        }
        previous = Some(*parent);
    }
    validate_operation_ciphertext(revision.operation, &revision.ciphertext)?;
    let expected_content_hash: Digest = Sha256::digest(&revision.ciphertext).into();
    if revision.content_hash != expected_content_hash {
        return Err(SyncError::InvalidRevision);
    }
    let _ = revision.canonical_bytes()?;
    let _ = revision.hash()?;
    Ok(())
}

fn validate_operation_ciphertext(operation: Operation, ciphertext: &[u8]) -> Result<(), SyncError> {
    match operation {
        Operation::Write if ciphertext.is_empty() => Err(SyncError::InvalidRevision),
        Operation::Write if ciphertext.len() > SYNC_MAX_CIPHERTEXT_BYTES => {
            Err(SyncError::LimitExceeded)
        }
        Operation::Delete if !ciphertext.is_empty() => Err(SyncError::InvalidRevision),
        Operation::Write | Operation::Delete => Ok(()),
    }
}

fn authorize_revision(revision: &Revision, trust: &dyn TrustSource) -> Result<(), SyncError> {
    let identity = trust
        .lookup(revision.author_device_id)?
        .ok_or(SyncError::UnknownAuthor)?;
    verify_author(revision, &identity)
}

/// Re-checks history already in the ledger; its author may have been forgotten.
fn authorize_stored_revision(
    revision: &Revision,
    trust: &dyn TrustSource,
) -> Result<(), SyncError> {
    let identity = trust
        .lookup_history(revision.author_device_id)?
        .ok_or(SyncError::UnknownAuthor)?;
    verify_author(revision, &identity)
}

fn verify_author(revision: &Revision, identity: &PublicIdentity) -> Result<(), SyncError> {
    if identity.device_id != revision.author_device_id {
        return Err(SyncError::InvalidSignature);
    }
    identity
        .validate()
        .map_err(|_| SyncError::InvalidSignature)?;
    revision.verify(identity)
}

fn same_revision(left: &Revision, right: &Revision) -> Result<bool, SyncError> {
    Ok(left.canonical_bytes()? == right.canonical_bytes()? && left.signature == right.signature)
}

fn normalize_expected_heads(expected_heads: &[Id]) -> Result<Vec<Id>, SyncError> {
    let mut normalized = expected_heads.to_vec();
    normalized.sort_unstable();
    if normalized.iter().any(|head| is_zero_id(*head))
        || normalized.windows(2).any(|window| window[0] == window[1])
    {
        return Err(SyncError::InvalidRevision);
    }
    Ok(normalized)
}

fn current_head_ids(connection: &Connection, record_id: Id) -> Result<Vec<Id>, SyncError> {
    let mut statement = connection
        .prepare(
            "SELECT r.revision_id FROM revisions AS r \
             WHERE r.record_id = ?1 \
               AND NOT EXISTS (SELECT 1 FROM parent_edges AS p WHERE p.parent_id = r.revision_id) \
             ORDER BY r.revision_id",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map(params![record_id.as_slice()], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .map_err(map_sqlite_error)?;
    let mut heads = Vec::new();
    for row in rows {
        heads.push(fixed::<16>(&row.map_err(map_sqlite_error)?).ok_or(SyncError::InvalidLedger)?);
        if heads.len() > SYNC_MAX_HEADS {
            return Err(SyncError::LimitExceeded);
        }
    }
    Ok(heads)
}

fn validate_parent_targets(
    connection: &Connection,
    incoming: &BTreeMap<Id, Revision>,
) -> Result<(), SyncError> {
    for revision in incoming.values() {
        for parent_id in &revision.parent_revision_ids {
            if let Some(parent) = incoming.get(parent_id) {
                if parent.record_id != revision.record_id {
                    return Err(SyncError::CrossRecordParent);
                }
                continue;
            }
            let parent_record: Option<Vec<u8>> = connection
                .query_row(
                    "SELECT record_id FROM revisions WHERE revision_id = ?1",
                    params![parent_id.as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            let parent_record = parent_record.ok_or(SyncError::MissingParent)?;
            let parent_record = fixed::<16>(&parent_record).ok_or(SyncError::InvalidLedger)?;
            if parent_record != revision.record_id {
                return Err(SyncError::CrossRecordParent);
            }
        }
    }
    Ok(())
}

fn topological_new_order(revisions: &BTreeMap<Id, Revision>) -> Result<Vec<Id>, SyncError> {
    let mut indegree = BTreeMap::<Id, usize>::new();
    let mut children = BTreeMap::<Id, Vec<Id>>::new();
    for (revision_id, revision) in revisions {
        let mut degree = 0_usize;
        for parent in &revision.parent_revision_ids {
            if revisions.contains_key(parent) {
                degree = degree.checked_add(1).ok_or(SyncError::LimitExceeded)?;
                children.entry(*parent).or_default().push(*revision_id);
            }
        }
        indegree.insert(*revision_id, degree);
    }
    let mut ready = BTreeSet::new();
    for (revision_id, degree) in &indegree {
        if *degree == 0 {
            ready.insert(*revision_id);
        }
    }
    let mut order = Vec::with_capacity(revisions.len());
    while let Some(revision_id) = ready.pop_first() {
        order.push(revision_id);
        if let Some(child_ids) = children.get(&revision_id) {
            for child_id in child_ids {
                let degree = indegree.get_mut(child_id).ok_or(SyncError::InvalidLedger)?;
                *degree = degree.checked_sub(1).ok_or(SyncError::InvalidLedger)?;
                if *degree == 0 {
                    ready.insert(*child_id);
                }
            }
        }
    }
    if order.len() != revisions.len() {
        return Err(SyncError::Cycle);
    }
    Ok(order)
}

fn validate_history_graph(vault_id: Id, revisions: &[RevisionMeta]) -> Result<(), SyncError> {
    if is_zero_id(vault_id) || revisions.len() > SYNC_MAX_LEDGER_REVISIONS {
        return Err(SyncError::LimitExceeded);
    }
    let mut by_id = BTreeMap::<Id, &RevisionMeta>::new();
    let mut total_ciphertext = 0_u64;
    for revision in revisions {
        if is_zero_id(revision.record_id) || is_zero_id(revision.revision_id) {
            return Err(SyncError::InvalidLedger);
        }
        if by_id.insert(revision.revision_id, revision).is_some() {
            return Err(SyncError::RevisionIdCollision);
        }
        total_ciphertext = total_ciphertext
            .checked_add(
                u64::try_from(revision.ciphertext_len).map_err(|_| SyncError::LimitExceeded)?,
            )
            .ok_or(SyncError::LimitExceeded)?;
    }
    if total_ciphertext > SYNC_MAX_LEDGER_CIPHERTEXT_BYTES as u64 {
        return Err(SyncError::LimitExceeded);
    }

    let mut referenced = BTreeSet::<Id>::new();
    let mut heads_by_record = HashMap::<Id, usize>::new();
    for revision in revisions {
        if revision.parent_revision_ids.len() > SYNC_MAX_PARENTS
            || revision
                .parent_revision_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err(SyncError::InvalidLedger);
        }
        for parent_id in &revision.parent_revision_ids {
            if *parent_id == revision.revision_id {
                return Err(SyncError::SelfParent);
            }
            let parent = by_id.get(parent_id).ok_or(SyncError::MissingParent)?;
            if parent.record_id != revision.record_id {
                return Err(SyncError::CrossRecordParent);
            }
            referenced.insert(*parent_id);
        }
    }
    topological_meta_order(revisions)?;
    for revision in revisions {
        if !referenced.contains(&revision.revision_id) {
            let count = heads_by_record.entry(revision.record_id).or_default();
            *count = count.saturating_add(1);
            if *count > SYNC_MAX_HEADS {
                return Err(SyncError::LimitExceeded);
            }
        }
    }
    Ok(())
}

pub(crate) fn topological_meta_order(revisions: &[RevisionMeta]) -> Result<Vec<Id>, SyncError> {
    let ids: BTreeSet<Id> = revisions
        .iter()
        .map(|revision| revision.revision_id)
        .collect();
    if ids.len() != revisions.len() {
        return Err(SyncError::RevisionIdCollision);
    }
    let mut indegree = BTreeMap::<Id, usize>::new();
    let mut children = BTreeMap::<Id, Vec<Id>>::new();
    for revision in revisions {
        let mut degree = 0_usize;
        for parent_id in &revision.parent_revision_ids {
            if ids.contains(parent_id) {
                degree = degree.checked_add(1).ok_or(SyncError::LimitExceeded)?;
                children
                    .entry(*parent_id)
                    .or_default()
                    .push(revision.revision_id);
            }
        }
        indegree.insert(revision.revision_id, degree);
    }
    let mut ready = BTreeSet::new();
    for (revision_id, degree) in &indegree {
        if *degree == 0 {
            ready.insert(*revision_id);
        }
    }
    let mut order = Vec::with_capacity(revisions.len());
    while let Some(revision_id) = ready.pop_first() {
        order.push(revision_id);
        if let Some(child_ids) = children.get(&revision_id) {
            for child_id in child_ids {
                let degree = indegree.get_mut(child_id).ok_or(SyncError::InvalidLedger)?;
                *degree = degree.checked_sub(1).ok_or(SyncError::InvalidLedger)?;
                if *degree == 0 {
                    ready.insert(*child_id);
                }
            }
        }
    }
    if order.len() != revisions.len() {
        return Err(SyncError::Cycle);
    }
    Ok(order)
}

fn check_projected_ledger_limits(
    connection: &Connection,
    new_revisions: &BTreeMap<Id, Revision>,
) -> Result<(), SyncError> {
    let added_ciphertext = new_revisions.values().try_fold(0_u64, |sum, revision| {
        sum.checked_add(revision.ciphertext.len() as u64)
            .ok_or(SyncError::LimitExceeded)
    })?;
    validate_projected_counts(connection, new_revisions.len(), added_ciphertext)
}

fn validate_projected_counts(
    connection: &Connection,
    added_revisions: usize,
    added_ciphertext: u64,
) -> Result<(), SyncError> {
    let stored_revisions: i64 = connection
        .query_row("SELECT COUNT(*) FROM revisions", [], |row| row.get(0))
        .map_err(map_sqlite_error)?;
    let stored_ciphertext: i64 = connection
        .query_row(
            "SELECT COALESCE(SUM(length(ciphertext)), 0) FROM revisions",
            [],
            |row| row.get(0),
        )
        .map_err(map_sqlite_error)?;
    let projected_revisions = usize::try_from(stored_revisions)
        .ok()
        .and_then(|stored| stored.checked_add(added_revisions))
        .ok_or(SyncError::InvalidLedger)?;
    let projected_ciphertext = u64::try_from(stored_ciphertext)
        .ok()
        .and_then(|stored| stored.checked_add(added_ciphertext))
        .ok_or(SyncError::InvalidLedger)?;
    if projected_revisions > SYNC_MAX_LEDGER_REVISIONS
        || projected_ciphertext > SYNC_MAX_LEDGER_CIPHERTEXT_BYTES as u64
    {
        return Err(SyncError::LimitExceeded);
    }
    Ok(())
}

fn insert_revision(
    connection: &Connection,
    revision: &Revision,
    vault_id: Id,
) -> Result<(), SyncError> {
    validate_revision_shape(revision, vault_id)?;
    let operation = match revision.operation {
        Operation::Write => 1_i64,
        Operation::Delete => 2_i64,
    };
    let revision_hash = revision.hash()?;
    connection
        .execute(
            "INSERT INTO revisions(\
                 revision_id, record_id, author_device_id, operation, ciphertext,\
                 content_hash, signature, revision_hash\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                revision.revision_id.as_slice(),
                revision.record_id.as_slice(),
                revision.author_device_id.as_bytes().as_slice(),
                operation,
                revision.ciphertext,
                revision.content_hash.as_slice(),
                revision.signature.as_slice(),
                revision_hash.as_slice()
            ],
        )
        .map_err(map_sqlite_error)?;
    for parent_id in &revision.parent_revision_ids {
        connection
            .execute(
                "INSERT INTO parent_edges(child_id, parent_id) VALUES (?1, ?2)",
                params![revision.revision_id.as_slice(), parent_id.as_slice()],
            )
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

fn validate_final_heads(connection: &Connection) -> Result<(), SyncError> {
    let mut statement = connection
        .prepare(
            "SELECT r.record_id, COUNT(*) FROM revisions AS r \
             WHERE NOT EXISTS (SELECT 1 FROM parent_edges AS p WHERE p.parent_id = r.revision_id) \
             GROUP BY r.record_id",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (record_id, count) = row.map_err(map_sqlite_error)?;
        if fixed::<16>(&record_id).is_none() {
            return Err(SyncError::InvalidLedger);
        }
        let count = usize::try_from(count).map_err(|_| SyncError::InvalidLedger)?;
        if count > SYNC_MAX_HEADS {
            return Err(SyncError::LimitExceeded);
        }
    }
    Ok(())
}

fn revision_exists(connection: &Connection, revision_id: Id) -> Result<bool, SyncError> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM revisions WHERE revision_id = ?1",
            params![revision_id.as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sqlite_error)?;
    Ok(exists.is_some())
}

fn load_revision_from(
    connection: &Connection,
    vault_id: Id,
    revision_id: Id,
) -> Result<Option<Revision>, SyncError> {
    let mut statement = connection
        .prepare(
            "SELECT record_id, author_device_id, operation, ciphertext, content_hash, signature, revision_hash \
             FROM revisions WHERE revision_id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query(params![revision_id.as_slice()])
        .map_err(map_sqlite_error)?;
    let Some(row) = rows.next().map_err(map_sqlite_error)? else {
        return Ok(None);
    };
    let record_id = fixed_blob::<16>(row.get_ref(0).map_err(map_sqlite_error)?)?;
    let author_id = fixed_blob::<32>(row.get_ref(1).map_err(map_sqlite_error)?)?;
    let operation = row.get::<_, i64>(2).map_err(map_sqlite_error)?;
    let ciphertext = copy_bounded_blob(
        row.get_ref(3).map_err(map_sqlite_error)?,
        SYNC_MAX_CIPHERTEXT_BYTES,
    )?;
    let content_hash = fixed_blob::<32>(row.get_ref(4).map_err(map_sqlite_error)?)?;
    let signature = fixed_blob::<64>(row.get_ref(5).map_err(map_sqlite_error)?)?;
    let stored_hash = fixed_blob::<32>(row.get_ref(6).map_err(map_sqlite_error)?)?;
    drop(rows);
    drop(statement);
    let parents = load_parent_ids(connection, revision_id)?;
    let revision = Revision {
        vault_id,
        record_id,
        revision_id,
        parent_revision_ids: parents,
        author_device_id: DeviceId::from_bytes(author_id),
        operation: decode_operation(operation)?,
        ciphertext,
        content_hash,
        signature,
    };
    let actual_hash = revision.hash().map_err(|_| SyncError::InvalidLedger)?;
    if stored_hash != actual_hash {
        return Err(SyncError::InvalidLedger);
    }
    Ok(Some(revision))
}

fn history_index_from(connection: &Connection) -> Result<Vec<RevisionMeta>, SyncError> {
    let mut parent_map = BTreeMap::<Id, Vec<Id>>::new();
    {
        let mut edge_statement = connection
            .prepare("SELECT child_id, parent_id FROM parent_edges ORDER BY child_id, parent_id")
            .map_err(map_sqlite_error)?;
        let mut rows = edge_statement.query([]).map_err(map_sqlite_error)?;
        let mut edge_count = 0_usize;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            edge_count = edge_count.checked_add(1).ok_or(SyncError::LimitExceeded)?;
            if edge_count > SYNC_MAX_LEDGER_REVISIONS.saturating_mul(SYNC_MAX_PARENTS) {
                return Err(SyncError::LimitExceeded);
            }
            let child_id = fixed_blob::<16>(row.get_ref(0).map_err(map_sqlite_error)?)?;
            let parent_id = fixed_blob::<16>(row.get_ref(1).map_err(map_sqlite_error)?)?;
            if !parent_map.contains_key(&child_id) && parent_map.len() >= SYNC_MAX_LEDGER_REVISIONS
            {
                return Err(SyncError::LimitExceeded);
            }
            let parents = parent_map.entry(child_id).or_default();
            parents.push(parent_id);
            if parents.len() > SYNC_MAX_PARENTS {
                return Err(SyncError::LimitExceeded);
            }
        }
    }

    let mut statement = connection
        .prepare(
            "SELECT revision_id, record_id, revision_hash, length(ciphertext) \
             FROM revisions ORDER BY revision_id",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement.query([]).map_err(map_sqlite_error)?;
    let mut history = Vec::new();
    history
        .try_reserve_exact(SYNC_MAX_LEDGER_REVISIONS.min(256))
        .map_err(|_| SyncError::Allocation)?;
    while let Some(row) = rows.next().map_err(map_sqlite_error)? {
        if history.len() >= SYNC_MAX_LEDGER_REVISIONS {
            return Err(SyncError::LimitExceeded);
        }
        let revision_id = fixed_blob::<16>(row.get_ref(0).map_err(map_sqlite_error)?)?;
        let record_id = fixed_blob::<16>(row.get_ref(1).map_err(map_sqlite_error)?)?;
        let revision_hash = fixed_blob::<32>(row.get_ref(2).map_err(map_sqlite_error)?)?;
        let ciphertext_len = usize::try_from(row.get::<_, i64>(3).map_err(map_sqlite_error)?)
            .map_err(|_| SyncError::InvalidLedger)?;
        if ciphertext_len > SYNC_MAX_CIPHERTEXT_BYTES {
            return Err(SyncError::LimitExceeded);
        }
        let revision = RevisionMeta {
            record_id,
            revision_id,
            parent_revision_ids: parent_map.remove(&revision_id).unwrap_or_default(),
            revision_hash,
            ciphertext_len,
        };
        history.push(revision);
    }
    if !parent_map.is_empty() {
        return Err(SyncError::InvalidLedger);
    }
    Ok(history)
}

fn load_parent_ids(connection: &Connection, revision_id: Id) -> Result<Vec<Id>, SyncError> {
    let mut statement = connection
        .prepare("SELECT parent_id FROM parent_edges WHERE child_id = ?1 ORDER BY parent_id")
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query(params![revision_id.as_slice()])
        .map_err(map_sqlite_error)?;
    let mut parents = Vec::new();
    while let Some(row) = rows.next().map_err(map_sqlite_error)? {
        parents.push(fixed_blob::<16>(row.get_ref(0).map_err(map_sqlite_error)?)?);
        if parents.len() > SYNC_MAX_PARENTS {
            return Err(SyncError::LimitExceeded);
        }
    }
    Ok(parents)
}

fn decode_operation(operation: i64) -> Result<Operation, SyncError> {
    match operation {
        1 => Ok(Operation::Write),
        2 => Ok(Operation::Delete),
        _ => Err(SyncError::InvalidLedger),
    }
}

fn map_sqlite_error(error: rusqlite::Error) -> SyncError {
    match &error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == rusqlite::ffi::ErrorCode::DiskFull =>
        {
            SyncError::LimitExceeded
        }
        _ => SyncError::Storage,
    }
}
