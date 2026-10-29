//! Size bounds shared by the vault, the stores and backups.

/// Largest plaintext of one item, as encoded for encryption.
pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;

/// Largest ciphertext of one item: the largest plaintext, its OpenPGP armor
/// growth and room for many recipients.
pub const MAX_CIPHERTEXT_BYTES: usize = 2 * MAX_CONTENT_BYTES + 256 * 1024;
