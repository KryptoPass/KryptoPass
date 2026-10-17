//! Versioned, fully encrypted store backups and isolated restore publication.

mod export;
mod filesystem;
mod format;
mod recipients;
mod restore;

pub use export::export_backup;
pub use recipients::parse_selected_recipients;
pub use restore::{RestoreFailure, restore_backup};

pub(super) const MAX_BACKUP_PLAINTEXT_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_BACKUP_CIPHERTEXT_BYTES: usize = 128 * 1024 * 1024 + 256 * 1024;
pub(super) const MAX_BACKUP_ROWS: usize = 10_000;
pub(super) const MAX_BACKUP_RECIPIENTS: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackupPublishOutcome {
    Durable,
    PublishedButDurabilityUnconfirmed,
}

#[cfg(test)]
use kryptopass_core::{
    EntryIcon, EntryIconAsset, Folder, FolderAppearance, FolderId, KeyFingerprint, RecipientSet,
    Secret, SecretId, SecretKind, SecretLink,
};
#[cfg(test)]
use std::{fs, path::PathBuf};
#[cfg(test)]
use uuid::Uuid;

#[cfg(test)]
use crate::infrastructure::GnuPgOpenPgp;
#[cfg(test)]
use crate::storage::SqliteStorage;

#[cfg(test)]
use format::{
    base64_decode, deserialize_backup, deserialize_backup_with_icons,
    deserialize_backup_with_links, serialize_backup, serialize_backup_with_icons,
    serialize_backup_with_links,
};

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/backup_order.rs"]
mod backup_order_tests;

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/backup_links.rs"]
mod backup_links_tests;

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/entry_icon.rs"]
mod entry_icon_tests;
