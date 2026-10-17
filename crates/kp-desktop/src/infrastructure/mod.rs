//! Adapters for operating-system cryptography, browser integration, and UI resources.

pub(crate) mod entry_icons;
pub mod openpgp;

pub use openpgp::GnuPgOpenPgp;

/// Upper bound for the encrypted form of one supported plaintext payload.
pub const MAX_CIPHERTEXT_BYTES: usize = 2 * kryptopass_core::MAX_CONTENT_BYTES + 256 * 1024;

pub mod browser;
