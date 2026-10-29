//! Shared domain logic for KryptoPass: template-driven items, folders,
//! recipients and the vault use cases over store and cipher ports.

pub mod domain;
pub mod otp;
pub mod search;
pub mod util;
pub mod vault;

pub use domain::entry_icon::{
    EntryIcon, EntryIconAsset, MAX_ENTRY_ICON_BYTES, MAX_ENTRY_ICON_DIMENSION,
    automatic_icon_for_url, favicon_for_url, png_sha256, valid_favicon_host,
    validate_normalized_png,
};
pub use domain::item::{
    ActionError, CodecError, CustomValue, DiffKind, DiffTarget, Difference, FieldValue,
    FormatCheck, FormatRules, ITEM_SCHEMA_VERSION, IssueCode, IssueTarget, ItemContent, ItemIssue,
    ItemIssues, ItemValue, MigrationError, Side, decode_item, encode_item, encode_item_with,
};
pub use domain::{
    DomainError, Folder, FolderAppearance, FolderId, KeyFingerprint, RecipientSet, SecretId,
    SecretLink, SecretLinkError,
};
pub use otp::{
    MAX_TOTP_EXTENSION_BYTES, MAX_TOTP_EXTENSIONS, MAX_TOTP_LABEL_BYTES, MAX_TOTP_SECRET_BYTES,
    MAX_TOTP_URI_BYTES, TotpSnapshot, format_totp_input, generate_totp, parse_totp_input,
};
pub use util::MAX_CONTENT_BYTES;

pub use kryptopass_secure_memory::{SensitiveBytes, SensitiveText};
