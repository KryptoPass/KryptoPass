//! Shared domain logic for KryptoPass: folders, secrets and crypto contracts.

pub mod domain;
pub mod keyring;
pub mod navigation;
pub mod ports;
pub mod service;

pub use domain::{
    DomainError, Folder, FolderId, KeyFingerprint, RecipientSet, Secret, SecretId, SecretKind,
};
pub use keyring::{KeyInfo, KeyValidity, Keyring, KeyringError, UserIdInfo};
pub use navigation::NavigationNode;
pub use ports::{OpenPgp, Storage};
pub use service::{CreateSecret, RecipientSelection, SecretsService};
