//! Isolated ledger demonstration; bytes below are synthetic opaque test data.
//! This does not open a user's vault, store keys or establish a network session.

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_sync::{Ledger, Operation, SyncError, TrustSource, fresh_id, reconcile};

struct DemoTrust([PublicIdentity; 2]);

impl TrustSource for DemoTrust {
    fn lookup(&self, id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok(self
            .0
            .iter()
            .copied()
            .find(|identity| identity.device_id == id))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let alice = DeviceIdentity::generate()?;
    let bob = DeviceIdentity::generate()?;
    let trust = DemoTrust([alice.public_identity(), bob.public_identity()]);
    let vault = fresh_id()?;
    let record = fresh_id()?;
    let mut a = Ledger::in_memory(vault)?;
    let mut b = Ledger::in_memory(vault)?;

    let initial = a.append_local(&alice, record, &[], Operation::Write, vec![0xa1], &trust)?;
    reconcile(&a, &mut b, &trust)?;

    // The core accepts already encrypted bytes. These short fixtures are not PGP.
    a.append_local(
        &alice,
        record,
        &[initial.revision_id],
        Operation::Write,
        vec![0xa2],
        &trust,
    )?;
    b.append_local(
        &bob,
        record,
        &[initial.revision_id],
        Operation::Write,
        vec![0xb2],
        &trust,
    )?;
    reconcile(&a, &mut b, &trust)?;
    reconcile(&b, &mut a, &trust)?;
    println!(
        "Offline edits preserved: {} conflicting heads",
        a.heads(record)?.len()
    );

    let parents = a
        .heads(record)?
        .iter()
        .map(|head| head.revision_id)
        .collect::<Vec<_>>();
    a.append_local(
        &alice,
        record,
        &parents,
        Operation::Write,
        vec![0xc3],
        &trust,
    )?;
    reconcile(&a, &mut b, &trust)?;
    println!("Explicit merge: {} current head", b.heads(record)?.len());

    let parents = a
        .heads(record)?
        .iter()
        .map(|head| head.revision_id)
        .collect::<Vec<_>>();
    a.append_local(
        &alice,
        record,
        &parents,
        Operation::Delete,
        Vec::new(),
        &trust,
    )?;
    reconcile(&a, &mut b, &trust)?;
    b.insert_batch(&[initial], &trust)?;
    println!(
        "Tombstone retained after ancestor replay; {} immutable revisions",
        b.revision_count()?
    );
    Ok(())
}
