//! Bounded Protocol Buffers encoding of a batch: deletion marks, versions and
//! the blobs they need.

use crate::model::{
    Clock, Deletion, Digest, Fingerprint, Id, RecordKind, SYNC_MAX_ATTACHMENTS,
    SYNC_MAX_BATCH_BYTES, SYNC_MAX_BLOB_BYTES, SYNC_MAX_OBJECTS_PER_BATCH, SYNC_MAX_PARENTS,
    SYNC_MAX_RECIPIENTS, SyncError, Version,
};
use kryptopass_identity::DeviceId;
use kryptopass_protocol::proto;
use prost::Message;

/// One transfer unit, in the order the receiver applies it.
#[derive(Default)]
pub struct Batch {
    pub deletions: Vec<Deletion>,
    /// Parents before children.
    pub versions: Vec<Version>,
    /// Encrypted blobs the versions refer to and the receiver lacks.
    pub blobs: Vec<Vec<u8>>,
}

impl Batch {
    pub fn objects(&self) -> usize {
        self.deletions.len() + self.versions.len()
    }
}

/// Encodes a batch, refusing one over the size or object limits.
pub fn encode_batch(batch: &Batch) -> Result<Vec<u8>, SyncError> {
    check_counts(batch)?;
    let mut wire = proto::SyncBatch::default();
    for deletion in &batch.deletions {
        deletion.validate()?;
        wire.deletions.push(proto::SyncDeletion {
            record_kind: deletion.record_kind.as_wire(),
            record_id: deletion.record_id.to_vec(),
            author: deletion.author.as_bytes().to_vec(),
            clock_millis: deletion.clock.millis,
            clock_counter: deletion.clock.counter,
            recipients: fingerprints(&deletion.recipients),
            signature: deletion.signature.to_vec(),
        });
    }
    for version in &batch.versions {
        version.validate()?;
        wire.versions.push(proto::SyncVersion {
            record_kind: version.record_kind.as_wire(),
            record_id: version.record_id.to_vec(),
            version_id: version.version_id.to_vec(),
            parents: version.parents.iter().map(|id| id.to_vec()).collect(),
            author: version.author.as_bytes().to_vec(),
            clock_millis: version.clock.millis,
            clock_counter: version.clock.counter,
            recipients: fingerprints(&version.recipients),
            state: version.state.clone(),
            content_hash: version
                .content_hash
                .map_or_else(Vec::new, |hash| hash.to_vec()),
            attachments: version
                .attachments
                .iter()
                .map(|hash| hash.to_vec())
                .collect(),
            signature: version.signature.to_vec(),
        });
    }
    wire.blobs.clone_from(&batch.blobs);
    let length = wire.encoded_len();
    if length > SYNC_MAX_BATCH_BYTES {
        return Err(SyncError::LimitExceeded);
    }
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(length)
        .map_err(|_| SyncError::Allocation)?;
    wire.encode(&mut encoded).map_err(|_| SyncError::Encode)?;
    Ok(encoded)
}

/// Decodes a batch and checks every object's fields; signatures, authors and
/// the graph are checked by whoever applies it.
pub fn decode_batch(bytes: &[u8]) -> Result<Batch, SyncError> {
    // Protobuf allocates at most what the input holds, so bounding the input
    // bounds the decode.
    if bytes.is_empty() || bytes.len() > SYNC_MAX_BATCH_BYTES {
        return Err(SyncError::LimitExceeded);
    }
    let wire = proto::SyncBatch::decode(bytes).map_err(|_| SyncError::Decode)?;
    if wire.deletions.len() + wire.versions.len() > SYNC_MAX_OBJECTS_PER_BATCH {
        return Err(SyncError::LimitExceeded);
    }
    let mut batch = Batch::default();
    for deletion in wire.deletions {
        let deletion = Deletion {
            record_kind: kind(deletion.record_kind)?,
            record_id: fixed(&deletion.record_id)?,
            author: DeviceId::from_bytes(fixed(&deletion.author)?),
            clock: Clock {
                millis: deletion.clock_millis,
                counter: deletion.clock_counter,
            },
            recipients: recipients(deletion.recipients)?,
            signature: fixed(&deletion.signature)?,
        };
        deletion.validate().map_err(|_| SyncError::Decode)?;
        batch.deletions.push(deletion);
    }
    for version in wire.versions {
        if version.parents.len() > SYNC_MAX_PARENTS
            || version.attachments.len() > SYNC_MAX_ATTACHMENTS
        {
            return Err(SyncError::LimitExceeded);
        }
        let version = Version {
            record_kind: kind(version.record_kind)?,
            record_id: fixed(&version.record_id)?,
            version_id: fixed(&version.version_id)?,
            parents: version
                .parents
                .iter()
                .map(|id| fixed::<16>(id))
                .collect::<Result<Vec<Id>, _>>()?,
            author: DeviceId::from_bytes(fixed(&version.author)?),
            clock: Clock {
                millis: version.clock_millis,
                counter: version.clock_counter,
            },
            recipients: recipients(version.recipients)?,
            state: version.state,
            content_hash: if version.content_hash.is_empty() {
                None
            } else {
                Some(fixed(&version.content_hash)?)
            },
            attachments: version
                .attachments
                .iter()
                .map(|hash| fixed::<32>(hash))
                .collect::<Result<Vec<Digest>, _>>()?,
            signature: fixed(&version.signature)?,
        };
        version.validate().map_err(|_| SyncError::Decode)?;
        batch.versions.push(version);
    }
    batch.blobs = wire.blobs;
    check_counts(&batch)?;
    Ok(batch)
}

fn check_counts(batch: &Batch) -> Result<(), SyncError> {
    let blob_limit = batch
        .versions
        .len()
        .saturating_mul(1 + SYNC_MAX_ATTACHMENTS);
    if batch.objects() == 0
        || batch.objects() > SYNC_MAX_OBJECTS_PER_BATCH
        || batch.blobs.len() > blob_limit
        || batch
            .blobs
            .iter()
            .any(|blob| blob.is_empty() || blob.len() > SYNC_MAX_BLOB_BYTES)
    {
        return Err(SyncError::LimitExceeded);
    }
    Ok(())
}

fn fingerprints(recipients: &[Fingerprint]) -> Vec<Vec<u8>> {
    recipients
        .iter()
        .map(|fingerprint| fingerprint.as_bytes().to_vec())
        .collect()
}

fn recipients(raw: Vec<Vec<u8>>) -> Result<Vec<Fingerprint>, SyncError> {
    if raw.len() > SYNC_MAX_RECIPIENTS {
        return Err(SyncError::LimitExceeded);
    }
    raw.into_iter()
        .map(|bytes| Fingerprint::new(bytes).map_err(|_| SyncError::Decode))
        .collect()
}

fn kind(value: u32) -> Result<RecordKind, SyncError> {
    RecordKind::from_wire(value).ok_or(SyncError::Decode)
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], SyncError> {
    bytes.try_into().map_err(|_| SyncError::Decode)
}
