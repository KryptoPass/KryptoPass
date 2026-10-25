//! Bounded stdin/stdout bridge for synthetic Kotlin interoperability tests only.
//! This executable is not a peer endpoint and never loads user identities.

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_sync::{
    Ledger, Operation, SYNC_MAX_BATCH_BYTES, SyncError, TrustSource, decode_batch, encode_batch,
};
use std::io::{Read, Write};

#[path = "support/session_interop.rs"]
mod session_interop;

const MAGIC: &[u8; 5] = b"KPSI1";
const HEADER_SIZE: usize = 5 + 32 + 32 + 4;

struct FixtureTrust(PublicIdentity);

impl TrustSource for FixtureTrust {
    fn lookup(&self, id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok((id == self.0.device_id).then_some(self.0))
    }
}

fn fixture_identity() -> DeviceIdentity {
    let static_seed = std::array::from_fn(|index| index as u8);
    let signing_seed = std::array::from_fn(|index| (index + 32) as u8);
    DeviceIdentity::from_seed_material(&static_seed, &signing_seed)
}

fn export_synthetic(identity: &DeviceIdentity) -> Result<(), SyncError> {
    let vault = [0x11; 16];
    let record = [0x22; 16];
    let root = kryptopass_sync::Revision::sign(
        vault,
        record,
        [0x33; 16],
        Vec::new(),
        Operation::Write,
        vec![0xa7; 131_072],
        identity,
    )?;
    let delete = kryptopass_sync::Revision::sign(
        vault,
        record,
        [0x44; 16],
        vec![root.revision_id],
        Operation::Delete,
        Vec::new(),
        identity,
    )?;
    let batch = encode_batch(vault, &[root, delete])?;
    let public = identity.public_identity();
    let mut output = std::io::stdout().lock();
    output.write_all(MAGIC).map_err(|_| SyncError::Storage)?;
    output
        .write_all(&public.signing_public_key)
        .map_err(|_| SyncError::Storage)?;
    output
        .write_all(&public.static_public_key)
        .map_err(|_| SyncError::Storage)?;
    let length = u32::try_from(batch.len()).map_err(|_| SyncError::LimitExceeded)?;
    output
        .write_all(&length.to_be_bytes())
        .map_err(|_| SyncError::Storage)?;
    output.write_all(&batch).map_err(|_| SyncError::Storage)?;
    Ok(())
}

fn accept_synthetic(identity: &DeviceIdentity) -> Result<(), SyncError> {
    let mut input = Vec::new();
    std::io::stdin()
        .lock()
        .take((HEADER_SIZE + SYNC_MAX_BATCH_BYTES + 1) as u64)
        .read_to_end(&mut input)
        .map_err(|_| SyncError::Storage)?;
    if input.len() < HEADER_SIZE || input.len() > HEADER_SIZE + SYNC_MAX_BATCH_BYTES {
        return Err(SyncError::Decode);
    }
    let expected = identity.public_identity();
    // Trust comes from this fixed fixture, never from a key supplied in stdin.
    if input.get(..5) != Some(MAGIC.as_slice())
        || input.get(5..37) != Some(expected.signing_public_key.as_slice())
        || input.get(37..69) != Some(expected.static_public_key.as_slice())
    {
        return Err(SyncError::InvalidSignature);
    }
    let length = input
        .get(69..73)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(u32::from_be_bytes)
        .ok_or(SyncError::Decode)? as usize;
    if length != input.len() - HEADER_SIZE {
        return Err(SyncError::Decode);
    }
    let (vault_id, revisions) = decode_batch(&input[HEADER_SIZE..])?;
    let mut ledger = Ledger::in_memory(vault_id)?;
    ledger.insert_batch(&revisions, &FixtureTrust(expected))?;
    println!("ok {}", ledger.revision_count()?);
    Ok(())
}

fn main() -> std::process::ExitCode {
    let identity = fixture_identity();
    let result = match std::env::args().nth(1).as_deref() {
        Some("export-synthetic") => export_synthetic(&identity),
        Some("accept-synthetic") => accept_synthetic(&identity),
        Some("session-synthetic") => session_interop::run(&identity),
        Some("session-continuous-empty-synthetic") => {
            session_interop::run_continuous_empty(&identity)
        }
        _ => Err(SyncError::Decode),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("synthetic sync interop rejected: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
