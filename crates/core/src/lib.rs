//! Shared domain logic for KryptoPass: folders, secrets and crypto contracts.

pub mod domain;
pub mod import;
pub mod keyring;
pub mod navigation;
pub mod otp;
pub mod password;
pub mod ports;
pub mod service;

pub use domain::content::{
    CONTENT_SCHEMA_VERSION, HistoricalPassword, LoginContent, LoginHistory, LoginHistoryEvent,
    LoginHistoryEventKind, MAX_CONTENT_BYTES, MAX_HISTORY_PASSWORD_BYTES, MAX_LOGIN_HISTORY_EVENTS,
    MAX_PREVIOUS_PASSWORDS, NoteContent, SecretContent, TotpAlgorithm, TotpConfig, decode_content,
    encode_content,
};
pub use domain::entry_icon::{
    EntryAppearanceUpdate, EntryIcon, EntryIconAsset, MAX_ENTRY_ICON_BYTES,
    MAX_ENTRY_ICON_DIMENSION, automatic_entry_icon, favicon_for_url, valid_favicon_host,
};
pub use domain::{
    DomainError, Folder, FolderAppearance, FolderId, KeyFingerprint, RecipientSet, Secret,
    SecretId, SecretKind, SecretMetadata,
};
pub use import::{
    ImportBatch, ImportFolder, ImportFolderPolicy, ImportItem, ImportOptions, ImportReceipt,
    ImportReceiver, ImportValidationError, MAX_IMPORT_RECORDS,
};
pub use keyring::{KeyInfo, KeyValidity, Keyring, KeyringError, UserIdInfo};
pub use navigation::NavigationNode;
pub use otp::{
    MAX_TOTP_LABEL_BYTES, MAX_TOTP_SECRET_BYTES, TotpSnapshot, format_totp_input, generate_totp,
    parse_totp_input,
};
pub use password::{PasswordOptions, generate_password};
pub use ports::{ImportStorage, OpenPgp, Storage};
pub use service::{CreateSecret, RecipientSelection, SaveContentRequest, SecretsService};
