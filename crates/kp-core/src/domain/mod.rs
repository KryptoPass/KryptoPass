//! Core entities and value objects for the local secret store.

pub mod content;
pub mod entry_icon;
pub mod item;
pub(crate) mod wifi;

use thiserror::Error;
use uuid::Uuid;

/// Domain validation failures independent of any storage or crypto backend.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("invalid OpenPGP fingerprint")]
    InvalidFingerprint,

    #[error("at least one recipient is required")]
    EmptyRecipients,

    #[error("unsupported secret kind")]
    UnsupportedSecretKind,

    #[error("invalid folder icon identifier")]
    InvalidFolderIcon,

    #[error("invalid folder color")]
    InvalidFolderColor,
}

/// Safe, typed failures that the UI can map to a relationship-specific message.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecretLinkError {
    #[error("linked item is unavailable")]
    InvalidTarget,

    #[error("linked item is already paired")]
    Occupied,
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

/// Public relationship between two secrets. Endpoints are stored in UUID
/// order so each pair has one stable representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecretLink {
    pub first: SecretId,
    pub second: SecretId,
}

impl SecretLink {
    /// The link between two different items, endpoints in UUID order so each
    /// pair has one representation. `None` when both are the same item.
    pub fn new(a: SecretId, b: SecretId) -> Option<Self> {
        match a.0.cmp(&b.0) {
            std::cmp::Ordering::Less => Some(Self {
                first: a,
                second: b,
            }),
            std::cmp::Ordering::Greater => Some(Self {
                first: b,
                second: a,
            }),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// The other end of the link, or `None` when `endpoint` is not on it.
    pub fn other(self, endpoint: SecretId) -> Option<SecretId> {
        if endpoint == self.first {
            Some(self.second)
        } else if endpoint == self.second {
            Some(self.first)
        } else {
            None
        }
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

/// A folder's display appearance. The icon is a theme icon name, not a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderAppearance {
    icon: String,
    color: String,
}

impl FolderAppearance {
    /// Creates an appearance from a theme icon name and a color.
    ///
    /// Colors may be `transparent` or six-digit RGB hex values; hex values are
    /// stored in lowercase.
    pub fn new(icon: String, color: String) -> Result<Self, DomainError> {
        if icon.is_empty()
            || icon.len() > 128
            || !icon
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(DomainError::InvalidFolderIcon);
        }

        let color = if color == "transparent" {
            color
        } else if color.len() == 7
            && color.starts_with('#')
            && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            color.to_ascii_lowercase()
        } else {
            return Err(DomainError::InvalidFolderColor);
        };

        Ok(Self { icon, color })
    }

    pub fn icon(&self) -> &str {
        &self.icon
    }

    pub fn color(&self) -> &str {
        &self.color
    }
}

impl Default for FolderAppearance {
    fn default() -> Self {
        Self {
            icon: "folder".to_owned(),
            color: "transparent".to_owned(),
        }
    }
}

/// A logical folder. Folders organize secrets; they are not crypto boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub parent_id: Option<FolderId>,
    /// Persistent order among siblings.
    pub sort_position: i64,
    pub name: String,
    pub appearance: FolderAppearance,
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
    Card,
    Identity,
    Custom,
    Wifi,
    License,
}

impl SecretKind {
    /// Stable storage identifier for this kind. These identifiers are part of
    /// the encrypted payload contract and must not depend on Rust variant names.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::ApiToken => "api-token",
            Self::Totp => "totp",
            Self::SshKey => "ssh-key",
            Self::SecureNote => "note",
            Self::Card => "card",
            Self::Identity => "identity",
            Self::Custom => "custom",
            Self::Wifi => "wifi",
            Self::License => "license",
        }
    }

    /// Parses a stable storage identifier without accepting lossy aliases.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "login" => Ok(Self::Login),
            "api-token" => Ok(Self::ApiToken),
            "totp" => Ok(Self::Totp),
            "ssh-key" => Ok(Self::SshKey),
            "note" => Ok(Self::SecureNote),
            "card" => Ok(Self::Card),
            "identity" => Ok(Self::Identity),
            "custom" => Ok(Self::Custom),
            "wifi" => Ok(Self::Wifi),
            "license" => Ok(Self::License),
            _ => Err(DomainError::UnsupportedSecretKind),
        }
    }
}

/// Public metadata used to list and organize a secret without loading its payload.
///
/// The encrypted content is deliberately absent; callers that need to decrypt
/// or replace a secret must use [`Secret`] instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMetadata {
    pub icon: Option<entry_icon::EntryIcon>,
    pub id: SecretId,
    pub folder_id: Option<FolderId>,
    pub title: String,
    pub kind: SecretKind,
    pub recipients: RecipientSet,
    pub favorite: bool,
    pub trashed: bool,
    /// Monotonic optimistic-concurrency token for metadata and ciphertext.
    pub revision: u64,
}

/// An encrypted secret. Plaintext must not be stored in this entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Secret {
    pub icon: Option<entry_icon::EntryIcon>,
    pub id: SecretId,
    pub folder_id: Option<FolderId>,
    pub title: String,
    pub kind: SecretKind,
    pub recipients: RecipientSet,
    pub ciphertext: Vec<u8>,
    pub favorite: bool,
    pub trashed: bool,
    /// Monotonic optimistic-concurrency token for metadata and ciphertext.
    pub revision: u64,
}
