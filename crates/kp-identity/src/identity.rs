use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use kryptopass_secure_memory::SensitiveBytes;
use sha2::{Digest, Sha256};
use std::fmt;
use thiserror::Error;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::{SecureKeyStore, SecureStoreError, StoreIfAbsent};

const IDENTITY_RECORD_MAGIC: &[u8; 4] = b"KPID";
const IDENTITY_RECORD_VERSION: u16 = 1;
const IDENTITY_RECORD_PAYLOAD_SIZE: usize = 4 + 2 + 32 + 32;
const IDENTITY_RECORD_CHECKSUM_SIZE: usize = 32;
const IDENTITY_RECORD_SIZE: usize = IDENTITY_RECORD_PAYLOAD_SIZE + IDENTITY_RECORD_CHECKSUM_SIZE;
const DEVICE_ID_DOMAIN: &[u8] = b"org.kryptopass.device-id.v1\0";
const IDENTITY_RECORD_CHECKSUM_DOMAIN: &[u8] = b"org.kryptopass.device-identity-record.v1\0";
const INVITATION_TOKEN_HASH_DOMAIN: &[u8] = b"org.kryptopass.invitation-token-hash.v1\0";

/// Stable identifier derived from an Ed25519 signing public key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId([u8; 32]);

impl DeviceId {
    /// Constructs an identifier from its fixed-width wire or database form.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the complete 32-byte identifier.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub(crate) fn from_signing_key(signing_public_key: &[u8; 32]) -> Self {
        let mut hash = Sha256::new();
        hash.update(DEVICE_ID_DOMAIN);
        hash.update(signing_public_key);
        Self(hash.finalize().into())
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("DeviceId").field(&Hex(&self.0)).finish()
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&Hex(&self.0), f)
    }
}

struct Hex<'a>(&'a [u8]);

impl fmt::Display for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Public, independently verifiable device identity material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicIdentity {
    /// Identifier derived from `signing_public_key`.
    pub device_id: DeviceId,
    /// Ed25519 key used to authenticate device protocol statements.
    pub signing_public_key: [u8; 32],
    /// Static X25519 key used by Noise.
    pub static_public_key: [u8; 32],
}

impl PublicIdentity {
    /// Creates and validates the public identity derived from the supplied keys.
    pub fn new(
        signing_public_key: [u8; 32],
        static_public_key: [u8; 32],
    ) -> Result<Self, SignatureError> {
        validate_signing_key(&signing_public_key)?;
        validate_static_key(&static_public_key)?;
        Ok(Self {
            device_id: DeviceId::from_signing_key(&signing_public_key),
            signing_public_key,
            static_public_key,
        })
    }

    /// Checks the key encodings and confirms the identifier matches the signing key.
    pub fn validate(&self) -> Result<(), SignatureError> {
        validate_signing_key(&self.signing_public_key)?;
        validate_static_key(&self.static_public_key)?;
        if self.device_id != DeviceId::from_signing_key(&self.signing_public_key) {
            return Err(SignatureError::DeviceIdMismatch);
        }
        Ok(())
    }
}

/// Private device identity; it intentionally has no `Debug` or `Clone` implementation.
pub struct DeviceIdentity {
    static_secret: StaticSecret,
    signing_key: SigningKey,
    public_identity: PublicIdentity,
}

impl DeviceIdentity {
    /// Generates an ephemeral in-memory identity.
    ///
    /// Use [`Self::create_new`] when initializing a persistent profile.
    pub fn generate() -> Result<Self, IdentityError> {
        let mut static_seed = Zeroizing::new([0_u8; 32]);
        let mut signing_seed = Zeroizing::new([0_u8; 32]);
        getrandom::fill(&mut *static_seed).map_err(|_| IdentityError::Randomness)?;
        getrandom::fill(&mut *signing_seed).map_err(|_| IdentityError::Randomness)?;
        Ok(Self::from_seeds(&static_seed, &signing_seed))
    }

    /// Creates a new persistent identity only when the secure store is empty.
    ///
    /// The store must implement `store_if_absent` atomically for its account.
    /// A race or any storage error returns an error and never replaces an existing key.
    pub fn create_new(store: &dyn SecureKeyStore) -> Result<Self, IdentityError> {
        let identity = Self::generate()?;
        let record = identity.encode_record()?;
        match store.store_if_absent(record.as_slice())? {
            StoreIfAbsent::Stored => Ok(identity),
            StoreIfAbsent::AlreadyExists => Err(IdentityError::AlreadyExists),
        }
    }

    /// Loads a persistent identity and fails when it is absent or malformed.
    ///
    /// This method never creates or replaces key material after a read failure.
    pub fn load_existing(store: &dyn SecureKeyStore) -> Result<Self, IdentityError> {
        let record = store.load()?.ok_or(IdentityError::Missing)?;
        Self::decode_record(&record)
    }

    /// Returns the stable identifier derived from the signing public key.
    pub const fn device_id(&self) -> DeviceId {
        self.public_identity.device_id
    }

    /// Returns a copy of this device's public identity.
    pub const fn public_identity(&self) -> PublicIdentity {
        self.public_identity
    }

    /// Returns the Ed25519 public key.
    pub const fn signing_public_key(&self) -> [u8; 32] {
        self.public_identity.signing_public_key
    }

    /// Returns the static X25519 public key.
    pub const fn static_public_key(&self) -> [u8; 32] {
        self.public_identity.static_public_key
    }

    /// Borrows the static secret in the byte representation accepted by Snow.
    pub fn static_secret(&self) -> &[u8; 32] {
        self.static_secret.as_bytes()
    }

    /// Signs a protocol statement with this device's Ed25519 identity key.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.signing_key.sign(message).to_bytes()
    }

    fn from_seeds(static_seed: &[u8; 32], signing_seed: &[u8; 32]) -> Self {
        let static_secret = StaticSecret::from(*static_seed);
        let signing_key = SigningKey::from_bytes(signing_seed);
        let static_public_key = X25519PublicKey::from(&static_secret).to_bytes();
        let signing_public_key = signing_key.verifying_key().to_bytes();
        let public_identity = PublicIdentity {
            device_id: DeviceId::from_signing_key(&signing_public_key),
            signing_public_key,
            static_public_key,
        };
        Self {
            static_secret,
            signing_key,
            public_identity,
        }
    }

    fn encode_record(&self) -> Result<SensitiveBytes, IdentityError> {
        let mut record = SensitiveBytes::try_with_capacity(IDENTITY_RECORD_SIZE)
            .map_err(|_| IdentityError::Allocation)?;
        record
            .try_extend_from_slice(IDENTITY_RECORD_MAGIC)
            .map_err(|_| IdentityError::Allocation)?;
        record
            .try_extend_from_slice(&IDENTITY_RECORD_VERSION.to_be_bytes())
            .map_err(|_| IdentityError::Allocation)?;
        record
            .try_extend_from_slice(self.static_secret.as_bytes())
            .map_err(|_| IdentityError::Allocation)?;
        let signing_seed = Zeroizing::new(self.signing_key.to_bytes());
        record
            .try_extend_from_slice(&signing_seed[..])
            .map_err(|_| IdentityError::Allocation)?;
        let checksum = record_checksum(record.as_slice());
        record
            .try_extend_from_slice(&checksum)
            .map_err(|_| IdentityError::Allocation)?;
        Ok(record)
    }

    fn decode_record(record: &SensitiveBytes) -> Result<Self, IdentityError> {
        let bytes = record.as_slice();
        if bytes.len() != IDENTITY_RECORD_SIZE || &bytes[..4] != IDENTITY_RECORD_MAGIC {
            return Err(IdentityError::CorruptRecord);
        }
        let version = u16::from_be_bytes([bytes[4], bytes[5]]);
        if version != IDENTITY_RECORD_VERSION {
            return Err(IdentityError::UnsupportedRecordVersion(version));
        }
        let expected_checksum = record_checksum(&bytes[..IDENTITY_RECORD_PAYLOAD_SIZE]);
        if bytes[IDENTITY_RECORD_PAYLOAD_SIZE..] != expected_checksum {
            return Err(IdentityError::CorruptRecord);
        }
        let mut static_seed = Zeroizing::new([0_u8; 32]);
        let mut signing_seed = Zeroizing::new([0_u8; 32]);
        static_seed.copy_from_slice(&bytes[6..38]);
        signing_seed.copy_from_slice(&bytes[38..70]);
        Ok(Self::from_seeds(&static_seed, &signing_seed))
    }

    #[cfg(feature = "test-support")]
    /// Imports deterministic seed material for isolated tests and protocol vectors.
    pub fn from_seed_material(static_seed: &[u8; 32], signing_seed: &[u8; 32]) -> Self {
        Self::from_seeds(static_seed, signing_seed)
    }
}

fn record_checksum(payload: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(IDENTITY_RECORD_CHECKSUM_DOMAIN);
    hash.update(payload);
    hash.finalize().into()
}

/// Errors returned while loading, generating, or persisting device keys.
#[derive(Debug, Error)]
pub enum IdentityError {
    /// The secure store has no record for this device.
    #[error("device identity is missing")]
    Missing,
    /// A key record is already present and was not overwritten.
    #[error("device identity already exists")]
    AlreadyExists,
    /// The stored key record has an invalid size or header.
    #[error("device identity record is corrupt")]
    CorruptRecord,
    /// The stored key record uses a version this implementation cannot read.
    #[error("unsupported device identity record version {0}")]
    UnsupportedRecordVersion(u16),
    /// The operating system random source failed.
    #[error("cryptographic random source failed")]
    Randomness,
    /// Allocation for secret storage failed.
    #[error("unable to allocate protected memory for key material")]
    Allocation,
    /// The secure store failed; its error contains no secret payload.
    #[error(transparent)]
    SecureStore(#[from] SecureStoreError),
}

/// Errors returned by strict Ed25519 signature verification.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SignatureError {
    /// The public key bytes do not encode a valid Ed25519 point.
    #[error("invalid Ed25519 public key")]
    InvalidPublicKey,
    /// The public key is a small-order point and is rejected.
    #[error("weak Ed25519 public key")]
    WeakPublicKey,
    /// The signature is malformed or does not verify strictly.
    #[error("invalid Ed25519 signature")]
    InvalidSignature,
    /// The supplied device identifier is not derived from the signing key.
    #[error("device identifier does not match signing key")]
    DeviceIdMismatch,
    /// The static X25519 public key must not be the all-zero encoding.
    #[error("invalid static X25519 public key")]
    InvalidStaticKey,
}

/// Domain-separated SHA-256 digest of a one-time pairing invitation token.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvitationTokenHash([u8; 32]);

impl InvitationTokenHash {
    /// Hashes a 32-byte invitation token using the pairing ledger domain.
    pub fn from_token(token: &[u8; 32]) -> Self {
        let mut hash = Sha256::new();
        hash.update(INVITATION_TOKEN_HASH_DOMAIN);
        hash.update(token);
        Self(hash.finalize().into())
    }

    /// Returns the fixed-width digest stored by the invitation ledger.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for InvitationTokenHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InvitationTokenHash([redacted])")
    }
}

/// Verifies a signature with weak-key rejection and Ed25519's strict verifier.
pub fn verify_signature(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), SignatureError> {
    let key = verifying_key(public_key)?;
    let signature = Signature::from_bytes(signature);
    key.verify_strict(message, &signature)
        .map_err(|_| SignatureError::InvalidSignature)
}

fn validate_signing_key(public_key: &[u8; 32]) -> Result<(), SignatureError> {
    verifying_key(public_key).map(|_| ())
}

fn validate_static_key(public_key: &[u8; 32]) -> Result<(), SignatureError> {
    let probe_secret = StaticSecret::from([0x5a; 32]);
    let shared = probe_secret.diffie_hellman(&X25519PublicKey::from(*public_key));
    if !shared.was_contributory() {
        return Err(SignatureError::InvalidStaticKey);
    }
    Ok(())
}

fn verifying_key(public_key: &[u8; 32]) -> Result<VerifyingKey, SignatureError> {
    let key = VerifyingKey::from_bytes(public_key).map_err(|_| SignatureError::InvalidPublicKey)?;
    if key.is_weak() {
        return Err(SignatureError::WeakPublicKey);
    }
    Ok(key)
}
