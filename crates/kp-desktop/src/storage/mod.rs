//! SQLite persistence for the desktop vault.
//!
//! `connection` opens stores; `schema` owns the store layout; `repository` implements
//! the core storage port. `records`, `recipients`, `folders`, `links`, and `icons`
//! own their SQL, while `snapshots` handles backup/import transactions and
//! `validation` enforces their bounds. `paths` selects vault locations and
//! `filesystem` protects store files and directories.

mod connection;
mod filesystem;
mod folders;
mod icons;
mod links;
pub(crate) mod paths;
mod primary_key;
mod recipients;
mod records;
mod repository;
mod schema;
mod snapshots;
mod validation;

pub(crate) use icons::read_icon_asset;
pub(crate) use primary_key::PrimaryKey;
use rusqlite::Connection;
pub(crate) use snapshots::EncryptedStoreSnapshot;

/// Persistent storage whose secret payload column contains ciphertext only.
pub struct SqliteStorage {
    connection: Connection,
}

#[cfg(test)]
use kryptopass_core::{
    EntryIcon, EntryIconAsset, Folder, FolderAppearance, FolderId, KeyFingerprint, RecipientSet,
    Secret, SecretId, SecretKind, SecretLink, SecretMetadata, Storage,
};
#[cfg(test)]
use rusqlite::params;
#[cfg(test)]
use schema::initialize_schema;
#[cfg(test)]
use std::fs;
#[cfg(test)]
use uuid::Uuid;

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/entry_links.rs"]
mod entry_links_tests;
#[cfg(test)]
#[path = "../../tests/unit/infrastructure/folder_move.rs"]
mod folder_move_tests;
