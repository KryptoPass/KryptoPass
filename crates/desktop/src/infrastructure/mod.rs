//! Adapters for persistent storage and operating-system cryptography.

pub mod app_paths;
pub mod backup;
pub mod openpgp;
pub mod sqlite_storage;

pub use openpgp::GnuPgOpenPgp;
pub use sqlite_storage::SqliteStorage;

/// Upper bound for the encrypted form of one supported plaintext payload.
pub const MAX_CIPHERTEXT_BYTES: usize = 2 * kryptopass_core::MAX_CONTENT_BYTES + 256 * 1024;
