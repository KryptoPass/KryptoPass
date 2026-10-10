//! Shared domain logic for KryptoPass: folders, secrets and crypto contracts.

pub mod domain;
pub mod keyring;
pub mod navigation;
pub mod password;
pub mod ports;
pub mod service;

pub use domain::content::{
    CONTENT_SCHEMA_VERSION, LoginContent, MAX_CONTENT_BYTES, NoteContent, SecretContent,
    decode_content, encode_content,
};
pub use domain::{
    DomainError, Folder, FolderAppearance, FolderId, KeyFingerprint, RecipientSet, Secret,
    SecretId, SecretKind, SecretMetadata,
};
pub use keyring::{KeyInfo, KeyValidity, Keyring, KeyringError, UserIdInfo};
pub use navigation::NavigationNode;
pub use password::{PasswordOptions, generate_password};
pub use ports::{OpenPgp, Storage};
pub use service::{CreateSecret, RecipientSelection, SecretsService};
