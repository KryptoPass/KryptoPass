//! Fully encrypted store backups and isolated restore publication.
//!
//! A backup is a compacted copy of the vault file, marked with its format and
//! encrypted as one OpenPGP message. The copy holds the whole version graph,
//! so a restored vault keeps its history and can keep syncing.

mod export;
mod filesystem;
mod recipients;
mod restore;

pub use export::export_backup;
pub use recipients::parse_selected_recipients;
pub use restore::{RestoreFailure, restore_backup};

pub(super) const MAX_BACKUP_PLAINTEXT_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_BACKUP_CIPHERTEXT_BYTES: usize = 128 * 1024 * 1024 + 256 * 1024;
/// Starts the plaintext of a backup. Older backups, in the format of the
/// previous store, do not have it and are refused as invalid.
pub(super) const BACKUP_MAGIC: &[u8] = b"KryptoPass backup 2\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackupPublishOutcome {
    Durable,
    PublishedButDurabilityUnconfirmed,
}
