//! The Sync layer of the store as a [`SyncStore`].
//!
//! Signed objects are read back exactly as stored. A received batch is
//! planned with `kp_sync::plan_apply` inside the write transaction, so the
//! rules every store applies run against the data the write then changes.

use std::collections::BTreeSet;

use kryptopass_identity::DeviceId;
use kryptopass_sync::{
    ApplyOutcome, Batch, Clock, Deletion, DeletionMeta, Digest, Fingerprint, Id, Index, RecordKind,
    StoreView, SyncError, SyncStore, Version, VersionMeta, plan_apply,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::SqliteStore;
use crate::convert::{
    SqlResult, StoreResult, fingerprint_bytes, id_bytes, id_from_bytes, item_id, text, uuid_text,
};
use crate::write;

impl SqliteStore {
    fn sync_read<T>(
        &self,
        query: impl FnOnce(&Connection) -> StoreResult<T>,
    ) -> Result<T, SyncError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|_| SyncError::Storage)?;
        let result = query(&transaction).map_err(|_| SyncError::Storage)?;
        transaction.commit().map_err(|_| SyncError::Storage)?;
        Ok(result)
    }
}

impl SyncStore for SqliteStore {
    fn index(&self) -> Result<Index, SyncError> {
        self.sync_read(index)
    }

    fn object_count(&self) -> Result<usize, SyncError> {
        self.sync_read(|connection| {
            let count: i64 = connection
                .query_row(
                    "SELECT (SELECT COUNT(*) FROM versions) + (SELECT COUNT(*) FROM deletions)",
                    [],
                    |row| row.get(0),
                )
                .sql()?;
            usize::try_from(count).map_err(|_| crate::convert::corrupt("object count"))
        })
    }

    fn version(&self, id: &Id) -> Result<Option<Version>, SyncError> {
        let key = text(id_from_bytes(*id).map_err(|_| SyncError::InvalidObject)?);
        self.sync_read(|connection| load_version(connection, &key))
    }

    fn deletion(&self, record: &Id) -> Result<Option<Deletion>, SyncError> {
        let key = text(id_from_bytes(*record).map_err(|_| SyncError::InvalidObject)?);
        self.sync_read(|connection| load_deletion(connection, &key))
    }

    fn blob(&self, hash: &Digest) -> Result<Option<Vec<u8>>, SyncError> {
        self.sync_read(|connection| {
            connection
                .query_row(
                    "SELECT bytes FROM blobs WHERE hash = ?1",
                    [hash.as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .sql()
        })
    }

    fn shared_records(&self, peer: &DeviceId) -> Result<BTreeSet<Id>, SyncError> {
        self.sync_read(|connection| {
            let mut statement = connection
                .prepare("SELECT record_id FROM peer_records WHERE peer = ?1")
                .sql()?;
            let rows = statement
                .query_map([peer.as_bytes().as_slice()], |row| row.get::<_, String>(0))
                .sql()?
                .collect::<rusqlite::Result<Vec<_>>>()
                .sql()?;
            rows.iter()
                .map(|raw| uuid_text(raw).map(id_bytes))
                .collect()
        })
    }

    fn apply(&mut self, batch: Batch, peer: &DeviceId) -> Result<ApplyOutcome, SyncError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| SyncError::Storage)?;
        let mut records: BTreeSet<Id> = batch
            .deletions
            .iter()
            .map(|deletion| deletion.record_id)
            .collect();
        records.extend(batch.versions.iter().map(|version| version.record_id));

        let plan = plan_apply(&View(&transaction), batch)?;
        let inserted = plan.deletions.len() + plan.versions.len();
        let mut blobs = plan.blobs;
        let mut touched = Vec::new();
        let purged = !plan.deletions.is_empty();
        for deletion in &plan.deletions {
            let record = record_text(&deletion.record_id)?;
            match deletion.record_kind {
                RecordKind::Item => write::purge_record(&transaction, &record),
                RecordKind::Folder => write::purge_folder(&transaction, &record),
            }
            .map_err(storage)?;
            write::insert_deletion(&transaction, deletion).map_err(storage)?;
        }
        for version in &plan.versions {
            let needed: Vec<_> = version
                .blobs()
                .filter_map(|hash| blobs.remove(hash).map(|bytes| (*hash, bytes)))
                .collect();
            write::insert_signed(&transaction, version, needed).map_err(storage)?;
            if version.record_kind == RecordKind::Item && !touched.contains(&version.record_id) {
                touched.push(version.record_id);
            }
        }
        // New versions arrive sealed; the item keeps its head until the vault
        // indexes them, but a conflict shows at once.
        for record in touched {
            let item = item_id(&record_text(&record)?).map_err(storage)?;
            write::refresh_item(&transaction, item).map_err(storage)?;
        }
        if purged {
            write::collect_garbage(&transaction).map_err(storage)?;
        }
        for record in &records {
            mark(&transaction, peer, record)?;
        }
        transaction.commit().map_err(|_| SyncError::Storage)?;
        tracing::debug!(inserted, skipped = plan.skipped, "sync batch stored");
        Ok(ApplyOutcome {
            inserted,
            skipped: plan.skipped,
        })
    }

    fn mark_shared(&mut self, peer: &DeviceId, records: &[Id]) -> Result<(), SyncError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| SyncError::Storage)?;
        for record in records {
            mark(&transaction, peer, record)?;
        }
        transaction.commit().map_err(|_| SyncError::Storage)
    }
}

/// Reads `plan_apply` makes inside the write transaction.
struct View<'a>(&'a Connection);

impl StoreView for View<'_> {
    fn version_meta(&self, id: &Id) -> Result<Option<(Id, Digest)>, SyncError> {
        let key = record_text(id)?;
        let row: Option<(String, Vec<u8>)> = self
            .0
            .query_row(
                "SELECT record_id, hash FROM versions WHERE id = ?1",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| SyncError::Storage)?;
        row.map(|(record, hash)| Ok((record_bytes(&record)?, digest(&hash)?)))
            .transpose()
    }

    fn deletion_hash(&self, record: &Id) -> Result<Option<Digest>, SyncError> {
        let key = record_text(record)?;
        let hash: Option<Vec<u8>> = self
            .0
            .query_row(
                "SELECT hash FROM deletions WHERE record_id = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| SyncError::Storage)?;
        hash.map(|hash| digest(&hash)).transpose()
    }

    fn has_blob(&self, hash: &Digest) -> Result<bool, SyncError> {
        self.0
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM blobs WHERE hash = ?1)",
                [hash.as_slice()],
                |row| row.get(0),
            )
            .map_err(|_| SyncError::Storage)
    }
}

fn index(connection: &Connection) -> StoreResult<Index> {
    let mut statement = connection
        .prepare(
            "SELECT id, record_kind, record_id, hash, content_hash FROM versions ORDER BY rowid",
        )
        .sql()?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
            ))
        })
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    let mut versions = Vec::with_capacity(rows.len());
    for (id, kind, record, hash, content) in rows {
        let mut blobs = Vec::new();
        if let Some(content) = content {
            blobs.push(stored_digest(&content)?);
        }
        blobs.extend(attachments(connection, &id)?);
        versions.push(VersionMeta {
            record_kind: kind_of(kind)?,
            record_id: id_bytes(uuid_text(&record)?),
            version_id: id_bytes(uuid_text(&id)?),
            hash: stored_digest(&hash)?,
            parents: parents(connection, &id)?,
            recipients: recipients(connection, "version_recipients", "version_id", &id)?,
            blobs,
        });
    }
    let mut statement = connection
        .prepare("SELECT record_id, record_kind, hash FROM deletions ORDER BY record_id")
        .sql()?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    let mut deletions = Vec::with_capacity(rows.len());
    for (record, kind, hash) in rows {
        deletions.push(DeletionMeta {
            record_kind: kind_of(kind)?,
            record_id: id_bytes(uuid_text(&record)?),
            hash: stored_digest(&hash)?,
            recipients: recipients(connection, "deletion_recipients", "record_id", &record)?,
        });
    }
    Ok(Index {
        versions,
        deletions,
    })
}

fn load_version(connection: &Connection, id: &str) -> StoreResult<Option<Version>> {
    type Row = (
        i64,
        String,
        Vec<u8>,
        i64,
        i64,
        Vec<u8>,
        Option<Vec<u8>>,
        Vec<u8>,
    );
    let row: Option<Row> = connection
        .query_row(
            "SELECT record_kind, record_id, author, clock_millis, clock_counter, state, \
                 content_hash, signature FROM versions WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()
        .sql()?;
    let Some((kind, record, author, millis, counter, state, content, signature)) = row else {
        return Ok(None);
    };
    let clock = crate::convert::clock(millis, counter)?;
    Ok(Some(Version {
        record_kind: kind_of(kind)?,
        record_id: id_bytes(uuid_text(&record)?),
        version_id: id_bytes(uuid_text(id)?),
        parents: parents(connection, id)?,
        author: device(&author)?,
        clock: Clock {
            millis: clock.millis,
            counter: clock.counter,
        },
        recipients: recipients(connection, "version_recipients", "version_id", id)?,
        state,
        content_hash: content.as_deref().map(stored_digest).transpose()?,
        attachments: attachments(connection, id)?,
        signature: signature_of(&signature)?,
    }))
}

fn load_deletion(connection: &Connection, record: &str) -> StoreResult<Option<Deletion>> {
    type Row = (i64, Vec<u8>, i64, i64, Vec<u8>);
    let row: Option<Row> = connection
        .query_row(
            "SELECT record_kind, author, clock_millis, clock_counter, signature \
             FROM deletions WHERE record_id = ?1",
            [record],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .sql()?;
    let Some((kind, author, millis, counter, signature)) = row else {
        return Ok(None);
    };
    let clock = crate::convert::clock(millis, counter)?;
    Ok(Some(Deletion {
        record_kind: kind_of(kind)?,
        record_id: id_bytes(uuid_text(record)?),
        author: device(&author)?,
        clock: Clock {
            millis: clock.millis,
            counter: clock.counter,
        },
        recipients: recipients(connection, "deletion_recipients", "record_id", record)?,
        signature: signature_of(&signature)?,
    }))
}

fn parents(connection: &Connection, version: &str) -> StoreResult<Vec<Id>> {
    let mut statement = connection
        .prepare("SELECT parent_id FROM version_parents WHERE version_id = ?1")
        .sql()?;
    let rows = statement
        .query_map([version], |row| row.get::<_, String>(0))
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    let mut parents = rows
        .iter()
        .map(|raw| uuid_text(raw).map(id_bytes))
        .collect::<StoreResult<Vec<_>>>()?;
    parents.sort_unstable();
    Ok(parents)
}

fn attachments(connection: &Connection, version: &str) -> StoreResult<Vec<Digest>> {
    let mut statement = connection
        .prepare("SELECT hash FROM version_attachments WHERE version_id = ?1 ORDER BY hash")
        .sql()?;
    let rows = statement
        .query_map([version], |row| row.get::<_, Vec<u8>>(0))
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    rows.iter().map(|hash| stored_digest(hash)).collect()
}

/// Recipients from `table`, keyed by `column`, as Sync's sorted fingerprints.
fn recipients(
    connection: &Connection,
    table: &str,
    column: &str,
    key: &str,
) -> StoreResult<Vec<Fingerprint>> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT fingerprint FROM {table} WHERE {column} = ?1"
        ))
        .sql()?;
    let rows = statement
        .query_map([key], |row| row.get::<_, String>(0))
        .sql()?
        .collect::<rusqlite::Result<Vec<_>>>()
        .sql()?;
    let mut fingerprints = rows
        .iter()
        .map(|raw| {
            Fingerprint::new(fingerprint_bytes(raw)?)
                .map_err(|_| crate::convert::corrupt("recipient fingerprint"))
        })
        .collect::<StoreResult<Vec<_>>>()?;
    fingerprints.sort_unstable();
    Ok(fingerprints)
}

fn mark(connection: &Connection, peer: &DeviceId, record: &Id) -> Result<(), SyncError> {
    connection
        .execute(
            "INSERT INTO peer_records(peer, record_id) VALUES (?1, ?2) \
             ON CONFLICT(peer, record_id) DO NOTHING",
            params![peer.as_bytes().as_slice(), record_text(record)?],
        )
        .map(drop)
        .map_err(|_| SyncError::Storage)
}

fn kind_of(value: i64) -> StoreResult<RecordKind> {
    u32::try_from(value)
        .ok()
        .and_then(RecordKind::from_wire)
        .ok_or_else(|| crate::convert::corrupt("record kind"))
}

fn device(bytes: &[u8]) -> StoreResult<DeviceId> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| crate::convert::corrupt("version author"))?;
    Ok(DeviceId::from_bytes(bytes))
}

fn signature_of(bytes: &[u8]) -> StoreResult<[u8; 64]> {
    bytes
        .try_into()
        .map_err(|_| crate::convert::corrupt("signature"))
}

fn stored_digest(bytes: &[u8]) -> StoreResult<Digest> {
    bytes
        .try_into()
        .map_err(|_| crate::convert::corrupt("hash"))
}

fn digest(bytes: &[u8]) -> Result<Digest, SyncError> {
    bytes.try_into().map_err(|_| SyncError::InvalidStore)
}

fn record_text(id: &Id) -> Result<String, SyncError> {
    id_from_bytes(*id)
        .map(text)
        .map_err(|_| SyncError::InvalidObject)
}

fn record_bytes(raw: &str) -> Result<Id, SyncError> {
    uuid_text(raw)
        .map(id_bytes)
        .map_err(|_| SyncError::InvalidStore)
}

fn storage(_: kryptopass_core::vault::StoreError) -> SyncError {
    SyncError::Storage
}
