//! Directed, bounded reconciliation over the immutable revision DAG.

use crate::{
    Id, Ledger, Revision, SYNC_MAX_BATCH_BYTES, SYNC_MAX_REVISIONS_PER_BATCH, SyncError,
    TrustSource, decode_batch, encode_batch, topological_meta_order,
};
use std::collections::BTreeMap;

/// Counts for one directed source-to-destination reconciliation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReconcileOutcome {
    /// Number of bounded batches committed by the destination.
    pub batches_sent: usize,
    /// Number of revisions sent in successfully committed batches.
    pub revisions_sent: usize,
    /// Number of sent revisions already present at the destination when applied.
    pub duplicates: usize,
    /// Number of sent revisions acknowledged after destination commit.
    pub acknowledged: usize,
}

/// Copies source revisions missing from the destination, including their missing
/// ancestors. Each batch passes through the protobuf codec before destination
/// validation and is acknowledged only after its SQLite transaction commits.
///
/// Reconciliation is directed: call it in the opposite direction to exchange
/// revisions authored only at the destination.
pub fn reconcile(
    source: &Ledger,
    destination: &mut Ledger,
    trust: &dyn TrustSource,
) -> Result<ReconcileOutcome, SyncError> {
    if source.vault_id() != destination.vault_id() {
        return Err(SyncError::VaultMismatch);
    }
    let vault_id = source.vault_id();
    let source_history = source.history_index()?;
    let destination_history = destination.history_index()?;

    let mut destination_hashes = BTreeMap::<Id, [u8; 32]>::new();
    for revision in &destination_history {
        destination_hashes.insert(revision.revision_id, revision.revision_hash);
    }
    let mut missing = BTreeMap::<Id, ()>::new();
    for revision in &source_history {
        match destination_hashes.get(&revision.revision_id) {
            Some(destination_hash) if *destination_hash == revision.revision_hash => {}
            Some(_) => return Err(SyncError::RevisionIdCollision),
            None => {
                missing.insert(revision.revision_id, ());
            }
        }
    }

    let topological_ids = topological_meta_order(&source_history)?;
    let mut pending = Vec::<Revision>::new();
    let mut estimated_encoded_bytes = 0_usize;
    let mut outcome = ReconcileOutcome::default();

    for revision_id in topological_ids {
        if !missing.contains_key(&revision_id) {
            continue;
        }
        let revision = source.get(revision_id)?.ok_or(SyncError::InvalidLedger)?;
        let single_revision_len = encode_batch(vault_id, std::slice::from_ref(&revision))?.len();
        if single_revision_len > SYNC_MAX_BATCH_BYTES {
            return Err(SyncError::LimitExceeded);
        }
        let would_exceed = !pending.is_empty()
            && (pending.len() >= SYNC_MAX_REVISIONS_PER_BATCH
                || estimated_encoded_bytes
                    .checked_add(single_revision_len)
                    .is_none_or(|size| size > SYNC_MAX_BATCH_BYTES));
        if would_exceed {
            commit_batch(destination, vault_id, &pending, trust, &mut outcome)?;
            pending.clear();
            estimated_encoded_bytes = 0;
        }
        estimated_encoded_bytes = estimated_encoded_bytes
            .checked_add(single_revision_len)
            .ok_or(SyncError::LimitExceeded)?;
        pending.push(revision);
    }

    if !pending.is_empty() {
        commit_batch(destination, vault_id, &pending, trust, &mut outcome)?;
    }
    Ok(outcome)
}

fn commit_batch(
    destination: &mut Ledger,
    vault_id: Id,
    revisions: &[Revision],
    trust: &dyn TrustSource,
    outcome: &mut ReconcileOutcome,
) -> Result<(), SyncError> {
    if revisions.is_empty() {
        return Ok(());
    }
    let encoded = encode_batch(vault_id, revisions)?;
    if encoded.len() > SYNC_MAX_BATCH_BYTES {
        return Err(SyncError::LimitExceeded);
    }
    let (decoded_vault_id, decoded_revisions) = decode_batch(&encoded)?;
    if decoded_vault_id != vault_id {
        return Err(SyncError::VaultMismatch);
    }
    let applied = destination.insert_batch(&decoded_revisions, trust)?;

    // `insert_batch` returns only after SQLite commit, so these counters are an
    // explicit per-call commit receipt, never a durable or unauthenticated peer ACK.
    outcome.batches_sent = outcome.batches_sent.saturating_add(1);
    outcome.revisions_sent = outcome
        .revisions_sent
        .saturating_add(decoded_revisions.len());
    outcome.duplicates = outcome.duplicates.saturating_add(applied.duplicates);
    outcome.acknowledged = outcome.acknowledged.saturating_add(decoded_revisions.len());
    Ok(())
}
