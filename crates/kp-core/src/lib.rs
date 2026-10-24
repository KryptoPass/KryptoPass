//! Shared domain logic for KryptoPass: folders, secrets and crypto contracts.

pub mod domain;
pub mod import;
pub mod navigation;
pub mod otp;
pub mod password;
pub mod ports;
pub mod search;
pub mod service;
pub mod vault_object;

pub use domain::content::{
    CONTENT_SCHEMA_VERSION, CardContent, CustomContent, CustomRecord, IdentityContent,
    IdentityDocument, LicenseContent, LoginContent, LoginHistory, LoginHistoryEvent,
    LoginHistoryEventKind, MAX_CONTENT_BYTES, MAX_HISTORY_PASSWORD_BYTES, MAX_LOGIN_HISTORY_EVENTS,
    NoteContent, SecretContent, SshKeyContent, TotpAlgorithm, TotpConfig, TotpContent,
    TotpUriParameter, WifiContent, WifiSecurity, decode_content, encode_content, wifi_qr_payload,
};
pub use domain::entry_icon::{
    EntryAppearanceUpdate, EntryIcon, EntryIconAsset, MAX_ENTRY_ICON_BYTES,
    MAX_ENTRY_ICON_DIMENSION, automatic_entry_icon, favicon_for_url, valid_favicon_host,
};
pub use domain::{
    DomainError, Folder, FolderAppearance, FolderId, KeyFingerprint, RecipientSet, Secret,
    SecretId, SecretKind, SecretLink, SecretLinkError, SecretMetadata,
};
pub use import::{
    ImportBatch, ImportFolder, ImportFolderPolicy, ImportItem, ImportOptions, ImportReceipt,
    ImportReceiver, ImportValidationError, MAX_IMPORT_RECORDS,
};
pub use navigation::NavigationNode;
pub use otp::{
    MAX_TOTP_EXTENSION_BYTES, MAX_TOTP_EXTENSIONS, MAX_TOTP_LABEL_BYTES, MAX_TOTP_SECRET_BYTES,
    MAX_TOTP_URI_BYTES, TotpSnapshot, format_totp_input, generate_totp, parse_totp_input,
};
pub use password::{PasswordOptions, generate_password};
pub use ports::{ImportStorage, OpenPgp, Storage};
pub use service::{CreateSecret, RecipientSelection, SaveContentRequest, SecretsService};
pub use vault_object::{
    MAX_VAULT_OBJECT_BYTES, VAULT_OBJECT_FORMAT, VaultObject, VaultObjectError,
    decode_vault_object, encode_vault_object,
};

pub use kryptopass_secure_memory::{SensitiveBytes, SensitiveText};
