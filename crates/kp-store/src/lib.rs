//! SQLite implementation of the vault's [`Store`] port.
//!
//! Items are stored as version graphs (see `schema`): writes add immutable
//! versions instead of overwriting, so concurrent edits from Sync or another
//! device stay side by side as heads until a merge joins them, and the common
//! ancestor of any two versions is always at hand as a merge base.
//!
//! Only ciphertext, ids and the metadata the list shows are stored; the
//! store never sees plaintext or keys. It is not wired to the app yet.

mod convert;
mod filesystem;
mod graph;
mod open;
mod read;
mod schema;
mod write;

use kryptopass_core::vault::{
    Change, ItemMeta, ItemRecord, ItemVersion, Store, StoreError, VersionId, VersionInfo,
};
use kryptopass_core::{EntryIconAsset, Folder, FolderId, SecretId, SecretLink};
use rusqlite::Connection;

use convert::{SqlResult, StoreResult};

pub use kryptopass_core::util::MAX_CIPHERTEXT_BYTES;

/// The vault store on one SQLite file.
pub struct SqliteStore {
    connection: Connection,
}

impl SqliteStore {
    /// Runs a read in one transaction, so it sees a single consistent state.
    fn read<T>(&self, query: impl FnOnce(&Connection) -> StoreResult<T>) -> StoreResult<T> {
        let transaction = self.connection.unchecked_transaction().sql()?;
        let result = query(&transaction)?;
        transaction.commit().sql()?;
        Ok(result)
    }
}

impl Store for SqliteStore {
    fn folders(&self) -> Result<Vec<Folder>, StoreError> {
        self.read(read::folders)
    }

    fn folder(&self, id: FolderId) -> Result<Option<Folder>, StoreError> {
        self.read(|connection| read::folder(connection, id))
    }

    fn items(&self) -> Result<Vec<ItemMeta>, StoreError> {
        self.read(read::items)
    }

    fn item_meta(&self, id: SecretId) -> Result<Option<ItemMeta>, StoreError> {
        self.read(|connection| read::item_meta(connection, id))
    }

    fn item(&self, id: SecretId) -> Result<Option<ItemRecord>, StoreError> {
        self.read(|connection| read::item(connection, id))
    }

    fn links(&self) -> Result<Vec<SecretLink>, StoreError> {
        self.read(read::links)
    }

    fn link_of(&self, id: SecretId) -> Result<Option<SecretId>, StoreError> {
        self.read(|connection| read::link_of(connection, id))
    }

    fn icon_asset(&self, id: uuid::Uuid) -> Result<Option<EntryIconAsset>, StoreError> {
        self.read(|connection| read::icon_asset(connection, id))
    }

    fn heads(&self, id: SecretId) -> Result<Vec<VersionInfo>, StoreError> {
        self.read(|connection| read::versions(connection, id, read::Versions::Heads))
    }

    fn history(&self, id: SecretId) -> Result<Vec<VersionInfo>, StoreError> {
        self.read(|connection| read::versions(connection, id, read::Versions::All))
    }

    fn version(&self, id: SecretId, version: VersionId) -> Result<Option<ItemVersion>, StoreError> {
        self.read(|connection| read::version(connection, id, version))
    }

    fn merge_base(
        &self,
        id: SecretId,
        a: VersionId,
        b: VersionId,
    ) -> Result<Option<VersionId>, StoreError> {
        self.read(|connection| {
            for version in [a, b] {
                if !write::belongs(connection, id, version)? {
                    return Err(StoreError::Rejected("version is not part of the item"));
                }
            }
            let edges = read::parents_of(connection, id)?;
            Ok(graph::merge_base(&edges, a, b))
        })
    }

    fn commit(&mut self, change: Change) -> Result<(), StoreError> {
        write::commit(&mut self.connection, change)
    }
}
