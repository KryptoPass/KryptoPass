//! Versioned Protocol Buffers contract for KryptoPass peer communication.
//!
//! Generated wire types intentionally do not implement Debug. Payloads may
//! contain pairing tokens or encrypted vault objects, so this crate provides
//! redacted wrappers for diagnostics.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

use prost::Message;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Current wire protocol version.
pub const PROTOCOL_VERSION: u32 = 1;
/// Maximum encoded Protocol Buffers envelope size.
pub const MAX_FRAME_SIZE: usize = 64 * 1024;
/// Maximum standalone bootstrap invitation size.
pub const MAX_PAIRING_INVITE_SIZE: usize = 512;
/// Maximum encoded payload, leaving room for its envelope.
pub const MAX_PAYLOAD_SIZE: usize = MAX_FRAME_SIZE - 64;
/// Maximum opaque bytes in one data chunk.
pub const MAX_FRAGMENT_SIZE: usize = 48 * 1024;
/// Maximum complete transfer size.
pub const MAX_TRANSFER_SIZE: usize = 8 * 1024 * 1024;
/// Maximum fragments in one transfer.
pub const MAX_FRAGMENT_COUNT: usize = 256;
/// Maximum simultaneous transfer assemblies.
pub const MAX_ACTIVE_TRANSFERS: usize = 2;
/// Maximum declared bytes reserved by simultaneous assemblies.
pub const MAX_BUFFERED_TRANSFER_BYTES: usize = 8 * 1024 * 1024;

/// Types generated from the shared Protocol Buffers schema.
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/org.kryptopass.protocol.v1.rs"));
}

/// Logical channel associated with a message type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Pairing and synchronization metadata.
    Control,
    /// Opaque transfer fragments.
    Data,
}

/// The peer's role in Noise XX. This describes message order only; it does not
/// run the Noise handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandshakeRole {
    /// Sends Noise message 1, receives 2, then sends 3.
    Initiator,
    /// Receives Noise message 1, sends 2, then receives 3.
    Responder,
}

/// Protocol validation failures. Error text deliberately excludes payload data.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// The encoded envelope is larger than the protocol limit.
    #[error("frame exceeds the maximum size")]
    FrameTooLarge,
    /// The protobuf envelope is malformed.
    #[error("malformed protobuf envelope")]
    MalformedEnvelope,
    /// The selected typed payload is malformed.
    #[error("malformed protobuf payload for message type {message_type}")]
    MalformedPayload { message_type: i32 },
    /// The peer uses an unsupported protocol version.
    #[error("unsupported protocol version {actual}")]
    UnsupportedVersion { actual: u32 },
    /// A session identifier is zero or has the wrong size.
    #[error("invalid session identifier")]
    InvalidSessionId,
    /// The frame belongs to another session.
    #[error("session identifier does not match")]
    SessionMismatch,
    /// Message IDs start at one.
    #[error("message identifier must be non-zero")]
    InvalidMessageId,
    /// The frame repeats an already consumed ID.
    #[error("replayed message identifier")]
    Replay,
    /// The frame skipped the next expected ID.
    #[error("message identifier is out of order")]
    OutOfOrder,
    /// The sequence reached u64::MAX.
    #[error("message sequence is exhausted")]
    SequenceExhausted,
    /// The envelope uses an unknown or reserved message type.
    #[error("unknown or unspecified message type {value}")]
    UnknownMessageType { value: i32 },
    /// The payload exceeds the envelope limit.
    #[error("payload exceeds the maximum size")]
    PayloadTooLarge,
    /// A required field is missing, duplicated, or outside its bound.
    #[error("invalid field: {field}")]
    InvalidField { field: &'static str },
    /// The transport lane disagrees with the message type.
    #[error("message was carried on the wrong channel")]
    WrongChannel { expected: Channel, actual: Channel },
    /// A Noise XX message arrived from the wrong role or at the wrong step.
    #[error("unexpected Noise handshake step {actual}; expected {expected}")]
    UnexpectedNoiseStep { expected: u32, actual: u32 },
    /// A QR/bootstrap invitation is not permitted inside a peer session.
    #[error("pairing invitations are out-of-band bootstrap objects")]
    OutOfBandMessage,
    /// Fragment metadata is invalid or inconsistent.
    #[error("invalid fragment metadata")]
    InvalidFragment,
    /// A field or transfer exceeds its configured cap.
    #[error("fragment limit exceeded")]
    FragmentLimitExceeded,
    /// A fragment index was reused with different bytes.
    #[error("conflicting duplicate fragment")]
    ConflictingFragment,
    /// Reassembled bytes do not match the advertised digest.
    #[error("fragment digest mismatch")]
    FragmentDigestMismatch,
    /// Concurrent assembly or reserved byte limits were reached.
    #[error("too many active fragment assemblies")]
    TooManyTransfers,
    /// Fragment assembly limits are invalid or exceed hard ceilings.
    #[error("invalid fragment limits")]
    InvalidFragmentLimits,
}

/// Type-safe payload returned after an envelope and payload have been checked.
pub enum MessagePayload {
    /// Single-use pairing invitation.
    PairingInvite(proto::PairingInvite),
    /// Pairing hello.
    PairingHello(proto::PairingHello),
    /// One Noise XX handshake message.
    NoiseHandshake(proto::NoiseHandshake),
    /// Signing identity and transcript proof.
    PairingIdentity(proto::PairingIdentity),
    /// SAS confirmation event.
    PairingConfirmation(proto::PairingConfirmation),
    /// Pairing completion event.
    PairingComplete(proto::PairingComplete),
    /// Pairing abort event.
    PairingAbort(proto::PairingAbort),
    /// Synchronization greeting.
    SyncHello(proto::SyncHello),
    /// Bounded revision manifest.
    SyncManifest(proto::SyncManifest),
    /// Bounded revision request.
    SyncRequest(proto::SyncRequest),
    /// Opaque synchronization fragment.
    SyncChunk(proto::SyncChunk),
    /// Synchronization transfer commit.
    SyncCommit(proto::SyncCommit),
    /// Synchronization acknowledgement.
    SyncAck(proto::SyncAck),
    /// Conflict metadata.
    SyncConflict(proto::SyncConflict),
    /// Reserved key transfer offer; this crate performs no key operation.
    KeyTransferOffer(proto::KeyTransferOffer),
    /// Reserved key transfer fragment; this crate performs no key operation.
    KeyTransferChunk(proto::KeyTransferChunk),
    /// Reserved key transfer completion; this crate performs no key operation.
    KeyTransferComplete(proto::KeyTransferComplete),
    /// Reserved key transfer abort; this crate performs no key operation.
    KeyTransferAbort(proto::KeyTransferAbort),
    /// Authenticated active-session identity exchange.
    SessionHello(proto::SessionHello),
    /// Active-session keepalive request.
    SessionPing(proto::SessionPing),
    /// Active-session keepalive response.
    SessionPong(proto::SessionPong),
    /// Authenticated active-session closure.
    SessionClose(proto::SessionClose),
}

impl fmt::Debug for MessagePayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::PairingInvite(_) => "PairingInvite",
            Self::PairingHello(_) => "PairingHello",
            Self::NoiseHandshake(_) => "NoiseHandshake",
            Self::PairingIdentity(_) => "PairingIdentity",
            Self::PairingConfirmation(_) => "PairingConfirmation",
            Self::PairingComplete(_) => "PairingComplete",
            Self::PairingAbort(_) => "PairingAbort",
            Self::SyncHello(_) => "SyncHello",
            Self::SyncManifest(_) => "SyncManifest",
            Self::SyncRequest(_) => "SyncRequest",
            Self::SyncChunk(_) => "SyncChunk",
            Self::SyncCommit(_) => "SyncCommit",
            Self::SyncAck(_) => "SyncAck",
            Self::SyncConflict(_) => "SyncConflict",
            Self::KeyTransferOffer(_) => "KeyTransferOffer",
            Self::KeyTransferChunk(_) => "KeyTransferChunk",
            Self::KeyTransferComplete(_) => "KeyTransferComplete",
            Self::KeyTransferAbort(_) => "KeyTransferAbort",
            Self::SessionHello(_) => "SessionHello",
            Self::SessionPing(_) => "SessionPing",
            Self::SessionPong(_) => "SessionPong",
            Self::SessionClose(_) => "SessionClose",
        };
        f.write_str(name)
    }
}

/// A fully validated message. The payload is redacted from Debug output.
pub struct DecodedMessage {
    /// Sender-direction sequence number, starting at one.
    pub message_id: u64,
    /// Validated protobuf message type.
    pub message_type: proto::MessageType,
    /// Logical channel implied by the message type.
    pub channel: Channel,
    /// Typed payload.
    pub payload: MessagePayload,
}

impl fmt::Debug for DecodedMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodedMessage")
            .field("message_id", &self.message_id)
            .field("message_type", &message_type_name(self.message_type))
            .field("channel", &self.channel)
            .field("payload", &"[redacted]")
            .finish()
    }
}

/// Session-scoped codec with independent, constant-memory send and receive
/// counters. A decode failure never consumes an ID.
pub struct SessionCodec {
    session_id: [u8; 16],
    next_send_id: Option<u64>,
    next_receive_id: Option<u64>,
}

impl fmt::Debug for SessionCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionCodec")
            .field("session_id", &"[redacted]")
            .field("next_send_id", &self.next_send_id)
            .field("next_receive_id", &self.next_receive_id)
            .finish()
    }
}

impl SessionCodec {
    /// Creates a codec for one 16-byte session ID.
    #[must_use]
    pub const fn new(session_id: [u8; 16]) -> Self {
        Self {
            session_id,
            next_send_id: Some(1),
            next_receive_id: Some(1),
        }
    }

    /// Returns the configured session ID.
    #[must_use]
    pub const fn session_id(&self) -> &[u8; 16] {
        &self.session_id
    }

    /// Encodes and validates a typed payload. The outgoing ID advances only
    /// after the full envelope has been created successfully.
    pub fn encode<M: Message>(
        &mut self,
        message_type: proto::MessageType,
        payload: &M,
    ) -> Result<Vec<u8>, ProtocolError> {
        self.validate_session_id()?;
        let message_id = self.next_send_id.ok_or(ProtocolError::SequenceExhausted)?;
        let type_value = message_type as i32;
        if type_value == 0 {
            return Err(ProtocolError::UnknownMessageType { value: type_value });
        }
        if message_type == proto::MessageType::PairingInvite {
            return Err(ProtocolError::OutOfBandMessage);
        }
        if payload.encoded_len() > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge);
        }
        let payload = payload.encode_to_vec();
        preflight_payload(type_value, &payload)?;
        let _ = decode_payload(type_value, &payload)?;
        let envelope = proto::Envelope {
            protocol_version: PROTOCOL_VERSION,
            session_id: self.session_id.to_vec(),
            message_id,
            message_type: type_value,
            payload,
        };
        if envelope.encoded_len() > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge);
        }
        let frame = envelope.encode_to_vec();
        self.next_send_id = message_id.checked_add(1);
        Ok(frame)
    }

    /// Decodes one frame and validates version, session, payload, and exact
    /// sender-direction ordering.
    pub fn decode(&mut self, frame: &[u8]) -> Result<DecodedMessage, ProtocolError> {
        self.decode_inner(frame, None)
    }

    /// Decodes a frame received on a transport lane and checks that the lane
    /// matches the message type before consuming its ID.
    pub fn decode_on(
        &mut self,
        channel: Channel,
        frame: &[u8],
    ) -> Result<DecodedMessage, ProtocolError> {
        self.decode_inner(frame, Some(channel))
    }

    fn decode_inner(
        &mut self,
        frame: &[u8],
        received_channel: Option<Channel>,
    ) -> Result<DecodedMessage, ProtocolError> {
        self.validate_session_id()?;
        if frame.len() > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge);
        }
        let envelope =
            proto::Envelope::decode(frame).map_err(|_| ProtocolError::MalformedEnvelope)?;
        if envelope.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion {
                actual: envelope.protocol_version,
            });
        }
        if envelope.session_id.len() != 16 {
            return Err(ProtocolError::InvalidSessionId);
        }
        if envelope.session_id.as_slice() != self.session_id {
            return Err(ProtocolError::SessionMismatch);
        }
        if envelope.message_id == 0 {
            return Err(ProtocolError::InvalidMessageId);
        }
        let expected = self
            .next_receive_id
            .ok_or(ProtocolError::SequenceExhausted)?;
        if envelope.message_id < expected {
            return Err(ProtocolError::Replay);
        }
        if envelope.message_id > expected {
            return Err(ProtocolError::OutOfOrder);
        }
        if envelope.payload.len() > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge);
        }
        let channel = channel_for_type(envelope.message_type)?;
        if envelope.message_type == proto::MessageType::PairingInvite as i32 {
            return Err(ProtocolError::OutOfBandMessage);
        }
        if let Some(actual) = received_channel {
            if actual != channel {
                return Err(ProtocolError::WrongChannel {
                    expected: channel,
                    actual,
                });
            }
        }
        preflight_payload(envelope.message_type, &envelope.payload)?;
        let payload = decode_payload(envelope.message_type, &envelope.payload)?;
        let message_type = proto::MessageType::try_from(envelope.message_type).map_err(|_| {
            ProtocolError::UnknownMessageType {
                value: envelope.message_type,
            }
        })?;
        self.next_receive_id = envelope.message_id.checked_add(1);
        Ok(DecodedMessage {
            message_id: envelope.message_id,
            message_type,
            channel,
            payload,
        })
    }

    fn validate_session_id(&self) -> Result<(), ProtocolError> {
        if self.session_id.iter().all(|byte| *byte == 0) {
            Err(ProtocolError::InvalidSessionId)
        } else {
            Ok(())
        }
    }
}

/// Small role-aware order guard for the three Noise XX messages. The caller
/// records an outgoing step only after its transport accepts the frame.
#[derive(Clone, Debug)]
pub struct NoiseHandshakeOrder {
    role: HandshakeRole,
    next_step: u8,
}

impl NoiseHandshakeOrder {
    /// Starts the role-specific Noise XX message sequence.
    #[must_use]
    pub const fn new(role: HandshakeRole) -> Self {
        Self { role, next_step: 1 }
    }

    /// Returns the only valid next outgoing step, or None when it is the
    /// peer's turn or the handshake is complete.
    #[must_use]
    pub const fn next_outgoing(&self) -> Option<u32> {
        match (self.role, self.next_step) {
            (HandshakeRole::Initiator, 1) => Some(1),
            (HandshakeRole::Initiator, 3) => Some(3),
            (HandshakeRole::Responder, 2) => Some(2),
            _ => None,
        }
    }

    /// Returns the only valid next incoming step, or None when it is this
    /// role's turn to send or the handshake is complete.
    #[must_use]
    pub const fn next_incoming(&self) -> Option<u32> {
        match (self.role, self.next_step) {
            (HandshakeRole::Initiator, 2) => Some(2),
            (HandshakeRole::Responder, 1) => Some(1),
            (HandshakeRole::Responder, 3) => Some(3),
            _ => None,
        }
    }

    /// Records a step after a successful outgoing transport send.
    pub fn record_outgoing(&mut self, actual: u32) -> Result<(), ProtocolError> {
        let Some(expected) = self.next_outgoing() else {
            return Err(ProtocolError::UnexpectedNoiseStep {
                expected: 0,
                actual,
            });
        };
        if actual != expected {
            return Err(ProtocolError::UnexpectedNoiseStep { expected, actual });
        }
        self.next_step += 1;
        Ok(())
    }

    /// Records a step after its frame and payload were validated.
    pub fn record_incoming(&mut self, actual: u32) -> Result<(), ProtocolError> {
        let Some(expected) = self.next_incoming() else {
            return Err(ProtocolError::UnexpectedNoiseStep {
                expected: 0,
                actual,
            });
        };
        if actual != expected {
            return Err(ProtocolError::UnexpectedNoiseStep { expected, actual });
        }
        self.next_step += 1;
        Ok(())
    }

    /// Whether all three Noise XX turns have completed in order.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.next_step > 3
    }
}

/// Maps a protobuf message type to its logical transport lane.
pub fn channel_for_type(type_value: i32) -> Result<Channel, ProtocolError> {
    use proto::MessageType as Type;
    match Type::try_from(type_value)
        .map_err(|_| ProtocolError::UnknownMessageType { value: type_value })?
    {
        Type::SyncChunk | Type::KeyTransferChunk => Ok(Channel::Data),
        Type::Unspecified => Err(ProtocolError::UnknownMessageType { value: type_value }),
        _ => Ok(Channel::Control),
    }
}

/// Encodes the bootstrap invitation as a standalone Protocol Buffers object
/// for an out-of-band channel such as a QR code. The token is not permitted in
/// an active session frame.
pub fn encode_pairing_invite(invite: &proto::PairingInvite) -> Result<Vec<u8>, ProtocolError> {
    validate_pairing_invite(invite)?;
    if invite.encoded_len() > MAX_PAIRING_INVITE_SIZE {
        return Err(ProtocolError::PayloadTooLarge);
    }
    Ok(invite.encode_to_vec())
}

/// Decodes and validates a standalone bootstrap invitation before returning
/// its single-use token to the caller.
pub fn decode_pairing_invite(bytes: &[u8]) -> Result<proto::PairingInvite, ProtocolError> {
    if bytes.len() > MAX_PAIRING_INVITE_SIZE {
        return Err(ProtocolError::PayloadTooLarge);
    }
    let invite =
        proto::PairingInvite::decode(bytes).map_err(|_| ProtocolError::MalformedPayload {
            message_type: proto::MessageType::PairingInvite as i32,
        })?;
    validate_pairing_invite(&invite)?;
    Ok(invite)
}

fn message_type_name(message_type: proto::MessageType) -> &'static str {
    use proto::MessageType as Type;
    match message_type {
        Type::Unspecified => "Unspecified",
        Type::PairingInvite => "PairingInvite",
        Type::PairingHello => "PairingHello",
        Type::NoiseHandshake => "NoiseHandshake",
        Type::PairingIdentity => "PairingIdentity",
        Type::PairingConfirmation => "PairingConfirmation",
        Type::PairingComplete => "PairingComplete",
        Type::PairingAbort => "PairingAbort",
        Type::SyncHello => "SyncHello",
        Type::SyncManifest => "SyncManifest",
        Type::SyncRequest => "SyncRequest",
        Type::SyncChunk => "SyncChunk",
        Type::SyncCommit => "SyncCommit",
        Type::SyncAck => "SyncAck",
        Type::SyncConflict => "SyncConflict",
        Type::KeyTransferOffer => "KeyTransferOffer",
        Type::KeyTransferChunk => "KeyTransferChunk",
        Type::KeyTransferComplete => "KeyTransferComplete",
        Type::KeyTransferAbort => "KeyTransferAbort",
        Type::SessionHello => "SessionHello",
        Type::SessionPing => "SessionPing",
        Type::SessionPong => "SessionPong",
        Type::SessionClose => "SessionClose",
    }
}

fn decode_payload(type_value: i32, bytes: &[u8]) -> Result<MessagePayload, ProtocolError> {
    use proto::MessageType as Type;
    let message_type = Type::try_from(type_value)
        .map_err(|_| ProtocolError::UnknownMessageType { value: type_value })?;
    macro_rules! decode {
        ($type:ty, $variant:ident, $validate:ident) => {{
            let value = <$type>::decode(bytes).map_err(|_| ProtocolError::MalformedPayload {
                message_type: type_value,
            })?;
            $validate(&value)?;
            MessagePayload::$variant(value)
        }};
    }
    Ok(match message_type {
        Type::Unspecified => {
            return Err(ProtocolError::UnknownMessageType { value: type_value });
        }
        Type::PairingInvite => {
            decode!(proto::PairingInvite, PairingInvite, validate_pairing_invite)
        }
        Type::PairingHello => decode!(proto::PairingHello, PairingHello, validate_pairing_hello),
        Type::NoiseHandshake => decode!(proto::NoiseHandshake, NoiseHandshake, validate_noise),
        Type::PairingIdentity => {
            decode!(
                proto::PairingIdentity,
                PairingIdentity,
                validate_pairing_identity
            )
        }
        Type::PairingConfirmation => decode!(
            proto::PairingConfirmation,
            PairingConfirmation,
            validate_pairing_confirmation
        ),
        Type::PairingComplete => {
            decode!(
                proto::PairingComplete,
                PairingComplete,
                validate_pairing_complete
            )
        }
        Type::PairingAbort => decode!(proto::PairingAbort, PairingAbort, validate_pairing_abort),
        Type::SyncHello => decode!(proto::SyncHello, SyncHello, validate_sync_hello),
        Type::SyncManifest => decode!(proto::SyncManifest, SyncManifest, validate_sync_manifest),
        Type::SyncRequest => decode!(proto::SyncRequest, SyncRequest, validate_sync_request),
        Type::SyncChunk => decode!(proto::SyncChunk, SyncChunk, validate_sync_chunk),
        Type::SyncCommit => decode!(proto::SyncCommit, SyncCommit, validate_sync_commit),
        Type::SyncAck => decode!(proto::SyncAck, SyncAck, validate_sync_ack),
        Type::SyncConflict => decode!(proto::SyncConflict, SyncConflict, validate_sync_conflict),
        Type::KeyTransferOffer => {
            decode!(
                proto::KeyTransferOffer,
                KeyTransferOffer,
                validate_key_transfer_offer
            )
        }
        Type::KeyTransferChunk => {
            decode!(
                proto::KeyTransferChunk,
                KeyTransferChunk,
                validate_key_transfer_chunk
            )
        }
        Type::KeyTransferComplete => decode!(
            proto::KeyTransferComplete,
            KeyTransferComplete,
            validate_key_transfer_complete
        ),
        Type::KeyTransferAbort => {
            decode!(
                proto::KeyTransferAbort,
                KeyTransferAbort,
                validate_key_transfer_abort
            )
        }
        Type::SessionHello => decode!(proto::SessionHello, SessionHello, validate_session_hello),
        Type::SessionPing => decode!(proto::SessionPing, SessionPing, validate_session_ping),
        Type::SessionPong => decode!(proto::SessionPong, SessionPong, validate_session_pong),
        Type::SessionClose => decode!(proto::SessionClose, SessionClose, validate_session_close),
    })
}

fn preflight_payload(type_value: i32, payload: &[u8]) -> Result<(), ProtocolError> {
    let repeated = match proto::MessageType::try_from(type_value) {
        Ok(proto::MessageType::SyncManifest) => Some((2_u64, 1024_usize)),
        Ok(proto::MessageType::SyncRequest) => Some((2, 256)),
        Ok(proto::MessageType::SyncConflict) => Some((2, 16)),
        Ok(_) => None,
        Err(_) => return Err(ProtocolError::UnknownMessageType { value: type_value }),
    };
    let is_chunk = matches!(
        proto::MessageType::try_from(type_value),
        Ok(proto::MessageType::SyncChunk | proto::MessageType::KeyTransferChunk)
    );
    let is_conflict = matches!(
        proto::MessageType::try_from(type_value),
        Ok(proto::MessageType::SyncConflict)
    );
    let mut offset = 0usize;
    let mut repeated_count = 0usize;
    while offset < payload.len() {
        let key = read_varint(payload, &mut offset, type_value)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        if field == 0 || field > 0x1fff_ffff {
            return Err(ProtocolError::MalformedPayload {
                message_type: type_value,
            });
        }
        match wire {
            0 => {
                let _ = read_varint(payload, &mut offset, type_value)?;
            }
            1 => skip(payload, &mut offset, 8, type_value)?,
            2 => {
                let length = usize::try_from(read_varint(payload, &mut offset, type_value)?)
                    .map_err(|_| ProtocolError::MalformedPayload {
                        message_type: type_value,
                    })?;
                let field_bytes = take(payload, &mut offset, length, type_value)?;
                if let Some((field_number, maximum)) = repeated {
                    if field == field_number {
                        repeated_count = repeated_count.saturating_add(1);
                        if repeated_count > maximum {
                            return Err(ProtocolError::FragmentLimitExceeded);
                        }
                    }
                }
                if is_chunk && field == 6 && field_bytes.len() > MAX_FRAGMENT_SIZE {
                    return Err(ProtocolError::FragmentLimitExceeded);
                }
                if is_conflict && field == 2 && field_bytes.len() != 16 {
                    return Err(ProtocolError::InvalidField {
                        field: "revision_ids",
                    });
                }
            }
            5 => skip(payload, &mut offset, 4, type_value)?,
            _ => {
                return Err(ProtocolError::MalformedPayload {
                    message_type: type_value,
                });
            }
        }
    }
    Ok(())
}

fn read_varint(bytes: &[u8], offset: &mut usize, message_type: i32) -> Result<u64, ProtocolError> {
    let mut value = 0_u64;
    for shift in (0..70).step_by(7) {
        let byte = *bytes
            .get(*offset)
            .ok_or(ProtocolError::MalformedPayload { message_type })?;
        *offset = (*offset)
            .checked_add(1)
            .ok_or(ProtocolError::MalformedPayload { message_type })?;
        if shift == 63 && byte > 1 {
            return Err(ProtocolError::MalformedPayload { message_type });
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(ProtocolError::MalformedPayload { message_type })
}

fn take<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    length: usize,
    message_type: i32,
) -> Result<&'a [u8], ProtocolError> {
    let end = (*offset)
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .ok_or(ProtocolError::MalformedPayload { message_type })?;
    let result = &bytes[*offset..end];
    *offset = end;
    Ok(result)
}

fn skip(
    bytes: &[u8],
    offset: &mut usize,
    length: usize,
    message_type: i32,
) -> Result<(), ProtocolError> {
    let _ = take(bytes, offset, length, message_type)?;
    Ok(())
}

fn require_id(value: &[u8], length: usize, field: &'static str) -> Result<(), ProtocolError> {
    if value.len() != length || value.iter().all(|byte| *byte == 0) {
        return Err(ProtocolError::InvalidField { field });
    }
    Ok(())
}

fn require_len(value: &[u8], length: usize, field: &'static str) -> Result<(), ProtocolError> {
    if value.len() == length {
        Ok(())
    } else {
        Err(ProtocolError::InvalidField { field })
    }
}

fn require_fingerprint(value: &[u8]) -> Result<(), ProtocolError> {
    if value.len() == 20 || value.len() == 32 {
        Ok(())
    } else {
        Err(ProtocolError::InvalidField {
            field: "openpgp_fingerprint",
        })
    }
}

fn validate_pairing_invite(value: &proto::PairingInvite) -> Result<(), ProtocolError> {
    require_id(&value.invite_id, 16, "invite_id")?;
    require_id(&value.single_use_token, 32, "single_use_token")?;
    if value.expires_at_unix_ms == 0 {
        return Err(ProtocolError::InvalidField {
            field: "expires_at_unix_ms",
        });
    }
    if value.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion {
            actual: value.protocol_version,
        });
    }
    Ok(())
}

fn validate_pairing_hello(value: &proto::PairingHello) -> Result<(), ProtocolError> {
    require_id(&value.invite_id, 16, "invite_id")
}

fn validate_noise(value: &proto::NoiseHandshake) -> Result<(), ProtocolError> {
    if !(1..=3).contains(&value.step) {
        return Err(ProtocolError::InvalidField { field: "step" });
    }
    if value.handshake_message.is_empty() || value.handshake_message.len() > MAX_PAYLOAD_SIZE {
        return Err(ProtocolError::InvalidField {
            field: "handshake_message",
        });
    }
    Ok(())
}

fn validate_pairing_identity(value: &proto::PairingIdentity) -> Result<(), ProtocolError> {
    require_id(&value.device_id, 32, "device_id")?;
    require_len(&value.signing_public_key, 32, "signing_public_key")?;
    require_len(&value.transcript_hash, 32, "transcript_hash")?;
    require_len(&value.proof, 64, "proof")
}

fn validate_pairing_confirmation(value: &proto::PairingConfirmation) -> Result<(), ProtocolError> {
    require_len(&value.transcript_hash, 32, "transcript_hash")
}

fn validate_pairing_complete(value: &proto::PairingComplete) -> Result<(), ProtocolError> {
    require_len(&value.transcript_hash, 32, "transcript_hash")
}

fn validate_pairing_abort(value: &proto::PairingAbort) -> Result<(), ProtocolError> {
    let code = proto::AbortCode::try_from(value.code)
        .map_err(|_| ProtocolError::InvalidField { field: "code" })?;
    if code == proto::AbortCode::Unspecified {
        return Err(ProtocolError::InvalidField { field: "code" });
    }
    Ok(())
}

fn validate_session_hello(value: &proto::SessionHello) -> Result<(), ProtocolError> {
    if value.session_version != 1 {
        return Err(ProtocolError::UnsupportedVersion {
            actual: value.session_version,
        });
    }
    require_id(&value.device_id, 32, "device_id")?;
    if value.reconnect_port > u32::from(u16::MAX) {
        return Err(ProtocolError::InvalidField {
            field: "reconnect_port",
        });
    }
    Ok(())
}

fn validate_session_ping(value: &proto::SessionPing) -> Result<(), ProtocolError> {
    if value.nonce == 0 {
        return Err(ProtocolError::InvalidField { field: "nonce" });
    }
    Ok(())
}

fn validate_session_pong(value: &proto::SessionPong) -> Result<(), ProtocolError> {
    if value.nonce == 0 {
        return Err(ProtocolError::InvalidField { field: "nonce" });
    }
    Ok(())
}

fn validate_session_close(value: &proto::SessionClose) -> Result<(), ProtocolError> {
    if !(1..=5).contains(&value.reason) {
        return Err(ProtocolError::InvalidField { field: "reason" });
    }
    Ok(())
}

fn validate_sync_hello(value: &proto::SyncHello) -> Result<(), ProtocolError> {
    require_id(&value.vault_id, 16, "vault_id")?;
    require_id(&value.device_id, 32, "device_id")?;
    if value.known_revision_count > 1_000_000 {
        return Err(ProtocolError::InvalidField {
            field: "known_revision_count",
        });
    }
    Ok(())
}

fn validate_sync_manifest(value: &proto::SyncManifest) -> Result<(), ProtocolError> {
    require_id(&value.snapshot_id, 16, "snapshot_id")?;
    if value.heads.len() > 1024 {
        return Err(ProtocolError::FragmentLimitExceeded);
    }
    let mut seen = HashSet::with_capacity(value.heads.len());
    for head in &value.heads {
        require_id(&head.record_id, 16, "record_id")?;
        require_id(&head.revision_id, 16, "revision_id")?;
        require_len(&head.revision_hash, 32, "revision_hash")?;
        if !seen.insert((head.record_id.as_slice(), head.revision_id.as_slice())) {
            return Err(ProtocolError::InvalidField {
                field: "duplicate_manifest_revision",
            });
        }
    }
    Ok(())
}

fn validate_sync_request(value: &proto::SyncRequest) -> Result<(), ProtocolError> {
    require_id(&value.snapshot_id, 16, "snapshot_id")?;
    if value.revisions.len() > 256 {
        return Err(ProtocolError::FragmentLimitExceeded);
    }
    let mut seen = HashSet::with_capacity(value.revisions.len());
    for revision in &value.revisions {
        require_id(&revision.record_id, 16, "record_id")?;
        require_id(&revision.revision_id, 16, "revision_id")?;
        if !seen.insert((
            revision.record_id.as_slice(),
            revision.revision_id.as_slice(),
        )) {
            return Err(ProtocolError::InvalidField {
                field: "duplicate_requested_revision",
            });
        }
    }
    Ok(())
}

fn validate_chunk_fields(
    transfer_id: &[u8],
    chunk_index: u32,
    chunk_count: u32,
    total_size: u64,
    digest: &[u8],
    data: &[u8],
) -> Result<(), ProtocolError> {
    require_id(transfer_id, 16, "transfer_id")?;
    if chunk_count == 0 || chunk_count as usize > MAX_FRAGMENT_COUNT || chunk_index >= chunk_count {
        return Err(ProtocolError::InvalidField {
            field: "chunk_index",
        });
    }
    require_len(digest, 32, "sha256_digest")?;
    if data.is_empty() || data.len() > MAX_FRAGMENT_SIZE {
        return Err(ProtocolError::InvalidField { field: "data" });
    }
    let total = usize::try_from(total_size).map_err(|_| ProtocolError::InvalidField {
        field: "total_size",
    })?;
    if total < chunk_count as usize {
        return Err(ProtocolError::InvalidField {
            field: "total_size",
        });
    }
    let maximum = (chunk_count as usize)
        .checked_mul(MAX_FRAGMENT_SIZE)
        .ok_or(ProtocolError::InvalidField {
            field: "chunk_count",
        })?;
    if total == 0 || total > MAX_TRANSFER_SIZE || total > maximum || data.len() > total {
        return Err(ProtocolError::InvalidField {
            field: "total_size",
        });
    }
    Ok(())
}

fn validate_sync_chunk(value: &proto::SyncChunk) -> Result<(), ProtocolError> {
    validate_chunk_fields(
        &value.transfer_id,
        value.chunk_index,
        value.chunk_count,
        value.total_size,
        &value.sha256_digest,
        &value.data,
    )
}

fn validate_sync_commit(value: &proto::SyncCommit) -> Result<(), ProtocolError> {
    require_id(&value.transfer_id, 16, "transfer_id")?;
    require_len(&value.sha256_digest, 32, "sha256_digest")
}

fn validate_sync_ack(value: &proto::SyncAck) -> Result<(), ProtocolError> {
    require_id(&value.transfer_id, 16, "transfer_id")?;
    let status = proto::AckStatus::try_from(value.status)
        .map_err(|_| ProtocolError::InvalidField { field: "status" })?;
    if status == proto::AckStatus::Unspecified {
        return Err(ProtocolError::InvalidField { field: "status" });
    }
    Ok(())
}

fn validate_sync_conflict(value: &proto::SyncConflict) -> Result<(), ProtocolError> {
    require_id(&value.record_id, 16, "record_id")?;
    if !(2..=16).contains(&value.revision_ids.len()) {
        return Err(ProtocolError::InvalidField {
            field: "revision_ids",
        });
    }
    let mut seen = HashSet::with_capacity(value.revision_ids.len());
    for id in &value.revision_ids {
        require_id(id, 16, "revision_ids")?;
        if !seen.insert(id.as_slice()) {
            return Err(ProtocolError::InvalidField {
                field: "duplicate_revision_id",
            });
        }
    }
    Ok(())
}

fn validate_key_transfer_offer(value: &proto::KeyTransferOffer) -> Result<(), ProtocolError> {
    require_id(&value.transfer_id, 16, "transfer_id")?;
    require_fingerprint(&value.openpgp_fingerprint)?;
    if value.total_size == 0 || value.total_size > MAX_TRANSFER_SIZE as u64 {
        return Err(ProtocolError::InvalidField {
            field: "total_size",
        });
    }
    Ok(())
}

fn validate_key_transfer_chunk(value: &proto::KeyTransferChunk) -> Result<(), ProtocolError> {
    validate_chunk_fields(
        &value.transfer_id,
        value.chunk_index,
        value.chunk_count,
        value.total_size,
        &value.sha256_digest,
        &value.data,
    )
}

fn validate_key_transfer_complete(value: &proto::KeyTransferComplete) -> Result<(), ProtocolError> {
    require_id(&value.transfer_id, 16, "transfer_id")?;
    require_fingerprint(&value.openpgp_fingerprint)
}

fn validate_key_transfer_abort(value: &proto::KeyTransferAbort) -> Result<(), ProtocolError> {
    require_id(&value.transfer_id, 16, "transfer_id")?;
    let code = proto::AbortCode::try_from(value.code)
        .map_err(|_| ProtocolError::InvalidField { field: "code" })?;
    if code == proto::AbortCode::Unspecified {
        return Err(ProtocolError::InvalidField { field: "code" });
    }
    Ok(())
}

/// Per-assembler limits for fragment reassembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FragmentLimits {
    /// Maximum number of transfer IDs held at once.
    pub max_active_transfers: usize,
    /// Maximum declared size of one transfer.
    pub max_total_size: usize,
    /// Maximum fragment count for one transfer.
    pub max_fragments: usize,
    /// Maximum bytes in one fragment.
    pub max_fragment_size: usize,
    /// Maximum sum of declared sizes held at once.
    pub max_buffered_bytes: usize,
    /// Lifetime of an incomplete assembly.
    pub transfer_timeout: Duration,
}

impl Default for FragmentLimits {
    fn default() -> Self {
        Self {
            max_active_transfers: MAX_ACTIVE_TRANSFERS,
            max_total_size: MAX_TRANSFER_SIZE,
            max_fragments: MAX_FRAGMENT_COUNT,
            max_fragment_size: MAX_FRAGMENT_SIZE,
            max_buffered_bytes: MAX_BUFFERED_TRANSFER_BYTES,
            transfer_timeout: Duration::from_secs(60),
        }
    }
}

/// Borrowed fragment metadata accepted by FragmentAssembler.
pub struct Fragment<'a> {
    /// Non-zero 16-byte transfer identifier.
    pub transfer_id: &'a [u8],
    /// Zero-based fragment index.
    pub index: u32,
    /// Number of fragments in this transfer.
    pub count: u32,
    /// Declared byte count of the complete transfer.
    pub total_size: usize,
    /// SHA-256 digest of the complete transfer.
    pub digest: &'a [u8],
    /// Data bytes for this fragment.
    pub data: &'a [u8],
}

impl fmt::Debug for Fragment<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fragment")
            .field("index", &self.index)
            .field("count", &self.count)
            .field("total_size", &self.total_size)
            .field("data", &"[redacted]")
            .finish()
    }
}

struct Assembly {
    digest: [u8; 32],
    total_size: usize,
    slots: Vec<Option<Vec<u8>>>,
    received_bytes: usize,
    received_count: usize,
    started_at: Instant,
}

/// Bounded reassembly with duplicate detection, timeout cleanup, and final
/// SHA-256 verification.
pub struct FragmentAssembler {
    limits: FragmentLimits,
    assemblies: HashMap<[u8; 16], Assembly>,
    reserved_bytes: usize,
}

impl fmt::Debug for FragmentAssembler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FragmentAssembler")
            .field("active_transfers", &self.assemblies.len())
            .field("reserved_bytes", &self.reserved_bytes)
            .finish()
    }
}

impl FragmentAssembler {
    /// Creates an assembler after validating the configured hard limits.
    pub fn new(limits: FragmentLimits) -> Result<Self, ProtocolError> {
        if limits.max_active_transfers == 0
            || limits.max_active_transfers > MAX_ACTIVE_TRANSFERS
            || limits.max_total_size == 0
            || limits.max_total_size > MAX_TRANSFER_SIZE
            || limits.max_fragments == 0
            || limits.max_fragments > MAX_FRAGMENT_COUNT
            || limits.max_fragment_size == 0
            || limits.max_fragment_size > MAX_FRAGMENT_SIZE
            || limits.max_buffered_bytes == 0
            || limits.max_buffered_bytes > MAX_BUFFERED_TRANSFER_BYTES
            || limits.transfer_timeout.is_zero()
        {
            return Err(ProtocolError::InvalidFragmentLimits);
        }
        Ok(Self {
            limits,
            assemblies: HashMap::new(),
            reserved_bytes: 0,
        })
    }

    /// Adds a fragment. An identical duplicate is idempotent. Completion is
    /// returned only after exact length and digest checks pass.
    pub fn push(&mut self, fragment: Fragment<'_>) -> Result<Option<Vec<u8>>, ProtocolError> {
        self.expire_stale();
        let id: [u8; 16] = fragment
            .transfer_id
            .try_into()
            .map_err(|_| ProtocolError::InvalidFragment)?;
        if id.iter().all(|byte| *byte == 0) {
            return Err(ProtocolError::InvalidFragment);
        }
        let digest: [u8; 32] = fragment
            .digest
            .try_into()
            .map_err(|_| ProtocolError::InvalidFragment)?;
        let count = usize::try_from(fragment.count).map_err(|_| ProtocolError::InvalidFragment)?;
        let index = usize::try_from(fragment.index).map_err(|_| ProtocolError::InvalidFragment)?;
        if count == 0
            || count > self.limits.max_fragments
            || index >= count
            || fragment.total_size == 0
            || fragment.total_size < count
            || fragment.total_size > self.limits.max_total_size
            || fragment.data.is_empty()
            || fragment.data.len() > self.limits.max_fragment_size
        {
            return Err(ProtocolError::FragmentLimitExceeded);
        }
        let maximum_size = count
            .checked_mul(self.limits.max_fragment_size)
            .ok_or(ProtocolError::FragmentLimitExceeded)?;
        if fragment.total_size > maximum_size || fragment.data.len() > fragment.total_size {
            return Err(ProtocolError::InvalidFragment);
        }

        if !self.assemblies.contains_key(&id) {
            let reserved = self
                .reserved_bytes
                .checked_add(fragment.total_size)
                .ok_or(ProtocolError::TooManyTransfers)?;
            if self.assemblies.len() >= self.limits.max_active_transfers
                || reserved > self.limits.max_buffered_bytes
            {
                return Err(ProtocolError::TooManyTransfers);
            }
            let slots = std::iter::repeat_with(|| None)
                .take(count)
                .collect::<Vec<Option<Vec<u8>>>>();
            self.assemblies.insert(
                id,
                Assembly {
                    digest,
                    total_size: fragment.total_size,
                    slots,
                    received_bytes: 0,
                    received_count: 0,
                    started_at: Instant::now(),
                },
            );
            self.reserved_bytes = reserved;
        }

        let assembly = self
            .assemblies
            .get_mut(&id)
            .ok_or(ProtocolError::InvalidFragment)?;
        if assembly.digest != digest
            || assembly.total_size != fragment.total_size
            || assembly.slots.len() != count
        {
            return Err(ProtocolError::InvalidFragment);
        }
        if let Some(existing) = &assembly.slots[index] {
            return if existing.as_slice() == fragment.data {
                Ok(None)
            } else {
                Err(ProtocolError::ConflictingFragment)
            };
        }
        let received_bytes = assembly
            .received_bytes
            .checked_add(fragment.data.len())
            .ok_or(ProtocolError::FragmentLimitExceeded)?;
        if received_bytes > assembly.total_size {
            return Err(ProtocolError::InvalidFragment);
        }
        assembly.slots[index] = Some(fragment.data.to_vec());
        assembly.received_bytes = received_bytes;
        assembly.received_count = assembly
            .received_count
            .checked_add(1)
            .ok_or(ProtocolError::FragmentLimitExceeded)?;
        if assembly.received_count != assembly.slots.len() {
            return Ok(None);
        }
        if assembly.received_bytes != assembly.total_size {
            self.remove_assembly(&id);
            return Err(ProtocolError::InvalidFragment);
        }

        let assembly = self
            .assemblies
            .remove(&id)
            .ok_or(ProtocolError::InvalidFragment)?;
        self.reserved_bytes = self.reserved_bytes.saturating_sub(assembly.total_size);
        let mut complete = Vec::with_capacity(assembly.total_size);
        for part in assembly.slots {
            let part = part.ok_or(ProtocolError::InvalidFragment)?;
            complete.extend_from_slice(&part);
        }
        if Sha256::digest(&complete)[..] != assembly.digest[..] {
            return Err(ProtocolError::FragmentDigestMismatch);
        }
        Ok(Some(complete))
    }

    /// Removes an incomplete transfer and releases its reserved byte budget.
    pub fn cancel(&mut self, transfer_id: &[u8]) -> bool {
        let Ok(id) = <[u8; 16]>::try_from(transfer_id) else {
            return false;
        };
        self.remove_assembly(&id)
    }

    /// Discards all timed-out assemblies and returns the number removed.
    pub fn expire_stale(&mut self) -> usize {
        let now = Instant::now();
        let timeout = self.limits.transfer_timeout;
        let stale = self
            .assemblies
            .iter()
            .filter_map(|(id, assembly)| {
                (now.saturating_duration_since(assembly.started_at) >= timeout).then_some(*id)
            })
            .collect::<Vec<_>>();
        let count = stale.len();
        for id in stale {
            self.remove_assembly(&id);
        }
        count
    }

    /// Number of incomplete transfers held by the assembler.
    #[must_use]
    pub fn active_transfers(&self) -> usize {
        self.assemblies.len()
    }

    /// Declared bytes reserved by incomplete transfers.
    #[must_use]
    pub fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }

    fn remove_assembly(&mut self, id: &[u8; 16]) -> bool {
        let Some(assembly) = self.assemblies.remove(id) else {
            return false;
        };
        self.reserved_bytes = self.reserved_bytes.saturating_sub(assembly.total_size);
        true
    }
}
