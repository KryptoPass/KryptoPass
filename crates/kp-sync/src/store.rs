//! What Sync needs from storage, and the rules every store applies.
//!
//! A store keeps versions, deletion marks and blobs, and remembers which
//! records it exchanged with each peer. It has no Sync logic of its own:
//! [`verify_batch`] checks what needs no stored data (fields, signatures,
//! authors, blob hashes) and [`plan_apply`] decides, against what is stored,
//! what a batch adds. A store runs `plan_apply` inside the transaction that
//! writes its result, so the decision and the write see the same data.

use crate::model::{
    Deletion, Digest, Fingerprint, Head, Id, RecordKind, SyncError, TrustSource, Version, blob_hash,
};
use crate::wire::Batch;
use kryptopass_identity::DeviceId;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// A stored version without its state or blobs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionMeta {
    pub record_kind: RecordKind,
    pub record_id: Id,
    pub version_id: Id,
    pub hash: Digest,
    pub parents: Vec<Id>,
    pub recipients: Vec<Fingerprint>,
    /// Content hash first, then attachments.
    pub blobs: Vec<Digest>,
}

/// A stored deletion mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletionMeta {
    pub record_kind: RecordKind,
    pub record_id: Id,
    pub hash: Digest,
    pub recipients: Vec<Fingerprint>,
}

/// Everything a store holds, without payloads, as of one moment.
#[derive(Clone, Debug, Default)]
pub struct Index {
    pub versions: Vec<VersionMeta>,
    pub deletions: Vec<DeletionMeta>,
}

/// What applying a batch did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyOutcome {
    /// Versions and deletion marks stored for the first time.
    pub inserted: usize,
    /// Objects already stored, or versions of records already deleted.
    pub skipped: usize,
}

/// Storage for Sync. Every method sees one consistent state.
pub trait SyncStore {
    /// Every stored version and deletion mark.
    fn index(&self) -> Result<Index, SyncError>;

    /// How many versions and deletion marks are stored.
    fn object_count(&self) -> Result<usize, SyncError>;

    fn version(&self, id: &Id) -> Result<Option<Version>, SyncError>;

    /// The deletion mark of a record, if it was deleted.
    fn deletion(&self, record: &Id) -> Result<Option<Deletion>, SyncError>;

    fn blob(&self, hash: &Digest) -> Result<Option<Vec<u8>>, SyncError>;

    /// Records exchanged with `peer` before. They keep going to it whatever
    /// their recipients, so it learns when it loses access.
    fn shared_records(&self, peer: &DeviceId) -> Result<BTreeSet<Id>, SyncError>;

    /// Applies a batch verified with [`verify_batch`] in one transaction:
    /// runs [`plan_apply`] inside it, deletes what each new deletion mark
    /// deletes, stores the new versions and the blobs they need, and records
    /// the batch's records as shared with `peer`. Nothing persists on error.
    fn apply(&mut self, batch: Batch, peer: &DeviceId) -> Result<ApplyOutcome, SyncError>;

    /// Records that `records` were sent to `peer`.
    fn mark_shared(&mut self, peer: &DeviceId, records: &[Id]) -> Result<(), SyncError>;
}

/// Reads [`plan_apply`] needs, made inside the store's write transaction.
pub trait StoreView {
    /// The record and hash of a stored version.
    fn version_meta(&self, id: &Id) -> Result<Option<(Id, Digest)>, SyncError>;

    /// The hash of a record's deletion mark, if it was deleted.
    fn deletion_hash(&self, record: &Id) -> Result<Option<Digest>, SyncError>;

    fn has_blob(&self, hash: &Digest) -> Result<bool, SyncError>;
}

/// What a batch adds to a store.
#[derive(Default)]
pub struct ApplyPlan {
    /// New deletion marks; each deletes its record's versions first.
    pub deletions: Vec<Deletion>,
    /// New versions, parents first.
    pub versions: Vec<Version>,
    /// Blobs of the new versions not stored yet, by hash.
    pub blobs: BTreeMap<Digest, Vec<u8>>,
    /// Objects already stored or versions of deleted records.
    pub skipped: usize,
}

/// Checks a received batch without touching storage: fields, that every
/// author is trusted and signed its object, that no id repeats with
/// different bytes, and that every blob matches a hash a version names.
pub fn verify_batch(batch: &Batch, trust: &dyn TrustSource) -> Result<(), SyncError> {
    let mut authors = HashMap::new();
    let mut check_author = |author: DeviceId| -> Result<_, SyncError> {
        if let Some(identity) = authors.get(&author) {
            return Ok(*identity);
        }
        let identity = trust.lookup(author)?.ok_or(SyncError::UnknownAuthor)?;
        authors.insert(author, identity);
        Ok(identity)
    };
    let mut records = HashSet::new();
    for deletion in &batch.deletions {
        deletion.verify(&check_author(deletion.author)?)?;
        if !records.insert(deletion.record_id) {
            return Err(SyncError::Collision);
        }
    }
    let mut versions = HashSet::new();
    let mut named = HashSet::new();
    for version in &batch.versions {
        version.verify(&check_author(version.author)?)?;
        if !versions.insert(version.version_id) || records.contains(&version.record_id) {
            return Err(SyncError::Collision);
        }
        named.extend(version.blobs().copied());
    }
    let mut blobs = HashSet::new();
    for blob in &batch.blobs {
        let hash = blob_hash(blob);
        if !named.contains(&hash) || !blobs.insert(hash) {
            return Err(SyncError::MissingBlob);
        }
    }
    Ok(())
}

/// Decides what a verified batch adds to the store `view` reads.
///
/// A version of a deleted record is skipped, never stored: deleting for good
/// means an old or offline edit cannot bring the record back. Parents must
/// be stored or come earlier in the batch, from the same record; every blob a
/// new version names must be stored or in the batch. An id stored with
/// different signed bytes is a collision and rejects the batch.
pub fn plan_apply(view: &dyn StoreView, batch: Batch) -> Result<ApplyPlan, SyncError> {
    let Batch {
        deletions,
        versions,
        blobs,
    } = batch;
    let mut plan = ApplyPlan::default();
    let mut deleted = HashSet::new();
    for deletion in deletions {
        let hash = deletion.hash()?;
        match view.deletion_hash(&deletion.record_id)? {
            Some(stored) if stored == hash => plan.skipped += 1,
            // Two devices deleting the same record sign different marks; the
            // record is deleted either way.
            Some(_) => plan.skipped += 1,
            None => {
                deleted.insert(deletion.record_id);
                plan.deletions.push(deletion);
            }
        }
    }
    let mut offered: HashMap<Digest, Vec<u8>> = blobs
        .into_iter()
        .map(|blob| (blob_hash(&blob), blob))
        .collect();
    let mut planned: HashMap<Id, Id> = HashMap::new();
    for version in versions {
        if deleted.contains(&version.record_id) || view.deletion_hash(&version.record_id)?.is_some()
        {
            plan.skipped += 1;
            continue;
        }
        let hash = version.hash()?;
        if let Some((record, stored)) = view.version_meta(&version.version_id)? {
            if record != version.record_id || stored != hash {
                return Err(SyncError::Collision);
            }
            plan.skipped += 1;
            continue;
        }
        for parent in &version.parents {
            let record = match planned.get(parent) {
                Some(record) => *record,
                None => {
                    view.version_meta(parent)?
                        .ok_or(SyncError::MissingParent)?
                        .0
                }
            };
            if record != version.record_id {
                return Err(SyncError::CrossRecordParent);
            }
        }
        for hash in version.blobs() {
            if plan.blobs.contains_key(hash) || view.has_blob(hash)? {
                continue;
            }
            let blob = offered.remove(hash).ok_or(SyncError::MissingBlob)?;
            plan.blobs.insert(*hash, blob);
        }
        planned.insert(version.version_id, version.record_id);
        plan.versions.push(version);
    }
    Ok(plan)
}

/// What one peer may receive from an index: records whose head is encrypted
/// to one of `keys`, or that were shared with it before, with all their
/// versions; and deletion marks routed the same way.
pub struct Routed {
    pub heads: Vec<Head>,
    pub versions: BTreeMap<Id, VersionMeta>,
    /// Version ids in topological order, parents first.
    pub order: Vec<Id>,
}

pub fn route(
    index: Index,
    keys: &[Fingerprint],
    shared: &BTreeSet<Id>,
) -> Result<Routed, SyncError> {
    let to_peer = |recipients: &[Fingerprint], record: &Id| {
        shared.contains(record) || recipients.iter().any(|key| keys.contains(key))
    };
    let parents: HashSet<Id> = index
        .versions
        .iter()
        .flat_map(|meta| meta.parents.iter().copied())
        .collect();
    let routed_records: HashSet<Id> = index
        .versions
        .iter()
        .filter(|meta| !parents.contains(&meta.version_id))
        .filter(|meta| to_peer(&meta.recipients, &meta.record_id))
        .map(|meta| meta.record_id)
        .collect();

    let mut heads = Vec::new();
    for deletion in &index.deletions {
        if to_peer(&deletion.recipients, &deletion.record_id) {
            heads.push(Head {
                record_kind: deletion.record_kind,
                record_id: deletion.record_id,
                version_id: None,
                object_hash: deletion.hash,
            });
        }
    }
    let mut versions = BTreeMap::new();
    for meta in index.versions {
        if !routed_records.contains(&meta.record_id) {
            continue;
        }
        if !parents.contains(&meta.version_id) {
            heads.push(Head {
                record_kind: meta.record_kind,
                record_id: meta.record_id,
                version_id: Some(meta.version_id),
                object_hash: meta.hash,
            });
        }
        if versions.insert(meta.version_id, meta).is_some() {
            return Err(SyncError::InvalidStore);
        }
    }
    heads.sort_by_key(|head| (head.record_id, head.version_id));
    let order = topological_order(&versions)?;
    Ok(Routed {
        heads,
        versions,
        order,
    })
}

/// Version ids with every parent before its children. A parent outside
/// `versions` is the store's inconsistency, and so is a cycle.
pub fn topological_order(versions: &BTreeMap<Id, VersionMeta>) -> Result<Vec<Id>, SyncError> {
    let mut pending: HashMap<Id, usize> = HashMap::with_capacity(versions.len());
    let mut children: HashMap<Id, Vec<Id>> = HashMap::new();
    for (id, meta) in versions {
        for parent in &meta.parents {
            let parent_meta = versions.get(parent).ok_or(SyncError::InvalidStore)?;
            if parent_meta.record_id != meta.record_id {
                return Err(SyncError::InvalidStore);
            }
            children.entry(*parent).or_default().push(*id);
        }
        pending.insert(*id, meta.parents.len());
    }
    let mut ready: Vec<Id> = pending
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| *id)
        .collect();
    ready.sort_unstable_by(|a, b| b.cmp(a));
    let mut order = Vec::with_capacity(versions.len());
    while let Some(id) = ready.pop() {
        order.push(id);
        let mut released = Vec::new();
        for child in children.get(&id).into_iter().flatten() {
            let count = pending.get_mut(child).ok_or(SyncError::InvalidStore)?;
            *count -= 1;
            if *count == 0 {
                released.push(*child);
            }
        }
        ready.extend(released);
        ready.sort_unstable_by(|a, b| b.cmp(a));
    }
    if order.len() == versions.len() {
        Ok(order)
    } else {
        Err(SyncError::Cycle)
    }
}
