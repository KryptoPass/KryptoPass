//! Private, bounded turn-by-turn protocol bridge; no production endpoint or user data.

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_protocol::{MAX_FRAME_SIZE, SessionCodec, proto};
use kryptopass_sync::{
    Ledger, Operation, Revision, SyncError, SyncOutbound, SyncSession, TrustSource,
};
use std::io::{Read, Write};
use std::time::Instant;

struct FixtureTrust([PublicIdentity; 2]);

impl TrustSource for FixtureTrust {
    fn lookup(&self, id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok(self
            .0
            .iter()
            .find(|identity| identity.device_id == id)
            .copied())
    }
}

/// The peer identity is fixed by this fixture, never accepted from incoming frames.
pub(super) fn run(local: &DeviceIdentity) -> Result<(), SyncError> {
    let peer = DeviceIdentity::from_seed_material(&[0x10; 32], &[0x20; 32]);
    let trust = FixtureTrust([local.public_identity(), peer.public_identity()]);
    let mut ledger = Ledger::in_memory([0x39; 16])?;
    let root = Revision::sign(
        [0x39; 16],
        [0x61; 16],
        [0x33; 16],
        Vec::new(),
        Operation::Write,
        vec![0xa7; 131_072],
        local,
    )?;
    let tombstone = Revision::sign(
        [0x39; 16],
        [0x61; 16],
        [0x44; 16],
        vec![root.revision_id],
        Operation::Delete,
        Vec::new(),
        local,
    )?;
    ledger.insert_batch(&[root, tombstone], &trust)?;
    let mut engine = SyncSession::new(
        ledger,
        Box::new(trust),
        local.device_id(),
        peer.public_identity(),
        vec![0x42; 20],
        Instant::now(),
    )
    .map_err(|_| SyncError::InvalidLedger)?;
    let mut codec = SessionCodec::new([0x52; 16]);
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();

    flush_round(&mut engine, &mut codec, &mut output)?;
    for _ in 0..4096 {
        let mut size_bytes = [0_u8; 4];
        input
            .read_exact(&mut size_bytes)
            .map_err(|_| SyncError::Storage)?;
        let size = u32::from_be_bytes(size_bytes) as usize;
        if size == 0 {
            let progress = engine.progress();
            return if engine.is_complete()
                && progress.received_revisions == 1
                && progress.acknowledged_revisions == 2
            {
                Ok(())
            } else {
                Err(SyncError::InvalidLedger)
            };
        }
        if size > MAX_FRAME_SIZE {
            return Err(SyncError::LimitExceeded);
        }
        let mut bytes = vec![0_u8; size];
        input
            .read_exact(&mut bytes)
            .map_err(|_| SyncError::Storage)?;
        let decoded = codec.decode(&bytes).map_err(|_| SyncError::Decode)?;
        engine
            .handle_message(decoded.message_type, decoded.payload, Instant::now())
            .map_err(|_| SyncError::InvalidLedger)?;
        flush_round(&mut engine, &mut codec, &mut output)?;
    }
    Err(SyncError::LimitExceeded)
}

fn flush_round(
    engine: &mut SyncSession,
    codec: &mut SessionCodec,
    output: &mut impl Write,
) -> Result<(), SyncError> {
    // One turn cannot contain more than one 8 MiB batch awaiting its ACK.
    for _ in 0..260 {
        let Some(outbound) = engine
            .poll_output(Instant::now())
            .map_err(|_| SyncError::InvalidLedger)?
        else {
            output
                .write_all(&0_u32.to_be_bytes())
                .map_err(|_| SyncError::Storage)?;
            output
                .write_all(&[u8::from(engine.is_complete())])
                .map_err(|_| SyncError::Storage)?;
            output.flush().map_err(|_| SyncError::Storage)?;
            return Ok(());
        };
        let bytes = match outbound {
            SyncOutbound::Hello(payload) => codec.encode(proto::MessageType::SyncHello, &payload),
            SyncOutbound::Manifest(payload) => {
                codec.encode(proto::MessageType::SyncManifest, &payload)
            }
            SyncOutbound::Request(payload) => {
                codec.encode(proto::MessageType::SyncRequest, &payload)
            }
            SyncOutbound::Chunk(payload) => codec.encode(proto::MessageType::SyncChunk, &payload),
            SyncOutbound::Commit(payload) => codec.encode(proto::MessageType::SyncCommit, &payload),
            SyncOutbound::Ack(payload) => codec.encode(proto::MessageType::SyncAck, &payload),
        }
        .map_err(|_| SyncError::Encode)?;
        let size = u32::try_from(bytes.len()).map_err(|_| SyncError::LimitExceeded)?;
        output
            .write_all(&size.to_be_bytes())
            .map_err(|_| SyncError::Storage)?;
        output.write_all(&bytes).map_err(|_| SyncError::Storage)?;
    }
    Err(SyncError::LimitExceeded)
}
