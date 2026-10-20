//! Authenticated Noise XX pairing and explicit trust authorization.
//!
//! Pairing consumes a locally hosted one-shot invitation before reading peer
//! bytes. Both devices authenticate their long-term identities inside Noise,
//! compare a transcript-bound emoji SAS, and exchange authenticated readiness
//! messages before the local SQLite trust transaction. The two devices' local
//! databases are independent and are not an atomic distributed transaction.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use hkdf::Hkdf;
use kryptopass_identity::{
    DeviceId, DeviceIdentity, InvitationTokenHash, PublicIdentity, SqliteTrustStore,
    TrustStoreError, verify_signature,
};
use kryptopass_protocol::{
    Channel, HandshakeRole, MessagePayload, NoiseHandshakeOrder, ProtocolError, SessionCodec,
    channel_for_type, decode_pairing_invite, encode_pairing_invite, proto,
};
use kryptopass_transport::{PeerTransport, TransportError};
use prost::Message;
use sha2::Sha256;
use snow::{Builder, HandshakeState, TransportState, params::NoiseParams};
use thiserror::Error;
use tokio::time::timeout;
use zeroize::{Zeroize, Zeroizing};

/// Current pairing and protocol version.
pub const PAIRING_VERSION: u32 = 1;
/// The supported Noise protocol suite.
pub const NOISE_SUITE: &str = "Noise_XX_25519_AESGCM_SHA256";
/// Noise handshake payloads are deliberately small; XX only carries an Ed key.
pub const MAX_NOISE_HANDSHAKE_MESSAGE_SIZE: usize = 1024;
/// Maximum Noise plaintext accepted by this layer.
pub const MAX_NOISE_PLAINTEXT_SIZE: usize = 65_519;
/// Maximum Noise ciphertext accepted by this layer.
pub const MAX_NOISE_CIPHERTEXT_SIZE: usize = 65_535;
/// Default maximum time for one interactive pairing attempt.
pub const DEFAULT_PAIRING_TIMEOUT: Duration = Duration::from_secs(120);
/// Hard upper bound on the configured pairing timeout.
pub const MAX_PAIRING_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
/// Hosted invitations are limited to ten minutes.
pub const MAX_INVITATION_LIFETIME: Duration = Duration::from_secs(10 * 60);

const PROLOGUE_DOMAIN: &[u8] = b"org.kryptopass.noise-prologue.v1\0";
const CONTEXT_DOMAIN: &[u8] = b"org.kryptopass.pairing.context.v1\0";
const IDENTITY_PROOF_DOMAIN: &[u8] = b"org.kryptopass.pairing.identity-proof.v1\0";
const SAS_INFO: &[u8] = b"org.kryptopass.pairing.sas-emoji.v1\0";

/// Which endpoint is taking part in Noise XX.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingRole {
    /// Noise initiator, sending XX message 1.
    Initiator,
    /// Noise responder, receiving XX message 1.
    Responder,
}

impl PairingRole {
    const fn handshake_role(self) -> HandshakeRole {
        match self {
            Self::Initiator => HandshakeRole::Initiator,
            Self::Responder => HandshakeRole::Responder,
        }
    }

    /// Canonical role byte included in the signed identity statement.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        match self {
            Self::Initiator => 0,
            Self::Responder => 1,
        }
    }
}

/// Pairing mode included in the Noise prologue and signed context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingMode {
    /// Create a new trust record after matching human SAS approval.
    Pairing,
    /// Reauthenticate an already trusted peer without showing a SAS.
    Reconnect,
}

impl PairingMode {
    /// Canonical mode byte included in prologue and context.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        match self {
            Self::Pairing => 1,
            Self::Reconnect => 2,
        }
    }
}

/// State names used for internal transitions and successful outcomes. The
/// engine does not expose a live progress trace while `run` is in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingState {
    /// No pairing attempt has started.
    Idle,
    /// A pairing invitation is ready for an attempt.
    InvitationCreated,
    /// Checking local configuration and consuming a hosted invitation.
    Connecting,
    /// Running the three ordered Noise XX handshake messages.
    NoiseHandshake,
    /// Checking Noise static keys, Ed25519 keys, and transcript proofs.
    IdentityBinding,
    /// Waiting for an explicit local human decision and remote confirmation.
    AwaitingSasConfirmation,
    /// Exchanging authenticated SAS and completion readiness messages.
    ConfirmationExchange,
    /// The local trust transaction is being committed.
    PersistingTrust,
    /// Local trust was committed for the authenticated peer.
    Trusted,
    /// The user rejected the peer or SAS.
    Rejected,
    /// The user or local application cancelled the attempt.
    Cancelled,
    /// The invitation or operation deadline expired.
    Expired,
    /// An authentication, protocol, transport, or storage check failed.
    Failed,
}

/// Parameters supplied by the application for one pairing attempt.
#[derive(Clone, Debug)]
pub struct PairingConfig {
    /// Fresh, non-zero 16-byte session identifier shared by both endpoints.
    pub session_id: [u8; 16],
    /// This endpoint's Noise role.
    pub role: PairingRole,
    /// Pairing or reauthentication mode.
    pub mode: PairingMode,
    /// Unix timestamp in milliseconds sampled when this config is created.
    /// The engine adds monotonic elapsed time so a delayed `run` cannot
    /// rejuvenate an invitation.
    now_ms: i64,
    /// Deadline for asynchronous pairing work. Checked before the synchronous
    /// SQLite commit, which cannot be preempted after it starts.
    pub timeout: Duration,
    /// The exact peer identity expected during reconnect; required in that mode.
    pub expected_peer: Option<PublicIdentity>,
    created_at: tokio::time::Instant,
}

impl PairingConfig {
    /// Creates a new-pairing configuration with a caller-provided fresh session ID.
    /// `now_ms` must be sampled when this constructor is called.
    #[must_use]
    pub fn pairing(session_id: [u8; 16], role: PairingRole, now_ms: i64) -> Self {
        Self {
            session_id,
            role,
            mode: PairingMode::Pairing,
            now_ms,
            timeout: DEFAULT_PAIRING_TIMEOUT,
            expected_peer: None,
            created_at: tokio::time::Instant::now(),
        }
    }

    /// Creates a reconnect configuration bound to the exact trusted peer keys.
    /// `now_ms` must be sampled when this constructor is called.
    #[must_use]
    pub fn reconnect(
        session_id: [u8; 16],
        role: PairingRole,
        now_ms: i64,
        expected_peer: PublicIdentity,
    ) -> Self {
        Self {
            session_id,
            role,
            mode: PairingMode::Reconnect,
            now_ms,
            timeout: DEFAULT_PAIRING_TIMEOUT,
            expected_peer: Some(expected_peer),
            created_at: tokio::time::Instant::now(),
        }
    }

    fn effective_now_ms(&self) -> Result<i64, PairingError> {
        let elapsed_ms = i64::try_from(self.created_at.elapsed().as_millis())
            .map_err(|_| PairingError::TimestampOverflow)?;
        self.now_ms
            .checked_add(elapsed_ms)
            .ok_or(PairingError::TimestampOverflow)
    }

    fn validate(
        &self,
        identity: &DeviceIdentity,
        store: &SqliteTrustStore,
    ) -> Result<(), PairingError> {
        if self.session_id.iter().all(|byte| *byte == 0)
            || self.now_ms < 0
            || self.timeout.is_zero()
            || self.timeout > MAX_PAIRING_TIMEOUT
            || store.local_device_id() != identity.device_id()
        {
            return Err(PairingError::InvalidConfiguration);
        }
        match (self.mode, self.expected_peer) {
            (PairingMode::Pairing, None) => {}
            (PairingMode::Reconnect, Some(peer)) => {
                peer.validate()
                    .map_err(|_| PairingError::InvalidPeerIdentity)?;
                if peer.device_id == identity.device_id() {
                    return Err(PairingError::InvalidPeerIdentity);
                }
            }
            _ => return Err(PairingError::InvalidConfiguration),
        }
        Ok(())
    }
}

/// A one-shot invitation. The raw bearer token is never exposed by Debug or
/// any accessor; use InvitationQr to transfer it out of band.
pub struct PairingInvitation {
    invite_id: [u8; 16],
    token: Zeroizing<[u8; 32]>,
    token_hash: InvitationTokenHash,
    expires_at_ms: i64,
}

impl PairingInvitation {
    /// Creates a random invitation and registers its hash in the local ledger.
    pub fn create(
        store: &mut SqliteTrustStore,
        now_ms: i64,
        lifetime: Duration,
    ) -> Result<Self, PairingError> {
        if now_ms < 0 || lifetime.is_zero() || lifetime > MAX_INVITATION_LIFETIME {
            return Err(PairingError::InvalidInvitation);
        }
        let lifetime_ms =
            i64::try_from(lifetime.as_millis()).map_err(|_| PairingError::InvalidInvitation)?;
        let expires_at_ms = now_ms
            .checked_add(lifetime_ms)
            .ok_or(PairingError::InvalidInvitation)?;
        let mut invite_id = [0_u8; 16];
        let mut token = Zeroizing::new([0_u8; 32]);
        getrandom::fill(&mut invite_id).map_err(|_| PairingError::Randomness)?;
        getrandom::fill(&mut *token).map_err(|_| PairingError::Randomness)?;
        if invite_id.iter().all(|byte| *byte == 0) || token.iter().all(|byte| *byte == 0) {
            return Err(PairingError::Randomness);
        }
        let token_hash = store.register_invitation(invite_id, &token, expires_at_ms, now_ms)?;
        Ok(Self {
            invite_id,
            token,
            token_hash,
            expires_at_ms,
        })
    }

    /// Parses the bounded Protocol Buffers bootstrap object received out of band.
    pub fn from_qr(bytes: &[u8]) -> Result<Self, PairingError> {
        let mut wire = decode_pairing_invite(bytes)?;
        let invite_id = wire.invite_id.as_slice().try_into();
        let mut token = Zeroizing::new([0_u8; 32]);
        token.copy_from_slice(&wire.single_use_token);
        wire.single_use_token.zeroize();
        let invite_id: [u8; 16] = invite_id.map_err(|_| PairingError::InvalidInvitation)?;
        let expires_at_ms =
            i64::try_from(wire.expires_at_unix_ms).map_err(|_| PairingError::InvalidInvitation)?;
        Ok(Self {
            invite_id,
            token_hash: InvitationTokenHash::from_token(&token),
            token,
            expires_at_ms,
        })
    }

    /// Encodes this invitation for a QR code or another out-of-band channel.
    pub fn to_qr_bytes(&self) -> Result<InvitationQr, PairingError> {
        let mut wire = proto::PairingInvite {
            invite_id: self.invite_id.to_vec(),
            single_use_token: self.token.to_vec(),
            expires_at_unix_ms: self.expires_at_ms as u64,
            protocol_version: PAIRING_VERSION,
        };
        let result = encode_pairing_invite(&wire);
        wire.single_use_token.zeroize();
        Ok(InvitationQr(Zeroizing::new(result?)))
    }

    /// Returns the public invitation identifier.
    #[must_use]
    pub const fn invite_id(&self) -> &[u8; 16] {
        &self.invite_id
    }

    /// Returns the Unix millisecond expiry.
    #[must_use]
    pub const fn expires_at_ms(&self) -> i64 {
        self.expires_at_ms
    }

    fn token(&self) -> &[u8; 32] {
        &self.token
    }

    fn token_hash(&self) -> &InvitationTokenHash {
        &self.token_hash
    }
}

impl fmt::Debug for PairingInvitation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PairingInvitation")
            .field("invite_id", &"[redacted]")
            .field("token", &"[redacted]")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

/// Zeroizing QR/bootstrap bytes with a redacted Debug implementation.
pub struct InvitationQr(Zeroizing<Vec<u8>>);

impl InvitationQr {
    /// Returns the encoded bytes. Keep them out of logs and discard the wrapper
    /// after displaying or transferring the invitation.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for InvitationQr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvitationQr")
            .field("length", &self.0.len())
            .field("bytes", &"[redacted]")
            .finish()
    }
}

/// Noise XX prologue bytes, zeroized on drop and redacted in diagnostics.
pub struct NoisePrologue(Zeroizing<Vec<u8>>);

impl NoisePrologue {
    /// Borrows the complete canonical prologue for Snow or a synthetic vector.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for NoisePrologue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoisePrologue")
            .field("length", &self.0.len())
            .field("bytes", &"[redacted]")
            .finish()
    }
}

/// Builds the byte-exact Noise prologue. Pairing includes the local raw token;
/// reconnect uses zero invitation fields and never needs an invitation.
pub fn build_prologue(
    mode: PairingMode,
    session_id: [u8; 16],
    invitation: Option<&PairingInvitation>,
) -> Result<NoisePrologue, PairingError> {
    if session_id.iter().all(|byte| *byte == 0) {
        return Err(PairingError::InvalidConfiguration);
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(128));
    bytes.extend_from_slice(PROLOGUE_DOMAIN);
    bytes.extend_from_slice(&PAIRING_VERSION.to_be_bytes());
    bytes.push(mode.as_byte());
    bytes.extend_from_slice(&session_id);
    match (mode, invitation) {
        (PairingMode::Pairing, Some(invitation)) => {
            bytes.extend_from_slice(invitation.invite_id());
            bytes.extend_from_slice(invitation.token());
            let expiry = u64::try_from(invitation.expires_at_ms)
                .map_err(|_| PairingError::InvalidInvitation)?;
            bytes.extend_from_slice(&expiry.to_be_bytes());
        }
        (PairingMode::Reconnect, None) => {
            bytes.extend_from_slice(&[0_u8; 16]);
            bytes.extend_from_slice(&[0_u8; 32]);
            bytes.extend_from_slice(&0_u64.to_be_bytes());
        }
        _ => return Err(PairingError::InvalidInvitation),
    }
    Ok(NoisePrologue(bytes))
}

/// Canonical, non-secret identity context shared by both endpoints.
pub struct CanonicalContext {
    bytes: Vec<u8>,
    final_noise_hash: [u8; 32],
}

impl CanonicalContext {
    /// Constructs the exact signed/SAS context in initiator/responder key order.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mode: PairingMode,
        session_id: [u8; 16],
        invite_id: [u8; 16],
        token_hash: [u8; 32],
        expires_at_ms: u64,
        final_noise_hash: [u8; 32],
        static_i: [u8; 32],
        static_r: [u8; 32],
        ed_i: [u8; 32],
        ed_r: [u8; 32],
    ) -> Result<Self, PairingError> {
        if session_id.iter().all(|byte| *byte == 0) {
            return Err(PairingError::InvalidConfiguration);
        }
        match mode {
            PairingMode::Pairing
                if invite_id.iter().all(|byte| *byte == 0)
                    || token_hash.iter().all(|byte| *byte == 0)
                    || expires_at_ms == 0 =>
            {
                return Err(PairingError::InvalidInvitation);
            }
            PairingMode::Reconnect
                if invite_id.iter().any(|byte| *byte != 0)
                    || token_hash.iter().any(|byte| *byte != 0)
                    || expires_at_ms != 0 =>
            {
                return Err(PairingError::InvalidInvitation);
            }
            _ => {}
        }
        let mut bytes =
            Vec::with_capacity(CONTEXT_DOMAIN.len() + 4 + 1 + 16 + 16 + 32 + 8 + 32 * 5);
        bytes.extend_from_slice(CONTEXT_DOMAIN);
        bytes.extend_from_slice(&PAIRING_VERSION.to_be_bytes());
        bytes.push(mode.as_byte());
        bytes.extend_from_slice(&session_id);
        bytes.extend_from_slice(&invite_id);
        bytes.extend_from_slice(&token_hash);
        bytes.extend_from_slice(&expires_at_ms.to_be_bytes());
        bytes.extend_from_slice(&final_noise_hash);
        bytes.extend_from_slice(&static_i);
        bytes.extend_from_slice(&static_r);
        bytes.extend_from_slice(&ed_i);
        bytes.extend_from_slice(&ed_r);
        Ok(Self {
            bytes,
            final_noise_hash,
        })
    }

    /// Returns the complete canonical byte string.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the final Noise handshake hash bound into this context.
    #[must_use]
    pub const fn final_noise_hash(&self) -> &[u8; 32] {
        &self.final_noise_hash
    }
}

impl fmt::Debug for CanonicalContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CanonicalContext")
            .field("length", &self.bytes.len())
            .field("final_noise_hash", &"[redacted]")
            .finish()
    }
}

/// Returns the role-separated Ed25519 statement for one canonical context.
#[must_use]
pub fn identity_proof_message(role: PairingRole, context: &CanonicalContext) -> Vec<u8> {
    let mut statement = Vec::with_capacity(IDENTITY_PROOF_DOMAIN.len() + 1 + context.bytes.len());
    statement.extend_from_slice(IDENTITY_PROOF_DOMAIN);
    statement.push(role.as_byte());
    statement.extend_from_slice(context.as_bytes());
    statement
}

/// One Matrix v1.16 emoji and its official English description.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SasEmoji {
    /// Six-bit Matrix SAS index, from 0 through 63.
    pub index: u8,
    /// Exact emoji string from the Matrix v1.16 SAS catalog.
    pub emoji: &'static str,
    /// Exact English description from the Matrix v1.16 SAS catalog.
    pub name: &'static str,
}

/// The seven six-bit symbols displayed for a pairing SAS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SasDisplay {
    /// Seven symbols, in canonical SAS order.
    pub emojis: [SasEmoji; 7],
}

impl SasDisplay {
    /// Returns the seven catalog indices for independent vector checking.
    #[must_use]
    pub const fn indices(&self) -> [u8; 7] {
        [
            self.emojis[0].index,
            self.emojis[1].index,
            self.emojis[2].index,
            self.emojis[3].index,
            self.emojis[4].index,
            self.emojis[5].index,
            self.emojis[6].index,
        ]
    }

    /// Returns the seven English catalog names.
    #[must_use]
    pub const fn names(&self) -> [&'static str; 7] {
        [
            self.emojis[0].name,
            self.emojis[1].name,
            self.emojis[2].name,
            self.emojis[3].name,
            self.emojis[4].name,
            self.emojis[5].name,
            self.emojis[6].name,
        ]
    }
}

/// Derives seven Matrix v1.16 emojis using the first 42 HKDF output bits.
pub fn derive_sas(context: &CanonicalContext) -> Result<SasDisplay, PairingError> {
    let hkdf = Hkdf::<Sha256>::new(Some(context.final_noise_hash()), context.as_bytes());
    let mut output = Zeroizing::new([0_u8; 6]);
    hkdf.expand(SAS_INFO, &mut *output)
        .map_err(|_| PairingError::CryptoFailure)?;
    let mut indices = [0_u8; 7];
    for (position, index) in indices.iter_mut().enumerate() {
        let bit_offset = position * 6;
        let byte_index = bit_offset / 8;
        let shift = bit_offset % 8;
        let window = (u16::from(output[byte_index]) << 8)
            | u16::from(*output.get(byte_index + 1).unwrap_or(&0));
        *index = ((window >> (10 - shift)) & 0x3f) as u8;
    }
    Ok(SasDisplay {
        emojis: indices.map(|index| {
            let (emoji, name) = SAS_EMOJI_DATA[usize::from(index)];
            SasEmoji { index, emoji, name }
        }),
    })
}

const SAS_EMOJI_DATA: [(&str, &str); 64] = [
    ("🐶", "Dog"),
    ("🐱", "Cat"),
    ("🦁", "Lion"),
    ("🐎", "Horse"),
    ("🦄", "Unicorn"),
    ("🐷", "Pig"),
    ("🐘", "Elephant"),
    ("🐰", "Rabbit"),
    ("🐼", "Panda"),
    ("🐓", "Rooster"),
    ("🐧", "Penguin"),
    ("🐢", "Turtle"),
    ("🐟", "Fish"),
    ("🐙", "Octopus"),
    ("🦋", "Butterfly"),
    ("🌷", "Flower"),
    ("🌳", "Tree"),
    ("🌵", "Cactus"),
    ("🍄", "Mushroom"),
    ("🌏", "Globe"),
    ("🌙", "Moon"),
    ("☁️", "Cloud"),
    ("🔥", "Fire"),
    ("🍌", "Banana"),
    ("🍎", "Apple"),
    ("🍓", "Strawberry"),
    ("🌽", "Corn"),
    ("🍕", "Pizza"),
    ("🎂", "Cake"),
    ("❤️", "Heart"),
    ("😀", "Smiley"),
    ("🤖", "Robot"),
    ("🎩", "Hat"),
    ("👓", "Glasses"),
    ("🔧", "Spanner"),
    ("🎅", "Santa"),
    ("👍", "Thumbs Up"),
    ("☂️", "Umbrella"),
    ("⌛", "Hourglass"),
    ("⏰", "Clock"),
    ("🎁", "Gift"),
    ("💡", "Light Bulb"),
    ("📕", "Book"),
    ("✏️", "Pencil"),
    ("📎", "Paperclip"),
    ("✂️", "Scissors"),
    ("🔒", "Lock"),
    ("🔑", "Key"),
    ("🔨", "Hammer"),
    ("☎️", "Telephone"),
    ("🏁", "Flag"),
    ("🚂", "Train"),
    ("🚲", "Bicycle"),
    ("✈️", "Aeroplane"),
    ("🚀", "Rocket"),
    ("🏆", "Trophy"),
    ("⚽", "Ball"),
    ("🎸", "Guitar"),
    ("🎺", "Trumpet"),
    ("🔔", "Bell"),
    ("⚓", "Anchor"),
    ("🎧", "Headphones"),
    ("📁", "Folder"),
    ("📌", "Pin"),
];

/// Returns the Matrix v1.16 catalog entry for one six-bit index.
#[must_use]
pub fn sas_emoji_at(index: u8) -> Option<SasEmoji> {
    let (emoji, name) = *SAS_EMOJI_DATA.get(usize::from(index))?;
    Some(SasEmoji { index, emoji, name })
}

/// Returns the complete Matrix v1.16 SAS catalog in canonical numeric order.
#[must_use]
pub fn sas_emoji_catalog() -> [SasEmoji; 64] {
    std::array::from_fn(|index| {
        let (emoji, name) = SAS_EMOJI_DATA[index];
        SasEmoji {
            index: index as u8,
            emoji,
            name,
        }
    })
}

/// A caller-visible local SAS decision. The library has no approve-all default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalDecision {
    /// The user explicitly compared and approved the displayed SAS.
    Approve,
    /// The user explicitly rejected the peer or a mismatching SAS.
    Reject,
    /// The user or local application cancelled this attempt.
    Cancel,
}

/// Application-provided interactive approval. Remote accepted=true never
/// substitutes for this local decision.
#[async_trait]
pub trait LocalApproval: Send {
    /// Ask the local user to compare all seven emoji and explicitly decide.
    async fn confirm_sas(&mut self, sas: &SasDisplay) -> LocalDecision;
}

/// Pairing failures. Variants and text never include frame, token, or key bytes.
#[derive(Debug, Error)]
pub enum PairingError {
    /// Configuration, mode, role, or local identity/store binding is invalid.
    #[error("invalid pairing configuration")]
    InvalidConfiguration,
    /// Invitation bytes, ID, token, or expiry are invalid.
    #[error("invalid or expired pairing invitation")]
    InvalidInvitation,
    /// The Unix timestamp could not be advanced by the monotonic elapsed time.
    #[error("pairing timestamp overflow")]
    TimestampOverflow,
    /// Secure random generation failed.
    #[error("secure randomness failed")]
    Randomness,
    /// The peer identity fields do not validate or match the authenticated keys.
    #[error("peer identity is invalid or changed")]
    InvalidPeerIdentity,
    /// The peer sent a message that is illegal in the current state.
    #[error("unexpected peer message for current pairing state")]
    UnexpectedMessage,
    /// The authenticated peer rejected the invitation or SAS.
    #[error("peer rejected pairing")]
    RemoteRejected,
    /// The local user rejected pairing.
    #[error("local user rejected pairing")]
    LocalRejected,
    /// The local user or app cancelled pairing.
    #[error("pairing cancelled")]
    Cancelled,
    /// A remote authenticated abort was received.
    #[error("peer aborted pairing")]
    RemoteAborted,
    /// The operation or invitation expired before trust was committed.
    #[error("pairing expired")]
    Expired,
    /// Snow, signature, transcript, proof, or SAS validation failed.
    #[error("cryptographic pairing check failed")]
    CryptoFailure,
    /// A Noise handshake or ciphertext exceeded the local bounds.
    #[error("Noise message exceeds the local size limit")]
    NoiseMessageTooLarge,
    /// The Noise handshake completed without the expected XX state.
    #[error("Noise handshake is incomplete")]
    IncompleteNoiseHandshake,
    /// The selected Noise protocol parameters were unavailable.
    #[error("Noise protocol suite is unavailable")]
    UnsupportedNoiseSuite,
    /// The underlying protocol codec rejected a frame.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// The underlying peer transport failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// Invitation ledger or trust persistence failed.
    #[error(transparent)]
    TrustStore(#[from] TrustStoreError),
    /// A verified signature was invalid.
    #[error("identity proof is invalid")]
    InvalidProof,
    /// The established session has failed or has already closed.
    #[error("authenticated session is closed")]
    SessionClosed,
}

/// Pairing engine consuming one transport and one local trust store.
pub struct PairingEngine<'a, T> {
    transport: T,
    identity: &'a DeviceIdentity,
    store: SqliteTrustStore,
    config: PairingConfig,
    state: PairingState,
}

impl<'a, T: PeerTransport> PairingEngine<'a, T> {
    /// Constructs a pairing engine without reading from the peer.
    pub fn new(
        transport: T,
        identity: &'a DeviceIdentity,
        store: SqliteTrustStore,
        config: PairingConfig,
    ) -> Result<Self, PairingError> {
        config.validate(identity, &store)?;
        if let Some(expected) = config.expected_peer {
            let stored = store.lookup_trusted_peer(&expected.device_id)?;
            if stored != expected {
                return Err(PairingError::InvalidPeerIdentity);
            }
        }
        Ok(Self {
            transport,
            identity,
            store,
            config,
            state: PairingState::Idle,
        })
    }

    /// Returns the current state before run begins.
    #[must_use]
    pub const fn state(&self) -> PairingState {
        self.state
    }

    /// Runs authentication, explicit approval, readiness exchange, then the
    /// local SQLite trust transaction. On success, the returned session keeps
    /// the transport alive while the peer finishes its own local commit.
    pub async fn run(
        mut self,
        invitation: Option<&PairingInvitation>,
        approval: &mut dyn LocalApproval,
    ) -> Result<PairingOutcome<T>, PairingError> {
        self.state = if self.config.mode == PairingMode::Pairing {
            PairingState::InvitationCreated
        } else {
            PairingState::Idle
        };
        let effective_now_ms = self.config.effective_now_ms()?;
        let invitation_remaining = match (self.config.mode, invitation) {
            (PairingMode::Pairing, Some(invitation)) => {
                if invitation.expires_at_ms <= effective_now_ms {
                    self.state = PairingState::Expired;
                    return Err(PairingError::Expired);
                }
                Some(Duration::from_millis(
                    u64::try_from(invitation.expires_at_ms - effective_now_ms)
                        .map_err(|_| PairingError::TimestampOverflow)?,
                ))
            }
            (PairingMode::Reconnect, None) => None,
            _ => {
                self.state = PairingState::Failed;
                return Err(PairingError::InvalidInvitation);
            }
        };
        let operation_timeout = invitation_remaining.map_or(self.config.timeout, |remaining| {
            remaining.min(self.config.timeout)
        });
        self.state = PairingState::Connecting;
        let deadline = tokio::time::Instant::now() + operation_timeout;
        let result = timeout(
            operation_timeout,
            self.run_inner(invitation, approval, deadline),
        )
        .await;
        match result {
            Err(_) => {
                self.state = PairingState::Expired;
                Err(PairingError::Expired)
            }
            Ok(Err(error)) => {
                self.state = match error {
                    PairingError::LocalRejected | PairingError::RemoteRejected => {
                        PairingState::Rejected
                    }
                    PairingError::Cancelled => PairingState::Cancelled,
                    PairingError::Expired => PairingState::Expired,
                    PairingError::RemoteAborted => PairingState::Failed,
                    _ => PairingState::Failed,
                };
                Err(error)
            }
            Ok(Ok((peer, sas, noise, codec))) => {
                self.state = PairingState::Trusted;
                Ok(PairingOutcome {
                    peer,
                    mode: self.config.mode,
                    state: PairingState::Trusted,
                    sas,
                    session: EstablishedSession {
                        transport: self.transport,
                        noise,
                        codec,
                        peer,
                        local_device_id: self.identity.device_id(),
                        closed: false,
                        failed: false,
                    },
                })
            }
        }
    }

    async fn run_inner(
        &mut self,
        invitation: Option<&PairingInvitation>,
        approval: &mut dyn LocalApproval,
        deadline: tokio::time::Instant,
    ) -> Result<
        (
            PublicIdentity,
            Option<SasDisplay>,
            TransportState,
            SessionCodec,
        ),
        PairingError,
    > {
        if let (PairingMode::Pairing, PairingRole::Responder, Some(invitation)) =
            (self.config.mode, self.config.role, invitation)
        {
            // Persist the single-use attempt before receiving attacker-controlled peer bytes.
            let consume_now_ms = self.config.effective_now_ms()?;
            if invitation.expires_at_ms <= consume_now_ms {
                return Err(PairingError::Expired);
            }
            self.store.consume_invitation(
                invitation.invite_id(),
                invitation.token_hash(),
                consume_now_ms,
            )?;
        }

        let hello_invite_id = match self.config.mode {
            PairingMode::Pairing => *invitation
                .ok_or(PairingError::InvalidInvitation)?
                .invite_id(),
            // PairingHello requires a non-zero ID. In reconnect mode the fresh
            // session ID is used only as a correlation value; the prologue has zero invite fields.
            PairingMode::Reconnect => self.config.session_id,
        };
        let mut codec = SessionCodec::new(self.config.session_id);
        self.state = PairingState::Connecting;
        if self.config.role == PairingRole::Initiator {
            send_clear(
                &mut self.transport,
                &mut codec,
                proto::MessageType::PairingHello,
                &proto::PairingHello {
                    invite_id: hello_invite_id.to_vec(),
                },
            )
            .await?;
        }
        if self.config.role == PairingRole::Responder {
            let hello = receive_clear(&mut self.transport, &mut codec).await?;
            let MessagePayload::PairingHello(hello) = hello.payload else {
                return Err(PairingError::UnexpectedMessage);
            };
            if hello.invite_id.as_slice() != hello_invite_id {
                return Err(PairingError::InvalidInvitation);
            }
        }

        self.state = PairingState::NoiseHandshake;
        let prologue = build_prologue(self.config.mode, self.config.session_id, invitation)?;
        let params: NoiseParams = NOISE_SUITE
            .parse()
            .map_err(|_| PairingError::UnsupportedNoiseSuite)?;
        let builder = Builder::new(params)
            .local_private_key(self.identity.static_secret())
            .map_err(|_| PairingError::CryptoFailure)?
            .prologue(prologue.as_bytes())
            .map_err(|_| PairingError::CryptoFailure)?;
        let mut handshake = match self.config.role {
            PairingRole::Initiator => builder.build_initiator(),
            PairingRole::Responder => builder.build_responder(),
        }
        .map_err(|_| PairingError::CryptoFailure)?;
        let remote_ed = run_noise_handshake(
            &mut self.transport,
            &mut codec,
            &mut handshake,
            self.identity,
            self.config.role,
        )
        .await?;
        let remote_static = handshake
            .get_remote_static()
            .ok_or(PairingError::IncompleteNoiseHandshake)?;
        let remote_static: [u8; 32] = remote_static
            .try_into()
            .map_err(|_| PairingError::InvalidPeerIdentity)?;
        let noise_hash: [u8; 32] = handshake
            .get_handshake_hash()
            .try_into()
            .map_err(|_| PairingError::IncompleteNoiseHandshake)?;
        let mut noise = handshake
            .into_transport_mode()
            .map_err(|_| PairingError::IncompleteNoiseHandshake)?;
        let remote = PublicIdentity::new(remote_ed, remote_static)
            .map_err(|_| PairingError::InvalidPeerIdentity)?;
        remote
            .validate()
            .map_err(|_| PairingError::InvalidPeerIdentity)?;
        if remote.device_id == self.identity.device_id() {
            return Err(PairingError::InvalidPeerIdentity);
        }
        if let Some(expected) = self.config.expected_peer {
            if remote != expected {
                return Err(PairingError::InvalidPeerIdentity);
            }
        }

        self.state = PairingState::IdentityBinding;
        let local = self.identity.public_identity();
        let (static_i, static_r, ed_i, ed_r) = match self.config.role {
            PairingRole::Initiator => (
                local.static_public_key,
                remote.static_public_key,
                local.signing_public_key,
                remote.signing_public_key,
            ),
            PairingRole::Responder => (
                remote.static_public_key,
                local.static_public_key,
                remote.signing_public_key,
                local.signing_public_key,
            ),
        };
        let invite_id = invitation.map_or([0; 16], |value| *value.invite_id());
        let token_hash = invitation.map_or([0; 32], |value| *value.token_hash().as_bytes());
        let expiry = invitation.map_or(0, |value| value.expires_at_ms as u64);
        let context = CanonicalContext::new(
            self.config.mode,
            self.config.session_id,
            invite_id,
            token_hash,
            expiry,
            noise_hash,
            static_i,
            static_r,
            ed_i,
            ed_r,
        )?;
        let proof_message = identity_proof_message(self.config.role, &context);
        let proof = self.identity.sign(&proof_message);
        let local_identity_message = proto::PairingIdentity {
            device_id: local.device_id.as_bytes().to_vec(),
            signing_public_key: local.signing_public_key.to_vec(),
            transcript_hash: noise_hash.to_vec(),
            proof: proof.to_vec(),
        };
        send_encrypted(
            &mut self.transport,
            &mut codec,
            &mut noise,
            proto::MessageType::PairingIdentity,
            &local_identity_message,
        )
        .await?;
        let remote_identity = receive_expected_encrypted(
            &mut self.transport,
            &mut codec,
            &mut noise,
            proto::MessageType::PairingIdentity,
        )
        .await?;
        let MessagePayload::PairingIdentity(remote_identity) = remote_identity.payload else {
            return Err(PairingError::UnexpectedMessage);
        };
        if remote_identity.device_id.as_slice() != remote.device_id.as_bytes()
            || remote_identity.signing_public_key.as_slice() != remote.signing_public_key
            || remote_identity.transcript_hash.as_slice() != noise_hash
        {
            return Err(PairingError::InvalidPeerIdentity);
        }
        let remote_role = match self.config.role {
            PairingRole::Initiator => PairingRole::Responder,
            PairingRole::Responder => PairingRole::Initiator,
        };
        let remote_proof = identity_proof_message(remote_role, &context);
        let proof: [u8; 64] = remote_identity
            .proof
            .as_slice()
            .try_into()
            .map_err(|_| PairingError::InvalidProof)?;
        verify_signature(&remote.signing_public_key, &remote_proof, &proof)
            .map_err(|_| PairingError::InvalidProof)?;

        let sas = if self.config.mode == PairingMode::Pairing {
            Some(derive_sas(&context)?)
        } else {
            None
        };
        if self.config.mode == PairingMode::Pairing {
            self.state = PairingState::AwaitingSasConfirmation;
            let sas_display = sas.as_ref().ok_or(PairingError::CryptoFailure)?;
            let local_decision = wait_for_sas_decision(
                &mut self.transport,
                &mut codec,
                &mut noise,
                approval,
                sas_display,
                &noise_hash,
            )
            .await?;
            if local_decision != LocalDecision::Approve {
                return Err(if local_decision == LocalDecision::Reject {
                    PairingError::LocalRejected
                } else {
                    PairingError::Cancelled
                });
            }
        }

        self.state = PairingState::ConfirmationExchange;
        send_encrypted(
            &mut self.transport,
            &mut codec,
            &mut noise,
            proto::MessageType::PairingComplete,
            &proto::PairingComplete {
                transcript_hash: noise_hash.to_vec(),
            },
        )
        .await?;
        let completion = receive_expected_encrypted(
            &mut self.transport,
            &mut codec,
            &mut noise,
            proto::MessageType::PairingComplete,
        )
        .await?;
        let MessagePayload::PairingComplete(completion) = completion.payload else {
            return Err(PairingError::UnexpectedMessage);
        };
        if completion.transcript_hash.as_slice() != noise_hash {
            return Err(PairingError::InvalidProof);
        }

        self.state = PairingState::PersistingTrust;
        if tokio::time::Instant::now() >= deadline {
            return Err(PairingError::Expired);
        }
        if self.config.mode == PairingMode::Pairing {
            self.store
                .authorize_peer(&remote, self.config.effective_now_ms()?)?;
        } else {
            // Reconnect never repairs or recreates a trust record. Recheck the
            // exact keys at the commit boundary to catch revocation during XX.
            let trusted = self.store.lookup_trusted_peer(&remote.device_id)?;
            if trusted != remote {
                return Err(PairingError::InvalidPeerIdentity);
            }
        }
        // This is the final fallible local action. Do not perform network I/O
        // after commit that could turn an authorized record into reported failure.
        Ok((remote, sas, noise, codec))
    }
}

/// Result of a committed pairing. It retains the authenticated channel until
/// explicitly closed or dropped, allowing the peer to finish its own commit.
pub struct PairingOutcome<T> {
    /// The identity authenticated by Noise and its signed context.
    pub peer: PublicIdentity,
    /// The mode used for this attempt.
    pub mode: PairingMode,
    /// Final state; successful outcomes are always Trusted.
    pub state: PairingState,
    /// Present only for a newly paired device.
    pub sas: Option<SasDisplay>,
    /// Live authenticated connection that must be retained or explicitly closed.
    pub session: EstablishedSession<T>,
}

/// Authenticated Noise transport retained after local trust has committed.
pub struct EstablishedSession<T> {
    transport: T,
    noise: TransportState,
    codec: SessionCodec,
    peer: PublicIdentity,
    local_device_id: DeviceId,
    closed: bool,
    failed: bool,
}

impl<T: PeerTransport> EstablishedSession<T> {
    /// Read volatile carrier observations; this cannot change the authenticated identity.
    pub async fn connection_info(&mut self) -> kryptopass_transport::ConnectionInfo {
        if self.closed || self.failed {
            return kryptopass_transport::ConnectionInfo::default();
        }
        self.transport.connection_info().await
    }

    /// Returns the exact public identity authenticated by this Noise handshake.
    pub const fn peer_identity(&self) -> PublicIdentity {
        self.peer
    }

    /// Returns the local identity bound to this authenticated session.
    pub const fn local_device_id(&self) -> DeviceId {
        self.local_device_id
    }

    /// Sends a typed, validated message through the existing Noise transport
    /// state and session codec. A send error or cancelled send poisons this
    /// session so its cipher nonce and protocol counters can never be reused.
    /// Callers should await sends to completion and tear down the whole session
    /// if the calling task is cancelled.
    pub async fn send_message<M: Message>(
        &mut self,
        message_type: proto::MessageType,
        payload: &M,
    ) -> Result<(), PairingError> {
        if self.closed || self.failed {
            let _ = self.close().await;
            return Err(PairingError::SessionClosed);
        }

        let mut operation = SessionOperationGuard::new(&mut self.failed);
        let result = async {
            let channel = channel_for_type(message_type as i32)?;
            let mut plaintext = Zeroizing::new(self.codec.encode(message_type, payload)?);
            if plaintext.len() > MAX_NOISE_PLAINTEXT_SIZE {
                plaintext.zeroize();
                return Err(PairingError::NoiseMessageTooLarge);
            }
            let mut ciphertext = Zeroizing::new(vec![0_u8; MAX_NOISE_CIPHERTEXT_SIZE]);
            let written = self
                .noise
                .write_message(&plaintext, &mut ciphertext)
                .map_err(|_| PairingError::CryptoFailure)?;
            plaintext.zeroize();
            if !(16..=MAX_NOISE_CIPHERTEXT_SIZE).contains(&written) {
                return Err(PairingError::NoiseMessageTooLarge);
            }
            self.transport.send(channel, &ciphertext[..written]).await?;
            ciphertext.zeroize();
            Ok(())
        }
        .await;
        if result.is_ok() {
            operation.complete();
        }
        drop(operation);
        if result.is_err() {
            let _ = self.close().await;
        }
        result
    }

    /// Receives and authenticates one typed message using the existing Noise
    /// transport state and session codec. Authentication, framing, channel, or
    /// protocol errors close the session immediately.
    pub async fn receive_message(
        &mut self,
    ) -> Result<kryptopass_protocol::DecodedMessage, PairingError> {
        if self.closed || self.failed {
            let _ = self.close().await;
            return Err(PairingError::SessionClosed);
        }

        let result = async {
            let frame = self.transport.receive().await?;
            if frame.data.len() > MAX_NOISE_CIPHERTEXT_SIZE || frame.data.len() < 16 {
                return Err(PairingError::UnexpectedMessage);
            }
            let mut plaintext = Zeroizing::new(vec![0_u8; MAX_NOISE_PLAINTEXT_SIZE]);
            let written = self
                .noise
                .read_message(&frame.data, &mut plaintext)
                .map_err(|_| PairingError::CryptoFailure)?;
            if written > MAX_NOISE_PLAINTEXT_SIZE {
                return Err(PairingError::NoiseMessageTooLarge);
            }
            self.codec
                .decode_on(frame.channel, &plaintext[..written])
                .map_err(Into::into)
        }
        .await;
        if result.is_err() {
            self.failed = true;
            let _ = self.close().await;
        }
        result
    }

    /// Closes the authenticated peer connection. Cleanup is idempotent.
    pub async fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.transport.close().await
    }
}

struct SessionOperationGuard<'a> {
    failed: &'a mut bool,
    completed: bool,
}

impl<'a> SessionOperationGuard<'a> {
    fn new(failed: &'a mut bool) -> Self {
        *failed = true;
        Self {
            failed,
            completed: false,
        }
    }

    fn complete(&mut self) {
        *self.failed = false;
        self.completed = true;
    }
}

impl Drop for SessionOperationGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            *self.failed = true;
        }
    }
}

/// Sends a canonical protocol message before Noise starts.
async fn send_clear<T: PeerTransport, M: Message>(
    transport: &mut T,
    codec: &mut SessionCodec,
    message_type: proto::MessageType,
    payload: &M,
) -> Result<(), PairingError> {
    let frame = codec.encode(message_type, payload)?;
    transport.send(Channel::Control, &frame).await?;
    Ok(())
}

async fn receive_clear<T: PeerTransport>(
    transport: &mut T,
    codec: &mut SessionCodec,
) -> Result<kryptopass_protocol::DecodedMessage, PairingError> {
    let frame = transport.receive().await?;
    codec
        .decode_on(frame.channel, &frame.data)
        .map_err(Into::into)
}

async fn run_noise_handshake<T: PeerTransport>(
    transport: &mut T,
    codec: &mut SessionCodec,
    handshake: &mut HandshakeState,
    identity: &DeviceIdentity,
    role: PairingRole,
) -> Result<[u8; 32], PairingError> {
    let mut order = NoiseHandshakeOrder::new(role.handshake_role());
    let mut remote_ed = None;
    let mut message_buffer = Zeroizing::new(vec![0_u8; MAX_NOISE_CIPHERTEXT_SIZE]);
    let mut payload_buffer = Zeroizing::new(vec![0_u8; MAX_NOISE_PLAINTEXT_SIZE]);
    while !order.is_complete() {
        if let Some(step) = order.next_outgoing() {
            let local_ed = identity.signing_public_key();
            let payload: &[u8] = if step == 1 { &[] } else { &local_ed };
            let written = handshake
                .write_message(payload, &mut message_buffer)
                .map_err(|_| PairingError::CryptoFailure)?;
            if written == 0 || written > MAX_NOISE_HANDSHAKE_MESSAGE_SIZE {
                return Err(PairingError::NoiseMessageTooLarge);
            }
            send_clear(
                transport,
                codec,
                proto::MessageType::NoiseHandshake,
                &proto::NoiseHandshake {
                    step,
                    handshake_message: message_buffer[..written].to_vec(),
                },
            )
            .await?;
            order.record_outgoing(step)?;
        } else if let Some(step) = order.next_incoming() {
            let message = receive_clear(transport, codec).await?;
            let MessagePayload::NoiseHandshake(noise_message) = message.payload else {
                return Err(PairingError::UnexpectedMessage);
            };
            if noise_message.step != step
                || noise_message.handshake_message.is_empty()
                || noise_message.handshake_message.len() > MAX_NOISE_HANDSHAKE_MESSAGE_SIZE
            {
                return Err(PairingError::UnexpectedMessage);
            }
            let payload_len = handshake
                .read_message(&noise_message.handshake_message, &mut payload_buffer)
                .map_err(|_| PairingError::CryptoFailure)?;
            match step {
                1 if payload_len == 0 => {}
                2 | 3 if payload_len == 32 => {
                    let ed_key: [u8; 32] = payload_buffer[..32]
                        .try_into()
                        .map_err(|_| PairingError::InvalidPeerIdentity)?;
                    remote_ed = Some(ed_key);
                }
                _ => return Err(PairingError::InvalidPeerIdentity),
            }
            order.record_incoming(step)?;
        } else {
            return Err(PairingError::UnexpectedMessage);
        }
    }
    remote_ed.ok_or(PairingError::IncompleteNoiseHandshake)
}

async fn send_encrypted<T: PeerTransport, M: Message>(
    transport: &mut T,
    codec: &mut SessionCodec,
    noise: &mut TransportState,
    message_type: proto::MessageType,
    payload: &M,
) -> Result<(), PairingError> {
    let plaintext = codec.encode(message_type, payload)?;
    if plaintext.len() > MAX_NOISE_PLAINTEXT_SIZE {
        return Err(PairingError::NoiseMessageTooLarge);
    }
    let mut ciphertext = Zeroizing::new(vec![0_u8; MAX_NOISE_CIPHERTEXT_SIZE]);
    let written = noise
        .write_message(&plaintext, &mut ciphertext)
        .map_err(|_| PairingError::CryptoFailure)?;
    if written > MAX_NOISE_CIPHERTEXT_SIZE {
        return Err(PairingError::NoiseMessageTooLarge);
    }
    transport
        .send(Channel::Control, &ciphertext[..written])
        .await?;
    Ok(())
}

async fn receive_encrypted<T: PeerTransport>(
    transport: &mut T,
    codec: &mut SessionCodec,
    noise: &mut TransportState,
) -> Result<kryptopass_protocol::DecodedMessage, PairingError> {
    let frame = transport.receive().await?;
    if frame.channel != Channel::Control
        || frame.data.len() > MAX_NOISE_CIPHERTEXT_SIZE
        || frame.data.len() < 16
    {
        return Err(PairingError::UnexpectedMessage);
    }
    let mut plaintext = Zeroizing::new(vec![0_u8; MAX_NOISE_PLAINTEXT_SIZE]);
    let written = noise
        .read_message(&frame.data, &mut plaintext)
        .map_err(|_| PairingError::CryptoFailure)?;
    if written > MAX_NOISE_PLAINTEXT_SIZE {
        return Err(PairingError::NoiseMessageTooLarge);
    }
    codec
        .decode_on(Channel::Control, &plaintext[..written])
        .map_err(Into::into)
}

async fn receive_expected_encrypted<T: PeerTransport>(
    transport: &mut T,
    codec: &mut SessionCodec,
    noise: &mut TransportState,
    expected_type: proto::MessageType,
) -> Result<kryptopass_protocol::DecodedMessage, PairingError> {
    let message = receive_encrypted(transport, codec, noise).await?;
    if let MessagePayload::PairingAbort(abort) = &message.payload {
        return Err(remote_abort_error(abort));
    }
    if message.message_type != expected_type {
        return Err(PairingError::UnexpectedMessage);
    }
    Ok(message)
}

async fn wait_for_sas_decision<T: PeerTransport>(
    transport: &mut T,
    codec: &mut SessionCodec,
    noise: &mut TransportState,
    approval: &mut dyn LocalApproval,
    sas: &SasDisplay,
    transcript_hash: &[u8; 32],
) -> Result<LocalDecision, PairingError> {
    let decision_future = approval.confirm_sas(sas);
    tokio::pin!(decision_future);
    let mut local_decision = None;
    let mut remote_accepted = false;

    while local_decision.is_none() || !remote_accepted {
        enum Selected {
            Decision(LocalDecision),
            Message(kryptopass_protocol::DecodedMessage),
        }
        let selected = {
            let incoming = receive_encrypted(transport, codec, noise);
            tokio::pin!(incoming);
            tokio::select! {
                decision = &mut decision_future, if local_decision.is_none() => Selected::Decision(decision),
                message = &mut incoming => Selected::Message(message?),
            }
        };
        match selected {
            Selected::Decision(decision) => {
                local_decision = Some(decision);
                match decision {
                    LocalDecision::Approve | LocalDecision::Reject => {
                        send_encrypted(
                            transport,
                            codec,
                            noise,
                            proto::MessageType::PairingConfirmation,
                            &proto::PairingConfirmation {
                                transcript_hash: transcript_hash.to_vec(),
                                accepted: decision == LocalDecision::Approve,
                            },
                        )
                        .await?;
                    }
                    LocalDecision::Cancel => {
                        send_encrypted(
                            transport,
                            codec,
                            noise,
                            proto::MessageType::PairingAbort,
                            &proto::PairingAbort {
                                code: proto::AbortCode::UserCancelled as i32,
                            },
                        )
                        .await?;
                        return Ok(LocalDecision::Cancel);
                    }
                }
                if decision == LocalDecision::Reject {
                    return Ok(LocalDecision::Reject);
                }
            }
            Selected::Message(message) => match message.payload {
                MessagePayload::PairingAbort(abort) => {
                    return Err(remote_abort_error(&abort));
                }
                MessagePayload::PairingConfirmation(confirmation) => {
                    if confirmation.transcript_hash.as_slice() != transcript_hash {
                        return Err(PairingError::InvalidProof);
                    }
                    if !confirmation.accepted {
                        return Err(PairingError::RemoteRejected);
                    }
                    if remote_accepted {
                        return Err(PairingError::UnexpectedMessage);
                    }
                    remote_accepted = true;
                }
                _ => return Err(PairingError::UnexpectedMessage),
            },
        }
    }
    if local_decision != Some(LocalDecision::Approve) {
        return Err(PairingError::UnexpectedMessage);
    }
    Ok(LocalDecision::Approve)
}

fn remote_abort_error(abort: &proto::PairingAbort) -> PairingError {
    match proto::AbortCode::try_from(abort.code) {
        Ok(proto::AbortCode::UserCancelled) => PairingError::Cancelled,
        Ok(proto::AbortCode::Expired) => PairingError::Expired,
        Ok(proto::AbortCode::Rejected) => PairingError::RemoteRejected,
        Ok(proto::AbortCode::Failed) | Ok(proto::AbortCode::Unspecified) | Err(_) => {
            PairingError::RemoteAborted
        }
    }
}
