//! Size bounds shared by the vault, the stores and backups.

use crate::domain::content::MAX_CONTENT_BYTES;

/// Largest ciphertext of one item: the largest plaintext, its OpenPGP armor
/// growth and room for many recipients.
pub const MAX_CIPHERTEXT_BYTES: usize = 2 * MAX_CONTENT_BYTES + 256 * 1024;
