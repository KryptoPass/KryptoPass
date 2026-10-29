//! Failures of vault operations.
//!
//! Variants are the cases a caller tells apart (a stale revision is retried
//! after a reload, an unavailable key asks to unlock). The `Display` text is
//! for logs and never carries a stored value; the UI shows [`VaultError::code`]
//! translated into the user's language.

use thiserror::Error;

use super::cipher::CipherError;
use super::store::StoreError;
use crate::domain::SecretLinkError;
use crate::domain::item::{CodecError, ItemIssues, MigrationError};

/// Why a vault operation failed.
#[derive(Debug, Error)]
pub enum VaultError {
    #[error("item does not exist")]
    ItemNotFound,
    #[error("folder does not exist")]
    FolderNotFound,
    /// The item changed after it was opened; reload before saving again.
    #[error("item changed since it was opened")]
    StaleRevision,
    #[error("item revision counter is exhausted")]
    RevisionExhausted,
    #[error("unknown item template")]
    UnknownTemplate,
    /// Content is not at the template's version and no migration leads there.
    #[error("item template version is not supported")]
    TemplateVersion,
    /// Written by a newer template version, by a more recent app; update to
    /// read it. Such an item is never overwritten.
    #[error("item was written by a newer version of the app")]
    TemplateTooNew,
    /// A template's migrations produced content that does not fit it.
    #[error("item content could not be migrated")]
    MigrationFailed,
    #[error("an item cannot change its template")]
    TemplateChanged,
    #[error("title is empty, too long or has control characters")]
    InvalidTitle,
    #[error("folder name is empty, too long or has control characters")]
    InvalidFolderName,
    #[error("a folder cannot move into itself or its subfolders")]
    FolderCycle,
    #[error("the position anchor is not a sibling at the destination")]
    InvalidPosition,
    #[error("invalid icon")]
    InvalidIcon,
    #[error("no recipients selected and the folder has no default")]
    NoRecipients,
    #[error("content is too large")]
    ContentTooLarge,
    /// Decrypted content is unreadable or does not belong to its record.
    #[error("stored content is corrupt")]
    CorruptContent,
    /// A conflicting difference was left without a choice.
    #[error("a conflict was left unresolved")]
    ConflictUnresolved,
    #[error(transparent)]
    Invalid(ItemIssues),
    #[error(transparent)]
    Link(#[from] SecretLinkError),
    #[error("encryption failed")]
    Cipher(#[from] CipherError),
    #[error("storage failed")]
    Store(#[source] StoreError),
}

impl VaultError {
    /// Stable, language-neutral code for the UI to translate.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ItemNotFound => "item-not-found",
            Self::FolderNotFound => "folder-not-found",
            Self::StaleRevision => "stale-revision",
            Self::RevisionExhausted => "revision-exhausted",
            Self::UnknownTemplate => "unknown-template",
            Self::TemplateVersion => "template-version",
            Self::TemplateTooNew => "template-too-new",
            Self::MigrationFailed => "migration-failed",
            Self::TemplateChanged => "template-changed",
            Self::InvalidTitle => "invalid-title",
            Self::InvalidFolderName => "invalid-folder-name",
            Self::FolderCycle => "folder-cycle",
            Self::InvalidPosition => "invalid-position",
            Self::InvalidIcon => "invalid-icon",
            Self::NoRecipients => "no-recipients",
            Self::ContentTooLarge => "content-too-large",
            Self::CorruptContent => "corrupt-content",
            Self::ConflictUnresolved => "conflict-unresolved",
            Self::Invalid(_) => "invalid-content",
            Self::Link(SecretLinkError::InvalidTarget) => "link-invalid-target",
            Self::Link(SecretLinkError::Occupied) => "link-occupied",
            Self::Cipher(CipherError::UnusableRecipient(_)) => "recipient-unusable",
            Self::Cipher(CipherError::UntrustedRecipient(_)) => "recipient-untrusted",
            Self::Cipher(CipherError::Canceled) => "canceled",
            Self::Cipher(CipherError::KeyUnavailable) => "key-unavailable",
            Self::Cipher(CipherError::Backend(_)) => "cipher-failed",
            Self::Store(_) => "storage-failed",
        }
    }

    /// Whether this is a refusal the user can act on, as opposed to a fault
    /// in the system or the data. Decides the log level.
    pub fn is_refusal(&self) -> bool {
        !matches!(
            self,
            Self::CorruptContent
                | Self::TemplateVersion
                | Self::MigrationFailed
                | Self::RevisionExhausted
                | Self::Cipher(CipherError::Backend(_))
                | Self::Store(_)
        )
    }
}

/// A storage conflict is a domain case, not a generic storage failure.
impl From<StoreError> for VaultError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Stale => Self::StaleRevision,
            StoreError::ItemNotFound => Self::ItemNotFound,
            StoreError::FolderNotFound => Self::FolderNotFound,
            other => Self::Store(other),
        }
    }
}

/// Invalid input stays invalid input; an unreadable payload is corruption.
impl From<CodecError> for VaultError {
    fn from(error: CodecError) -> Self {
        match error {
            CodecError::Invalid(issues) => Self::Invalid(issues),
            CodecError::TooLarge => Self::ContentTooLarge,
            CodecError::UnsupportedVersion | CodecError::Malformed { .. } => Self::CorruptContent,
        }
    }
}

/// Content newer than the app is the user's to resolve by updating; anything
/// else is a fault in the stored data or the template.
impl From<MigrationError> for VaultError {
    fn from(error: MigrationError) -> Self {
        match error {
            MigrationError::TooNew => Self::TemplateTooNew,
            MigrationError::NoPath { .. } => Self::TemplateVersion,
            MigrationError::WrongTemplate => Self::CorruptContent,
            MigrationError::Conflict(_) | MigrationError::Inconsistent(_) => Self::MigrationFailed,
        }
    }
}
