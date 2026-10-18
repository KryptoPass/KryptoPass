# KryptoPass peer protocol v1

This document defines the Phase 1 wire contract. The normative schema is
[crates/kp-protocol/proto/protocol.proto](../crates/kp-protocol/proto/protocol.proto),
with protobuf package **org.kryptopass.protocol.v1**. Rust types are generated
with prost-build; no Rust struct layout or serde format is part of the wire
format. Generated messages deliberately omit Debug because fields include
invitation credentials and opaque transfer data.

The `kp-protocol` crate provides schema, validation, a bounded session codec,
and bounded fragment reassembly. Pairing, Noise, SAS derivation, and trust
persistence are implemented by `kp-pairing` and documented in
[pairing-security.md](pairing-security.md). Synchronization and key
import/export remain inactive; decoding their reserved messages does not
perform those operations.

## Envelope and framing

Every in-session frame is one protobuf Envelope:

| Field | Contract |
| --- | --- |
| protocol_version | Exactly 1; other versions are rejected before dispatch. |
| session_id | Exactly 16 bytes, not all zero. SessionCodec checks the expected value; the caller supplies a fresh cryptographically random ID for each session. |
| message_id | Non-zero, starts at 1, and increments by exactly one for each sender direction. |
| message_type | A defined, non-zero MessageType. Unknown and unspecified values are rejected. |
| payload | The protobuf encoding of exactly the message selected by message_type. Required fields, lengths, and per-message limits are checked. |

Frames are limited to 65,536 bytes before the envelope is decoded. The encoded
payload limit is 65,472 bytes, leaving room for envelope fields. Transports
must preserve frame boundaries. There is no additional length prefix in this
contract.

Each peer has an independent inbound and outbound sequence. The receiver holds
only the next expected inbound ID, so replay and ordering checks use constant
memory. A duplicate is rejected as replay; a gap is rejected as out of order.
The inbound counter advances only after version, session, payload wire
structure, and typed fields pass validation. When decode_on is used, the
transport channel must also match. A malformed frame can
therefore be corrected and retried with the same ID. Sequence state belongs
to one session and must not be resumed after reconnecting.

SessionCodec.encode consumes the next outbound ID when it returns a frame.
If sending that frame fails before the transport accepts it, retry those exact
bytes. Encoding a replacement frame would leave a gap. If the transport cannot
tell whether a frame was accepted, discard the session and create a new one.

Channel::Control carries pairing and synchronization metadata.
Channel::Data carries SyncChunk and the reserved KeyTransferChunk.
Channel is transport metadata, not an extra protobuf field.
SessionCodec.decode_on verifies that transport metadata agrees with the
message type. A transport must use one FIFO sequence per direction across
both channels; prioritizing one channel in a way that reorders frames causes
the sequence check to reject the later frame.

Before decoding a payload into generated types, the codec scans the protobuf
wire fields without allocating. It bounds repeated manifest/request/conflict
entries and chunk data. The envelope cap bounds all remaining protobuf work.
Unknown protobuf fields are ignored according to normal protobuf semantics;
unknown enum values and invalid message types are rejected. No signature is
computed over serialized protobuf bytes.

## Message families

Byte lengths below are enforced by the codec. IDs have the listed length and
cannot be all zero; individual zero bytes are valid. Hashes and public keys
are fixed-width byte strings. Repeated entries must be unique where stated.

| Type | Required fields and limits | Lane / meaning |
| --- | --- | --- |
| PairingInvite | invite_id: 16 bytes, not all zero; single_use_token: 32 bytes, not all zero; expires_at_unix_ms > 0 with no codec-level upper bound; protocol_version = 1. Standalone encoding is capped at 512 bytes. | Out-of-band bootstrap object, such as a QR. SessionCodec rejects this type so the bearer token cannot be sent inside a peer session frame. It must not be logged or shown in debug output. The pairing engine limits hosted invitation creation to 10 minutes; see [pairing-security.md](pairing-security.md). |
| PairingHello | invite_id: 16 bytes. | Control. The ephemeral Noise key is carried in Noise XX message 1, avoiding a second source of truth. |
| NoiseHandshake | step in 1–3; non-empty handshake_message, within the payload cap. | Control. NoiseHandshakeOrder enforces the complete role-specific turn order; the codec itself checks only the range and frame sequence. |
| PairingIdentity | device_id: 32 bytes; Ed25519 signing_public_key: 32; transcript_hash: 32; proof: 64. | Control. X25519 static identity is obtained from the Noise handshake rather than duplicated here. |
| PairingConfirmation | transcript_hash: 32; accepted is the sender's local decision. | Control. A received accepted=true never counts as this device's local approval. |
| PairingComplete | transcript_hash: 32. | Control event only; it does not create trust. |
| PairingAbort | A defined non-zero AbortCode. | Control. |
| SyncHello | vault_id: 16; device_id: 32; known_revision_count ≤ 1,000,000. | Control. |
| SyncManifest | snapshot_id: 16; at most 1,024 heads. Each head has a 16-byte record ID, 16-byte revision ID, and 32-byte revision hash; (record_id, revision_id) pairs are unique. | Control. |
| SyncRequest | snapshot_id: 16; at most 256 requested (record_id, revision_id) pairs, each ID 16 bytes and unique. | Control. |
| SyncChunk | transfer_id: 16; 0 ≤ chunk_index < chunk_count ≤ 256; total_size 1–8 MiB and at least the chunk count; SHA-256 digest: 32; data: 1–48 KiB. | Data. Reassembled bytes must match the declared size and SHA-256 digest. Data is opaque and is expected to be encrypted vault content. |
| SyncCommit | transfer_id: 16; SHA-256 digest: 32. | Control. |
| SyncAck | transfer_id: 16; defined, non-zero AckStatus. | Control. |
| SyncConflict | record_id: 16; 2–16 unique, non-zero revision IDs of 16 bytes each. | Control. |
| KeyTransferOffer | transfer_id: 16; OpenPGP fingerprint: 20 or 32 bytes; total_size: 1–8 MiB. | Reserved control message. |
| KeyTransferChunk | Same fragment limits and digest fields as SyncChunk. | Reserved data message. |
| KeyTransferComplete | transfer_id: 16; OpenPGP fingerprint: 20 or 32 bytes. | Reserved control message. |
| KeyTransferAbort | transfer_id: 16; defined non-zero AbortCode. | Reserved control message. |

The key-transfer types are schema and codec definitions only. No Phase 1
component exports, imports, persists, or authorizes private-key material.
Applications must not attach an operation handler to these messages yet.

## Pairing identity fields

The pairing engine's canonical identity proof, Noise prologue, invitation
token hash, SAS derivation, confirmation boundary, and reconnect checks are
defined in [pairing-security.md](pairing-security.md). The device identifier is
SHA-256(ASCII("org.kryptopass.device-id.v1") || 0x00 || signing_public_key).
The codec validates lengths only; it does not verify the signature or device
identifier. PairingComplete remains an authenticated readiness event and is
not proof that the remote trust database committed.

NoiseHandshakeOrder is a role-aware sequence guard, not a Noise implementation.
It allows only the current global turn: the initiator sends step 1, receives
step 2, then sends step 3; the responder receives step 1, sends step 2, then
receives step 3. It does not expose a later local turn early. Call
record_outgoing only after the transport accepts the encoded frame; call
record_incoming after the codec accepts the corresponding frame. Any mismatch
ends the handshake.

PairingInvite is an out-of-band bootstrap object. Its single-use token is a
bearer credential, not a secret created by Noise. The `kp-pairing` crate
registers the token hash in the local ledger and consumes the responder's
single-use attempt before accepting peer bytes. See
[pairing-security.md](pairing-security.md) for expiry and attempt semantics.

## Fragment assembly

FragmentAssembler is an in-memory primitive that the caller creates per
session. Its default limits allow at most two incomplete transfers, 8 MiB of
declared bytes reserved in total, 8 MiB per transfer, 256 fragments, and
48 KiB per fragment. Incomplete assemblies expire after 60 seconds when
push or expire_stale is called. cancel immediately releases one assembly.
Dropping the assembler releases all of its state.

Each fragment repeats its transfer ID, count, total size, and digest. Metadata
must agree across fragments. Identical duplicate fragments are idempotent;
conflicting duplicates fail. The assembler rejects empty chunks, impossible
size/count combinations, sums beyond the declared total, and a final size or
digest mismatch. A metadata conflict leaves the bounded assembly available for
explicit cancellation or timeout cleanup; callers should cancel a transfer
after an assembly error.

The byte budget tracks declared incomplete-transfer sizes. During completion,
the returned contiguous vector is allocated while fragment buffers are being
consumed, so total transfer bytes can briefly reach about 16 MiB with default
limits, plus per-fragment vector/allocation overhead. The limit is not a
process-wide memory quota. Callers must also bound the number of assembler
instances.

## Kotlin interoperability

Kotlin/Java clients must generate protobuf types from the checked-in
protocol.proto, use package org.kryptopass.protocol.v1, and follow the
numeric enum and field tags in that source. They must implement the same
required-field checks, frame and field caps, per-direction message sequence,
channel mapping, and fragment digest rules. Do not derive a signature from
protobuf-encoded bytes. Kotlin and Rust implementations must use the byte-exact
prologue, identity-proof, Noise, and SAS contract in
[pairing-security.md](pairing-security.md).

uint64 sizes and counts are bounded by this contract. The invitation timestamp
is checked by the codec only for being greater than zero; the pairing engine
applies the invitation expiry and 10-minute creation policy described in
[pairing-security.md](pairing-security.md). message_id is a raw unsigned sequence value; a Kotlin/JVM
implementation must compare it as unsigned if represented by a signed Long.
For message_id and expires_at_unix_ms, Kotlin must preserve uint64 semantics.
For bounded data sizes and counts, reject negative JVM Long values before
conversion.

Field numbers and enum numbers are stable public identifiers. Removed field
numbers and names remain reserved in the schema and must never be reused.
Changing a required semantic or any size limit requires a new protocol version
or a documented compatible extension.

## Codec boundaries

The codec is not an authenticated channel. It does not implement Noise,
encryption, signatures, emoji SAS, trust persistence, synchronization, or key
import/export. Those pairing duties are implemented by `kp-pairing` and
documented in [pairing-security.md](pairing-security.md). The in-memory
transport is only a deterministic test adapter. Synchronization and key
transfer remain reserved and have no active handlers.
