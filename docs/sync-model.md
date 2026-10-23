# Sync v1 — immutable ciphertext revision ledger

Status: Rust/Kotlin ledgers and an optional authenticated-session round implemented;
user-vault synchronization is pending.
This is independent of the GUI,
the current mutable vault tables, OpenPGP providers and network routing. No
production database is migrated by this milestone. Existing local CAS counters
are not causal revisions and must not be advertised as such.

## Responsibilities and trust

`kp-sync` stores only opaque ciphertext and public causal metadata. The caller
encrypts the complete credential (including title, username and domain) before
creating a write. It supplies the existing device identity and a `TrustSource`
backed by the pairing trust store. Parsing a revision cannot authorize an author.
The ledger is bound to one nonzero 16-byte vault ID; signatures bind that ID too.

Trust is not transitive: pairing A with C does not automatically authorize B,
even if A holds revisions signed by B. All historical authors must resolve in
the supplied trust source. The application adapter must define vault membership
and historical-author handling explicitly before supporting multi-device import.
Revoking an author currently causes `open_existing` to fail closed while retaining
the database; it does not silently remove that author's revisions.

The first milestone exchanges snapshots between isolated Rust ledgers in memory.
The optional [session adapter](sync-session-v1.md) runs over authenticated Noise,
checks the remote device/vault/fingerprint binding and shares its existing message
sequence. It requires an explicitly supplied existing ledger and remains disabled
for normal application sessions. No new socket, pairing mechanism or private-key
permission is introduced. Android remains Kotlin; protobuf and vectors are shared,
not the Rust runtime. The [vault adapters](vault-sync-adapter-v1.md) now connect
local edits and explicit migration to that ledger. The opt-in DEBUG exercise
passed [Desktop/emulator checks](vault-sync-evidence-2026-10-22.md); durable
selection and migration of normal user vaults remain pending.

## Revision identity and canonical signatures

Vault, record and revision identifiers are 16-byte values, nonzero. Locally
generated IDs use random UUID v4. Author IDs reuse `kp-identity` device-id v1.
Parents are unique, sorted in unsigned byte lexicographic order and at most 16.
An operation is `Write = 1` or `Delete = 2`. A write contains 1..2097152 bytes of
already encrypted content. A delete contains no ciphertext. `content_hash` is
SHA-256 of the ciphertext, including SHA-256(empty) for a delete.

Ed25519 signs these exact concatenated bytes, with no protobuf serialization:

1. ASCII `org.kryptopass.sync-revision.v1` followed by a zero byte.
2. Version 1 as unsigned 16-bit big endian.
3. Vault ID, record ID and revision ID, each exactly 16 bytes.
4. Parent count as unsigned 16-bit big endian, followed by the sorted parent IDs.
5. Author device ID, exactly 32 bytes.
6. Operation as one byte.
7. Ciphertext length as unsigned 32-bit big endian, followed by ciphertext.
8. Content hash, exactly 32 bytes.

The immutable revision digest is SHA-256 of ASCII
`org.kryptopass.sync-object.v1` + zero byte + canonical bytes + the 64-byte
signature. It therefore commits to all signed fields and the signature. Wire
objects use `SyncRevision`/`SyncRevisionBatch` protobuf messages transported as
opaque SyncChunk content, not Rust serialization. A bounded wire preflight runs
before allocation of repeated revisions/parents or ciphertext.

The batch codec rejects duplicate singular fields, invalid wire types, protobuf
groups and operation varints other than the literal numeric values 1 and 2.
Unknown fields are skipped within the encoded-size budget. No signed semantic
extension may be introduced through an ignored field; it needs a new documented
signature format/version. Empty delete ciphertext may be omitted by Protobuf;
this does not change the canonical signed bytes.

## Transactions and graph rules

Every accepted parent must exist in the same vault and record, either in the
existing ledger or in the same bounded batch. Input ordering is irrelevant;
cycles, self-parents, missing parents, cross-record links, unknown authors,
invalid signatures/hashes and ID collisions reject the complete transaction.
Replaying exactly the same immutable revision is idempotent. Reusing a revision
ID for different bytes is an error. No error may discard an existing branch.

The record heads are revisions not referenced as a parent by another revision
of that record. A single head is the current causal descendant; two or more
heads are a conflict, including delete-versus-edit. There is no clock-based
winner. Resolving requires an explicit new revision whose parents cover the
heads being merged. A delete remains in history; replaying its ancestors cannot
resurrect the record. An intentional new descendant can restore it.

Local append uses an exact expected-heads check inside the write transaction.
It signs a fresh immutable revision and records it before returning. Remote
batches are acknowledged only after SQLite commits. The isolated reconciliation
API returns per-call commit receipts; it does not invent a peer identity for a
database. The later authenticated session adapter must persist progress under
the verified remote device ID and revision digest, separately from the graph.
An acknowledgement cannot create or replace a revision. Interrupted transfers
retry from their last committed batch, without duplicate objects or partial commits.

Limits: 16 parents/heads per record, 1024 advertised heads per snapshot, 32
revisions and 8 MiB encoded bytes per transfer, 10000 revisions and 64 MiB total
ciphertext per isolated ledger. Exceeding a cap is an explicit failure, never
silent truncation or conflict resolution. No tombstone garbage collection yet.
The SQLite file is limited to 96 MiB (24576 pages of 4096 bytes). Unix creation
requires a private parent directory and creates the file with mode 0600;
symlinks and pre-existing SQLite sidecars are refused for a new ledger.
Other platforms need an application ACL adapter before production integration.

## Milestone API shared by implementers

Types: `Id = [u8;16]`, `Digest = [u8;32]`, `Revision`, `Operation`,
`RevisionHead { record_id, revision_id, revision_hash }`, redacted `SyncError`.
`Revision` has public immutable-object fields (owned bytes), a redacted Debug,
`sign(vault_id, record_id, revision_id, parents, operation, ciphertext, author)`,
`verify(&PublicIdentity)`, `canonical_bytes()` and `hash()` returning Result.
`TrustSource::lookup(DeviceId) -> Result<Option<PublicIdentity>, SyncError>`.

`Ledger::create(path, vault_id)`, `Ledger::in_memory(vault_id)` and
`Ledger::open_existing(path, vault_id, trust)` never create an identity or trust.
`insert_batch(&[Revision], trust)` is atomic and validates authors afresh;
`append_local(identity, record_id, expected_heads, operation, ciphertext, trust)`
returns the signed committed revision. `manifest()`, `heads(record_id)`,
`get(revision_id)`, `revision_count()` expose bounded validated history.
`encode_batch(vault_id, &[Revision])` / `decode_batch(&[u8])` are bounded codecs.
`reconcile(source, destination, trust)` sends missing ancestors in topological,
bounded transactions and acknowledges only successful commits. The function
returns counts; a second identical reconciliation transfers nothing.

## Acceptance gate

Two isolated nodes create/edit offline, exchange encrypted revision batches,
converge, preserve parallel branches and delete-versus-edit conflicts, and
reopen persisted state. Tests cover interruption/retry, out-of-order batches,
unknown author, corrupt ciphertext, invalid graph, transaction rollback,
limits, protobuf fuzz/property input and deterministic cross-language vectors.
No real user keys, vault writes or UI changes are part of these tests.

`just test-sync` runs the bounded functional suite. The 10000-revision capacity
stress case is opt-in through `just test-sync-stress`; its runtime and maximum
capacity are not certified by the default suite.

## Remaining application gate

- The Kotlin model, ledger, SQLite adapter and shared-vector/live-Rust tests
  are implemented. Keep both implementations within the same canonical contract.
- The optional dispatcher negotiates one explicit vault/fingerprint and capability
  in the existing Noise session. It never merges unrelated vaults or grants author
  trust. Manifests, requests, commits and ACK are scoped to its frozen snapshots.
- The committed ledger provides restart/retry progress. Persist separate peer
  receipt journals and schedule new rounds for live edits in the app adapter.
- Adapt both existing vault stores without exposing plaintext metadata, with a
  reversible migration and transactional consistency between edits and revisions.
- Exercise real OpenPGP ciphertext, process restart, cancellation, interrupted
  network transfers and conflict presentation before enabling application sync.

The public relay remains an untrusted signaling service; it does not acquire a
vault database, decryption keys or a plaintext credential API for this milestone.
