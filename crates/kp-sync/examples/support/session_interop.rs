//! Private, bounded turn-by-turn protocol bridge; no production endpoint or user data.

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_protocol::{MAX_FRAME_SIZE, SessionCodec, proto};
use kryptopass_sync::{
    Ledger, Operation, Revision, SyncError, SyncOutbound, SyncSession, TrustSource,
};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const APPEND_BEFORE_INITIAL_HELLO_COMMAND: u32 = u32::MAX;
const REQUEST_NEXT_ROUND_COMMAND: u32 = u32::MAX - 1;
const APPEND_ONE_COMMAND: u32 = u32::MAX - 2;
const APPEND_PAGED_HEADS_COMMAND: u32 = u32::MAX - 3;
const APPENDED_HEAD_COUNT: usize = 300;
const EXPECTED_FINAL_REVISION_COUNT: usize = APPENDED_HEAD_COUNT * 2 + 3;

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

/// Continuous-session interop case: both peers start empty, then test-only
/// commands append local revisions through a second ledger handle between rounds.
pub(super) fn run_continuous_empty(local: &DeviceIdentity) -> Result<(), SyncError> {
    let peer = DeviceIdentity::from_seed_material(&[0x10; 32], &[0x20; 32]);
    let trusted = [local.public_identity(), peer.public_identity()];
    let temporary_ledger = TemporaryLedger::new()?;
    let ledger = Ledger::create(&temporary_ledger.path, [0x39; 16])?;
    let mut engine = SyncSession::new(
        ledger,
        Box::new(FixtureTrust(trusted)),
        local.device_id(),
        peer.public_identity(),
        vec![0x42; 20],
        Instant::now(),
    )
    .map_err(|_| SyncError::InvalidLedger)?;
    engine
        .enable_continuous_rounds()
        .map_err(|_| SyncError::InvalidLedger)?;
    let mutation_trust = FixtureTrust(trusted);
    let mut mutation_ledger =
        Ledger::open_existing(&temporary_ledger.path, [0x39; 16], &mutation_trust)?;

    let mut codec = SessionCodec::new([0x52; 16]);
    let mut logical_now = Instant::now();
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();

    // Reproduce a write after SyncHello was queued with known_revision_count=0,
    // but before that Hello is sent to the peer.
    let mut command_bytes = [0_u8; 4];
    input
        .read_exact(&mut command_bytes)
        .map_err(|_| SyncError::Storage)?;
    if u32::from_be_bytes(command_bytes) != APPEND_BEFORE_INITIAL_HELLO_COMMAND {
        return Err(SyncError::InvalidLedger);
    }
    append_local_heads(&mut mutation_ledger, local, &mutation_trust, 1, 1)?;
    flush_round_at(&mut engine, &mut codec, &mut output, logical_now)?;

    for _ in 0..4096 {
        let mut size_bytes = [0_u8; 4];
        input
            .read_exact(&mut size_bytes)
            .map_err(|_| SyncError::Storage)?;
        let size = u32::from_be_bytes(size_bytes);
        match size {
            0 => {
                let revision_count = mutation_ledger.revision_count()?;
                return if engine.is_complete() && revision_count == EXPECTED_FINAL_REVISION_COUNT {
                    Ok(())
                } else {
                    Err(SyncError::InvalidLedger)
                };
            }
            REQUEST_NEXT_ROUND_COMMAND => {
                request_and_flush_next_round(
                    &mut engine,
                    &mut codec,
                    &mut output,
                    &mut logical_now,
                )?;
            }
            APPEND_ONE_COMMAND => {
                append_local_heads(&mut mutation_ledger, local, &mutation_trust, 2, 1)?;
                request_and_flush_next_round(
                    &mut engine,
                    &mut codec,
                    &mut output,
                    &mut logical_now,
                )?;
            }
            APPEND_PAGED_HEADS_COMMAND => {
                append_local_heads(
                    &mut mutation_ledger,
                    local,
                    &mutation_trust,
                    3,
                    APPENDED_HEAD_COUNT,
                )?;
                request_and_flush_next_round(
                    &mut engine,
                    &mut codec,
                    &mut output,
                    &mut logical_now,
                )?;
            }
            _ if size as usize > MAX_FRAME_SIZE => return Err(SyncError::LimitExceeded),
            _ => {
                let mut bytes = vec![0_u8; size as usize];
                input
                    .read_exact(&mut bytes)
                    .map_err(|_| SyncError::Storage)?;
                let decoded = codec.decode(&bytes).map_err(|_| SyncError::Decode)?;
                engine
                    .handle_message(decoded.message_type, decoded.payload, logical_now)
                    .map_err(|_| SyncError::InvalidLedger)?;
                flush_round_at(&mut engine, &mut codec, &mut output, logical_now)?;
            }
        }
    }
    Err(SyncError::LimitExceeded)
}

fn request_and_flush_next_round(
    engine: &mut SyncSession,
    codec: &mut SessionCodec,
    output: &mut impl Write,
    logical_now: &mut Instant,
) -> Result<(), SyncError> {
    if !engine.is_complete() || !engine.request_round() {
        return Err(SyncError::InvalidLedger);
    }
    *logical_now += Duration::from_secs(2);
    flush_round_at(engine, codec, output, *logical_now)
}

fn append_local_heads(
    ledger: &mut Ledger,
    identity: &DeviceIdentity,
    trust: &dyn TrustSource,
    group: u8,
    count: usize,
) -> Result<(), SyncError> {
    for index in 0..count {
        let mut record_id = [0_u8; 16];
        record_id[0] = 0x71;
        record_id[1] = group;
        record_id[12..].copy_from_slice(&(index as u32).to_be_bytes());
        let mut ciphertext = vec![0xa7; 32];
        ciphertext[0] = group;
        ciphertext[1..5].copy_from_slice(&(index as u32).to_be_bytes());
        ledger.append_local(
            identity,
            record_id,
            &[],
            Operation::Write,
            ciphertext,
            trust,
        )?;
    }
    Ok(())
}

struct TemporaryLedger {
    directory: PathBuf,
    path: PathBuf,
}

impl TemporaryLedger {
    fn new() -> Result<Self, SyncError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| SyncError::Storage)?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "kryptopass-sync-session-{}-{nonce}.sqlite",
            std::process::id()
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).map_err(|_| SyncError::Storage)?;
        let path = directory.join("ledger.sqlite");
        Ok(Self { directory, path })
    }
}

impl Drop for TemporaryLedger {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let mut wal = self.path.as_os_str().to_os_string();
        wal.push("-wal");
        let _ = std::fs::remove_file(PathBuf::from(wal));
        let mut shm = self.path.as_os_str().to_os_string();
        shm.push("-shm");
        let _ = std::fs::remove_file(PathBuf::from(shm));
        let _ = std::fs::remove_dir(&self.directory);
    }
}

fn flush_round(
    engine: &mut SyncSession,
    codec: &mut SessionCodec,
    output: &mut impl Write,
) -> Result<(), SyncError> {
    flush_round_at(engine, codec, output, Instant::now())
}

fn flush_round_at(
    engine: &mut SyncSession,
    codec: &mut SessionCodec,
    output: &mut impl Write,
    now: Instant,
) -> Result<(), SyncError> {
    // One turn cannot contain more than one 8 MiB batch awaiting its ACK.
    for _ in 0..260 {
        let Some(outbound) = engine
            .poll_output(now)
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
