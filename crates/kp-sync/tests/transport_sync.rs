#![allow(clippy::expect_used)] // In-memory peer setup should name the failed protocol step.

use std::collections::HashMap;

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_protocol::{
    Channel, Fragment, FragmentAssembler, FragmentLimits, MAX_FRAGMENT_SIZE, MessagePayload,
    SessionCodec, proto,
};
use kryptopass_sync::{
    Id, InsertOutcome, Ledger, Operation, SyncError, TrustSource, decode_batch, encode_batch,
};
use kryptopass_transport::{MemoryTransport, MemoryTransportConfig, PeerTransport, TransportFrame};
use sha2::{Digest, Sha256};

const VAULT: Id = [0x61; 16];

struct TestTrust(HashMap<DeviceId, PublicIdentity>);

impl TestTrust {
    fn with(identity: &DeviceIdentity) -> Self {
        Self(HashMap::from([(
            identity.device_id(),
            identity.public_identity(),
        )]))
    }

    fn empty() -> Self {
        Self(HashMap::new())
    }
}

impl TrustSource for TestTrust {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok(self.0.get(&device_id).copied())
    }
}

fn fragment_messages(bytes: &[u8], transfer_id: Id) -> (Vec<proto::SyncChunk>, [u8; 32]) {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    let chunks = bytes.chunks(MAX_FRAGMENT_SIZE).collect::<Vec<_>>();
    let count = u32::try_from(chunks.len()).expect("bounded fragment count");
    let messages = chunks
        .into_iter()
        .enumerate()
        .map(|(index, data)| proto::SyncChunk {
            transfer_id: transfer_id.to_vec(),
            chunk_index: u32::try_from(index).expect("bounded chunk index"),
            chunk_count: count,
            total_size: u64::try_from(bytes.len()).expect("bounded transfer size"),
            sha256_digest: digest.to_vec(),
            data: data.to_vec(),
        })
        .collect();
    (messages, digest)
}

fn receive_chunk(
    frame: TransportFrame,
    codec: &mut SessionCodec,
    assembler: &mut FragmentAssembler,
) -> Option<(Id, [u8; 32], Vec<u8>)> {
    let decoded = codec
        .decode_on(frame.channel, &frame.data)
        .expect("protocol validates incoming fragment");
    let MessagePayload::SyncChunk(chunk) = decoded.payload else {
        panic!("expected a synchronization chunk");
    };
    let transfer_id: Id = chunk
        .transfer_id
        .as_slice()
        .try_into()
        .expect("validated transfer ID");
    let digest: [u8; 32] = chunk
        .sha256_digest
        .as_slice()
        .try_into()
        .expect("validated transfer digest");
    let complete = assembler
        .push(Fragment {
            transfer_id: &chunk.transfer_id,
            index: chunk.chunk_index,
            count: chunk.chunk_count,
            total_size: usize::try_from(chunk.total_size).expect("transfer fits usize"),
            digest: &chunk.sha256_digest,
            data: &chunk.data,
        })
        .expect("bounded fragment is valid");
    complete.map(|complete| (transfer_id, digest, complete))
}

/// Test-side receiver rule used by both success and rejection flows: send the
/// acknowledgement only after `insert_batch` has returned from SQLite commit.
async fn commit_received_batch_then_ack(
    destination: &mut Ledger,
    trust: &dyn TrustSource,
    complete_batch: &[u8],
    expected_transfer_id: Id,
    expected_digest: [u8; 32],
    receiver: &mut MemoryTransport,
    receiver_codec: &mut SessionCodec,
) -> Result<InsertOutcome, SyncError> {
    let commit_frame = receiver.receive().await.expect("peer receives SyncCommit");
    let decoded = receiver_codec
        .decode_on(commit_frame.channel, &commit_frame.data)
        .expect("commit follows the session sequence");
    let MessagePayload::SyncCommit(commit) = decoded.payload else {
        panic!("expected SyncCommit");
    };
    assert_eq!(
        commit.transfer_id.as_slice(),
        expected_transfer_id.as_slice()
    );
    assert_eq!(commit.sha256_digest.as_slice(), expected_digest.as_slice());
    let complete_digest: [u8; 32] = Sha256::digest(complete_batch).into();
    assert_eq!(complete_digest, expected_digest);

    let (batch_vault, revisions) = decode_batch(complete_batch)?;
    if batch_vault != destination.vault_id() {
        return Err(SyncError::VaultMismatch);
    }
    let before = destination.revision_count()?;
    let inserted = destination.insert_batch(&revisions, trust)?;
    assert_eq!(destination.revision_count()?, before + inserted.inserted);

    let ack = proto::SyncAck {
        transfer_id: expected_transfer_id.to_vec(),
        status: proto::AckStatus::Accepted as i32,
    };
    let encoded_ack = receiver_codec
        .encode(proto::MessageType::SyncAck, &ack)
        .expect("ack is bounded");
    receiver
        .send(Channel::Control, &encoded_ack)
        .await
        .expect("ack follows the successful SQLite commit");
    Ok(inserted)
}

#[tokio::test]
async fn fragmented_two_node_transfer_retries_and_acks_only_after_sqlite_commit() {
    let sender_identity = DeviceIdentity::from_seed_material(&[0x31; 32], &[0x32; 32]);
    let trust = TestTrust::with(&sender_identity);
    let mut source = Ledger::in_memory(VAULT).expect("source SQLite ledger");
    let mut destination = Ledger::in_memory(VAULT).expect("destination SQLite ledger");
    let revision = source
        .append_local(
            &sender_identity,
            [0x62; 16],
            &[],
            Operation::Write,
            vec![0xa7; 150_000],
            &trust,
        )
        .expect("large encrypted revision is committed locally");
    let batch = encode_batch(VAULT, std::slice::from_ref(&revision))
        .expect("bounded protobuf revision batch");
    let first_transfer = [0x71; 16];
    let (first_messages, _) = fragment_messages(&batch, first_transfer);
    assert!(first_messages.len() > 1, "payload must use multiple chunks");

    let mut sender_codec = SessionCodec::new([0x70; 16]);
    let mut receiver_codec = SessionCodec::new([0x70; 16]);
    let mut assembler =
        FragmentAssembler::new(FragmentLimits::default()).expect("assembler limits");
    let (mut sender, mut receiver) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("first memory link");

    let encoded_first = sender_codec
        .encode(proto::MessageType::SyncChunk, &first_messages[0])
        .expect("first chunk is bounded");
    sender
        .send(Channel::Data, &encoded_first)
        .await
        .expect("first chunk enters the memory link");
    let received_first = receiver.receive().await.expect("peer receives first chunk");
    assert!(receive_chunk(received_first, &mut receiver_codec, &mut assembler).is_none());
    assert_eq!(destination.revision_count().expect("no partial write"), 0);
    assert!(
        assembler.cancel(&first_transfer),
        "discard incomplete assembly"
    );
    sender.close().await.expect("interrupt first transfer");
    assert!(
        sender.receive().await.is_err(),
        "interrupted transfer emitted no ack"
    );

    let retry_transfer = [0x72; 16];
    let (retry_messages, retry_digest) = fragment_messages(&batch, retry_transfer);
    let (mut sender, mut receiver) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("retry memory link");
    // A new carrier has a fresh authenticated session sequence and assembler.
    let mut sender_codec = SessionCodec::new([0x73; 16]);
    let mut receiver_codec = SessionCodec::new([0x73; 16]);
    let mut assembler =
        FragmentAssembler::new(FragmentLimits::default()).expect("assembler limits");
    let mut complete_batch = None;
    for chunk in &retry_messages {
        let encoded = sender_codec
            .encode(proto::MessageType::SyncChunk, chunk)
            .expect("retry chunk is bounded");
        sender
            .send(Channel::Data, &encoded)
            .await
            .expect("retry chunk enters the memory link");
        let received = receiver.receive().await.expect("peer receives retry chunk");
        if let Some(complete) = receive_chunk(received, &mut receiver_codec, &mut assembler) {
            complete_batch = Some(complete);
        }
    }
    let (completed_transfer, completed_digest, complete_batch) =
        complete_batch.expect("all bounded chunks reassemble");
    assert_eq!(completed_transfer, retry_transfer);
    assert_eq!(completed_digest, retry_digest);

    let commit = proto::SyncCommit {
        transfer_id: retry_transfer.to_vec(),
        sha256_digest: retry_digest.to_vec(),
    };
    let encoded_commit = sender_codec
        .encode(proto::MessageType::SyncCommit, &commit)
        .expect("commit is bounded");
    sender
        .send(Channel::Control, &encoded_commit)
        .await
        .expect("commit enters the memory link");
    let inserted = commit_received_batch_then_ack(
        &mut destination,
        &trust,
        &complete_batch,
        retry_transfer,
        retry_digest,
        &mut receiver,
        &mut receiver_codec,
    )
    .await
    .expect("destination commits then acknowledges");
    assert_eq!(inserted.inserted, 1);
    assert_eq!(destination.revision_count().expect("committed row"), 1);

    let received_ack = sender
        .receive()
        .await
        .expect("sender receives committed ack");
    let decoded_ack = sender_codec
        .decode_on(received_ack.channel, &received_ack.data)
        .expect("ack follows the reverse session sequence");
    let MessagePayload::SyncAck(decoded_ack) = decoded_ack.payload else {
        panic!("expected SyncAck");
    };
    assert_eq!(
        decoded_ack.transfer_id.as_slice(),
        retry_transfer.as_slice()
    );
    assert_eq!(decoded_ack.status, proto::AckStatus::Accepted as i32);
    assert_eq!(
        destination
            .get(revision.revision_id)
            .expect("stored revision"),
        Some(revision)
    );
}

#[tokio::test]
async fn unknown_author_transfer_is_rejected_without_acknowledgement() {
    let unknown = DeviceIdentity::from_seed_material(&[0x41; 32], &[0x42; 32]);
    let sender_trust = TestTrust::with(&unknown);
    let receiver_trust = TestTrust::empty();
    let mut source = Ledger::in_memory(VAULT).expect("source SQLite ledger");
    let mut destination = Ledger::in_memory(VAULT).expect("destination SQLite ledger");
    let revision = source
        .append_local(
            &unknown,
            [0x63; 16],
            &[],
            Operation::Write,
            b"opaque unknown-author ciphertext".to_vec(),
            &sender_trust,
        )
        .expect("source has a locally authorized author");
    let batch = encode_batch(VAULT, std::slice::from_ref(&revision))
        .expect("bounded protobuf revision batch");
    let transfer_id = [0x75; 16];
    let (messages, digest) = fragment_messages(&batch, transfer_id);
    let (mut sender, mut receiver) =
        MemoryTransport::pair(MemoryTransportConfig::default()).expect("memory link");
    let mut sender_codec = SessionCodec::new([0x76; 16]);
    let mut receiver_codec = SessionCodec::new([0x76; 16]);
    let mut assembler =
        FragmentAssembler::new(FragmentLimits::default()).expect("assembler limits");
    let mut complete_batch = None;

    for chunk in &messages {
        let encoded = sender_codec
            .encode(proto::MessageType::SyncChunk, chunk)
            .expect("chunk is bounded");
        sender
            .send(Channel::Data, &encoded)
            .await
            .expect("chunk enters the link");
        let received = receiver.receive().await.expect("peer receives chunk");
        if let Some(complete) = receive_chunk(received, &mut receiver_codec, &mut assembler) {
            complete_batch = Some(complete.2);
        }
    }
    let complete_batch = complete_batch.expect("batch reassembles before commit");
    let commit = proto::SyncCommit {
        transfer_id: transfer_id.to_vec(),
        sha256_digest: digest.to_vec(),
    };
    let encoded_commit = sender_codec
        .encode(proto::MessageType::SyncCommit, &commit)
        .expect("commit is bounded");
    sender
        .send(Channel::Control, &encoded_commit)
        .await
        .expect("commit enters the link");

    assert!(matches!(
        commit_received_batch_then_ack(
            &mut destination,
            &receiver_trust,
            &complete_batch,
            transfer_id,
            digest,
            &mut receiver,
            &mut receiver_codec,
        )
        .await,
        Err(SyncError::UnknownAuthor)
    ));
    assert_eq!(
        destination
            .revision_count()
            .expect("rejected batch is atomic"),
        0
    );
    receiver.close().await.expect("close rejected transfer");
    assert!(
        sender.receive().await.is_err(),
        "rejected batch emitted no ack"
    );
}
