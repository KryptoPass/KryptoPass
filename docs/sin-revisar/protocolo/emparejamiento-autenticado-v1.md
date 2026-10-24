# KryptoPass authenticated pairing v1

This document fixes the cryptographic and state-machine contract implemented
by the Rust pairing engine. The wire schema remains
[org.kryptopass.protocol.v1](protocolo-peer-v1.md). Public identities and
durable trust records are provided by `kp-identity`; frames and sequence
numbers are provided by `kp-protocol`; frame delivery is provided by
`kp-transport`.

The engine uses Noise XX with the exact suite
`Noise_XX_25519_AESGCM_SHA256`. It does not use OpenPGP keys. The Ed25519
device-signing key authenticates the final transcript, and the device's static
X25519 key is supplied to Noise. Each session uses a fresh, non-zero 16-byte
session ID. Noise handshake plaintext is at most 65,519 bytes and ciphertext
is at most 65,535 bytes. XX handshake messages have a separate 1,024-byte cap;
the only handshake payloads are 32-byte Ed25519 public keys.

## Invitation and session bootstrap

A hosted invitation contains a random 16-byte invite ID, a random 32-byte
single-use token, an expiry in Unix milliseconds, and protocol version 1. Its
Protocol Buffers encoding is bounded to 512 bytes and is intended for an
out-of-band channel such as a QR code. The QR object carries the raw token; the
pairing engine never sends that raw token in a peer frame and never logs it.
The token is held in zeroizing memory by the pairing types. The identity
database stores only
`SHA-256(b"org.kryptopass.invitation-token-hash.v1\0" || raw_token_32)`.

The invitation creator is the host and must run as Noise responder. It
registers the ID and token hash in its local SQLite ledger. The QR scanner must
run as Noise initiator; it does not consume the responder's ledger. The host
responder consumes that ledger entry transactionally before it reads any peer
bytes. An interrupted attempt remains consumed, including after a crash; the
user must create a new invitation. This intentionally accepts the
possibility that an attacker who can reach the responder may cause an
invitation to be consumed by starting an incomplete attempt. The ledger never
stores the raw token and is bounded by `kp-identity`.

The application samples the `now_ms` argument when calling the pairing or
reconnect config constructor. The config retains that monotonic origin, and
the engine adds elapsed monotonic time when checking expiry, consuming the
invitation, and recording trust. A config delayed before `run` therefore
cannot extend the invitation's lifetime; cloning the config preserves the
original origin.

In this first transport adapter, both applications provide the same fresh
session ID in configuration. The initiator sends `PairingHello` with the
expected invite ID; the responder checks it before starting Noise. In
reconnect mode, the schema still requires a non-zero `PairingHello.invite_id`,
so the fresh session ID is used there only as a correlation value. Reconnect
prologue and context invitation fields remain zero. A future network bootstrap
may obtain the session ID from the bounded first envelope; it must treat that
untrusted value only as a proposed session ID until `SessionCodec` validates
version, session, sequence, channel, and invite ID.

## Noise prologue and identity context

All integers below are unsigned big-endian values of the stated width. All
fixed-width byte strings are included exactly as shown. No protobuf bytes are
signed.

The Noise prologue is:

```text
ASCII("org.kryptopass.noise-prologue.v1") || 0x00
|| protocol_version_u32be (= 1)
|| mode_u8 (1 = pairing, 2 = reconnect)
|| session_id_16
|| invite_id_16
|| raw_invitation_token_32
|| expiry_unix_ms_u64be
```

For reconnect, `invite_id`, token, and expiry are all-zero values. The raw
pairing token is local input to the prologue and is not a Noise PSK. The
prologue binds the intended session context into the handshake hash; it does
not replace Noise authentication or make a weak key stronger.

After all three Noise XX messages, each endpoint obtains the final 32-byte
handshake hash `H`, the authenticated remote static X25519 key, and the remote
Ed25519 public key carried in the encrypted Noise handshake payload. The
responder carries its Ed25519 key in message 2; the initiator carries its key
in message 3. Version 1 has no `static_public_key` field in `PairingIdentity`
(the schema reserves that former field name): the static key is authenticated
exclusively by Noise.

The canonical shared context `C` is:

```text
ASCII("org.kryptopass.pairing.context.v1") || 0x00
|| protocol_version_u32be (= 1)
|| mode_u8
|| session_id_16
|| invite_id_16
|| invitation_token_hash_32
|| expiry_unix_ms_u64be
|| final_noise_hash_H_32
|| static_x25519_I_32
|| static_x25519_R_32
|| ed25519_I_32
|| ed25519_R_32
```

The suffixes `I` and `R` always mean initiator and responder, independent of
the local endpoint. Reconnect uses all-zero invitation ID, token hash, and
expiry. The token hash in `C` is the identity crate's domain-separated hash
defined above. `H` commits to the raw-token prologue without putting that raw
token into the signed identity statement.

Each endpoint sends a `PairingIdentity` over Noise transport encryption. Its
device ID is
`SHA-256(b"org.kryptopass.device-id.v1\0" || ed25519_public_key)`. Its proof is
Ed25519 over:

```text
ASCII("org.kryptopass.pairing.identity-proof.v1") || 0x00
|| signer_role_u8 (0 = initiator, 1 = responder)
|| C
```

The receiver uses strict Ed25519 verification, confirms the proof's role is
the remote endpoint, checks the device ID against the signing key, checks
`PairingIdentity.transcript_hash == H`, and checks the remote static key
against the authenticated Noise static key. The local identity is not
accepted as its own peer. Any mismatch ends the attempt before trust can be
written.

## Emoji SAS

Pairing derives six output bytes using HKDF-SHA256:

```text
salt = H
IKM  = C
info = b"org.kryptopass.pairing.sas-emoji.v1\0"
L    = 6 bytes
```

The engine reads the first 42 output bits, most-significant bit first, as seven
consecutive 6-bit unsigned indices. Each index maps directly to the exact
64-entry English emoji catalog in Matrix specification v1.16's
[SAS emoji JSON](https://raw.githubusercontent.com/matrix-org/matrix-spec/v1.16/data-definitions/sas-emoji.json).
The catalog order is wire-visible: for example, index 0 is Dog, 1 is Cat, 2
is Lion, 3 is Horse, and 4 is Unicorn. KryptoPass uses this catalog and bit
mapping; it does not claim Matrix SAS protocol interoperability.

Both endpoints display the same seven emojis and names. The application must
obtain an explicit local decision through `LocalApproval`; there is no
approve-all library default. A remote accepted confirmation never changes the
local approval decision. While waiting for the local user, the engine continues
to receive the encrypted remote confirmation or abort, and the session timeout
continues to run. A remote rejection, local rejection, cancellation, mismatch,
timeout, or transport failure is terminal for that attempt.

## State machine and trust boundary

The new-pairing flow is:

```text
Idle → InvitationCreated → Connecting → NoiseHandshake → IdentityBinding
  → AwaitingSasConfirmation → ConfirmationExchange → PersistingTrust → Trusted
```

Reconnect starts at `Connecting`, skips invitation and SAS states, and requires
an exact expected trusted peer identity. Rejection, cancellation, expiry, and
other failures enter their matching terminal state. `kp-identity`'s trust
database must be bound to the local device ID before the engine can run.

`PairingEngine::state()` can report `Idle` before `run` starts. `run` consumes
the engine, so intermediate progress and terminal failure states are not
exposed as a live trace; successful `PairingOutcome` reports `Trusted`, and
failures are returned as typed `PairingError` values. The other public
`PairingState` variants name the internal transitions and terminal outcomes,
not a progress-observation API.

For a new pairing, both devices must verify both identity proofs and display
the same SAS; each user must explicitly approve locally; and each endpoint
must receive an authenticated `PairingConfirmation(accepted=true)` from the
other. Only then do both endpoints exchange `PairingComplete(H)`. In this
engine, `PairingComplete` means “all authentication and approval checks passed;
I am ready to commit locally.” It is a readiness message sent before local
SQLite persistence. It is not a receipt for the remote database and does not
grant trust on receipt.

Each endpoint then performs one local SQLite authorization transaction as its
final fallible action and immediately returns `Trusted`. The engine checks its
monotonic deadline immediately before starting that synchronous transaction.
It also advances the supplied Unix timestamp by monotonic elapsed time before
recording the authorization time.
The transaction is not preempted once started; a blocking database commit may
finish after the deadline and the engine reports its durable result. It never
returns a timeout while a detached database operation might still commit. No
network operation after the commit can turn the local result into a reported
failure. The two
endpoints' SQLite databases cannot commit atomically: if one local transaction
fails after sending readiness, the other endpoint may already commit its
independent valid trust decision. The engine does not claim distributed
atomicity. Any failure before a local commit leaves that endpoint's trust
records unchanged. On the hosted side, the invitation ledger entry may already
be durably consumed, as required by the single-use attempt policy.

Reconnect runs a fresh Noise XX handshake, session ID, ephemeral DH keys,
transcript hash, and identity proofs. Before handshake it requires the expected
peer to exist in the local trust database. It compares both Ed25519 and static
X25519 keys with that exact record, exchanges `PairingComplete(H)`, then checks
the local record again immediately before returning `Trusted`. It never calls
`authorize_peer`, creates a missing record, repairs a key mismatch, or falls
back to a new pairing without SAS. A revoked or absent record fails closed.

`PairingAbort` is accepted only inside the authenticated Noise transport.
Synchronization and key-transfer messages remain reserved schema entries and
have no handlers in the pairing engine. The engine does not accept application
data before a completed trust decision.

Remote `UserCancelled`, `Expired`, and `Rejected` abort codes map to the
corresponding `Cancelled`, `Expired`, and `Rejected` outcomes. `Failed`,
unspecified, and unknown abort codes fail closed as `Failed`.

Abort and rejection notifications are best-effort. Dropping or closing a
transport may discard a queued final control frame, so the peer can observe a
closed transport instead of the specific abort reason. The security guarantee
is that both sides fail without granting trust; delivery of a particular
terminal reason is not guaranteed.

On any failed or expired attempt, `run` drops its transport. The in-memory
adapter closes the entire link on drop. Future `PeerTransport` adapters must
also release or close their connection when dropped; the engine does not await
an unbounded asynchronous close after its deadline, which could otherwise
extend a timed-out pairing.

## Kotlin and other implementations

Other implementations must follow the exact byte concatenations, integer
endianness, mode and role values, invitation token hash, SAS catalog ordering,
and MSB-first 42-bit extraction above. They must validate the protobuf envelope
and size limits in [protocolo-peer-v1.md](protocolo-peer-v1.md), use Noise XX with the
specified suite and prologue, carry the Ed25519 keys in Noise messages 2 and 3,
and authenticate the signed context only after the final Noise hash exists.
They must not sign protobuf encodings or treat `PairingComplete` as proof that
the remote trust transaction committed.

The implementation uses Snow's documented `Builder`, `HandshakeState`, and
transport-mode APIs for the Noise state progression. See the primary
[Snow Builder documentation](https://docs.rs/snow/latest/snow/struct.Builder.html)
and [HandshakeState documentation](https://docs.rs/snow/latest/snow/struct.HandshakeState.html).

The deterministic byte contract is checked against
[`pairing_v1.json`](../../../crates/kp-pairing/tests/fixtures/pairing_v1.json), which
contains synthetic identities, invitation material, transcript inputs,
signatures, SAS indices, and a fixed-key Noise XX exchange. Its private and
ephemeral keys are test fixtures only and must never be reused by an
application. The complete display catalog is also checked against the
[Matrix v1.16 fixture](../../../crates/kp-pairing/tests/fixtures/matrix_sas_emoji_v1_16.json)
and its [official source](https://raw.githubusercontent.com/matrix-org/matrix-spec/v1.16/data-definitions/sas-emoji.json).
Production pairing does not expose a deterministic ephemeral-key hook.

## Memory-hygiene limitation

Invitation tokens, prologue buffers, Noise scratch buffers, and derived SAS
material are held in zeroizing wrappers where the implementation controls the
buffer. Snow 0.10's default X25519 and AES-GCM state also contains private and
cipher key arrays without explicit zeroizing drop behavior. Dropping a pairing
engine therefore does not guarantee that every cryptographic key copy has
been erased from process memory. Before production use, evaluate a Snow
backend or upstream change with explicit key zeroization and verify its
behavior on supported targets.
