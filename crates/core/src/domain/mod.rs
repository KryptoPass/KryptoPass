//! Core entities and value objects for the local secret store.

use thiserror::Error;
use uuid::Uuid;

/// Domain validation failures independent of any storage or crypto backend.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("invalid OpenPGP fingerprint")]
    InvalidFingerprint,

    #[error("at least one recipient is required")]
    EmptyRecipients,
}

/// Stable identifier for a folder in the local secret store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FolderId(pub Uuid);

impl FolderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for FolderId {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable identifier for a secret in the local secret store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecretId(pub Uuid);

impl SecretId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SecretId {
    fn default() -> Self {
        Self::new()
    }
}

/// Canonical OpenPGP fingerprint used as a recipient reference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyFingerprint(String);

impl KeyFingerprint {
    /// Parses a 160-bit or 256-bit hexadecimal OpenPGP fingerprint.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let value = value.trim();

        if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(DomainError::InvalidFingerprint);
        }

        Ok(Self(value.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Explicit OpenPGP recipients for one encrypted secret or folder default.
///
/// Multiple recipients mean alternative access: any corresponding private key
/// can decrypt the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientSet(Vec<KeyFingerprint>);

impl RecipientSet {
    pub fn new(mut fingerprints: Vec<KeyFingerprint>) -> Result<Self, DomainError> {
        fingerprints.sort_unstable();
        fingerprints.dedup();

        if fingerprints.is_empty() {
            return Err(DomainError::EmptyRecipients);
        }

        Ok(Self(fingerprints))
    }

    pub fn as_slice(&self) -> &[KeyFingerprint] {
        &self.0
    }
}

/// A logical folder. Folders organize secrets; they are not crypto boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub parent_id: Option<FolderId>,
    pub name: String,
    /// Copied into a new secret at creation time; does not re-encrypt existing secrets.
    pub default_recipients: Option<RecipientSet>,
}

/// Supported first-generation secret projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Login,
    ApiToken,
    Totp,
    SshKey,
    SecureNote,
}

/// An encrypted secret. Plaintext must not be stored in this entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Secret {
    pub id: SecretId,
    pub folder_id: Option<FolderId>,
    pub title: String,
    pub kind: SecretKind,
    pub recipients: RecipientSet,
    pub ciphertext: Vec<u8>,
}
