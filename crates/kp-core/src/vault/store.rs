//! Persistence boundary.
//!
//! Reads are plain queries. Every write is one [`Change`] handed to
//! [`Store::commit`], so what must persist together (an item, its icon image,
//! its link) is a single value the store applies atomically, instead of a
//! method per combination.
//!
//! The vault checks every precondition before committing; the store checks
//! them again inside its transaction, because another writer may have changed
//! the data in between. That second check is what makes a revision a guarantee.

use thiserror::Error;

use super::record::{ItemMeta, ItemRecord, ItemVersion, VersionId, VersionInfo};
use crate::EntryIconAsset;
use crate::domain::{Folder, FolderId, SecretId, SecretLink};
use uuid::Uuid;

/// Why the store refused or failed a read or a change.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The stored revision differs from the one the change was based on.
    #[error("stored revision differs from the expected one")]
    Stale,
    #[error("item not found")]
    ItemNotFound,
    #[error("folder not found")]
    FolderNotFound,
    /// The change would break an invariant, such as a folder cycle or an id collision.
    #[error("change rejected by the store: {0}")]
    Rejected(&'static str),
    /// This store does not implement the change.
    #[error("change not supported by the store")]
    Unsupported,
    #[error("storage backend failed")]
    Backend(#[source] anyhow::Error),
}

/// What an item write is based on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// No item with this id may exist yet.
    New,
    /// The stored revision must still be this one.
    Revision(u64),
}

/// Which versions a new local version descends from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Parents {
    /// The current head: an ordinary edit. Other heads, if any, stay in conflict.
    #[default]
    Head,
    /// These versions of the item: a merge. Listing every current head
    /// resolves the conflict.
    Merge(Vec<VersionId>),
}

/// What a write does to the item's link with another item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkChange {
    /// Leave the link as it is.
    #[default]
    Keep,
    /// Pair with this item, or unpair with `None`.
    Set(Option<SecretId>),
}

/// What happens to the contents of a removed folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderContents {
    /// Subfolders and items move to the removed folder's parent; each moved
    /// item gets a new version.
    Reparent,
    /// The whole subtree and its items are deleted, trashed ones included.
    Delete,
}

/// Adds a local version of an item, or creates the item with its first one.
///
/// The record's `meta.head` is the new version's id and its revision is the
/// one the item will have: 0 for a new item, the next one for an update. The
/// store checks `base` and the parents, stores the version and moves the head
/// to it.
#[derive(Clone, Debug)]
pub struct PutItem {
    pub record: ItemRecord,
    pub base: Base,
    /// Ignored for a new item, which has no parents.
    pub parents: Parents,
    /// Seconds since the Unix epoch.
    pub created_at: u64,
    /// Image to store when the icon is a newly chosen picture.
    pub icon_asset: Option<EntryIconAsset>,
    pub link: LinkChange,
}

/// One atomic write. Either all of it persists or none of it does.
///
/// Changes that move or delete items must give every item they touch a new
/// revision, so an editor that opened one sees it as stale.
#[derive(Clone, Debug)]
pub enum Change {
    /// Adds a local version; also how flags, folder and recipients change.
    PutItem(Box<PutItem>),
    /// Stores a version written elsewhere, such as one received through Sync.
    ///
    /// Its parents must already be stored; the item is created if this is its
    /// first version. The head moves to it only when it descends from the
    /// current head; otherwise the item is left in conflict. Storing a version
    /// that is already there does nothing.
    AddVersion {
        version: Box<ItemVersion>,
        icon_asset: Option<EntryIconAsset>,
    },
    /// Deletes an item with all its versions and its link, only at `base`.
    RemoveItem { id: SecretId, base: u64 },
    /// Adds a folder after its siblings; `sort_position` is assigned by the store.
    AddFolder(Folder),
    /// Replaces name, appearance and default recipients. Parent and position
    /// change only through [`Change::MoveFolder`].
    UpdateFolder(Folder),
    /// Moves before `before`, or after the last sibling when it is `None`.
    /// Items do not change: they reference the folder, not its place.
    MoveFolder {
        id: FolderId,
        destination: Option<FolderId>,
        before: Option<FolderId>,
    },
    RemoveFolder {
        id: FolderId,
        contents: FolderContents,
    },
    /// Adds a batch without replacing anything; any collision rejects it all.
    /// Each item gets its record as a first version.
    Append {
        folders: Vec<Folder>,
        items: Vec<ItemRecord>,
        links: Vec<SecretLink>,
    },
}

/// Storage the vault runs on.
pub trait Store {
    /// All folders, siblings in display order.
    fn folders(&self) -> Result<Vec<Folder>, StoreError>;

    fn folder(&self, id: FolderId) -> Result<Option<Folder>, StoreError>;

    /// Metadata of every item, without loading encrypted payloads.
    fn items(&self) -> Result<Vec<ItemMeta>, StoreError>;

    /// Metadata of one item, without loading its encrypted payload.
    fn item_meta(&self, id: SecretId) -> Result<Option<ItemMeta>, StoreError>;

    fn item(&self, id: SecretId) -> Result<Option<ItemRecord>, StoreError>;

    fn links(&self) -> Result<Vec<SecretLink>, StoreError>;

    /// A stored icon image, validated; `None` when no version uses it.
    fn icon_asset(&self, id: Uuid) -> Result<Option<EntryIconAsset>, StoreError>;

    /// The item paired with `id`, if any.
    fn link_of(&self, id: SecretId) -> Result<Option<SecretId>, StoreError>;

    /// Versions without children, the head among them. More than one means
    /// the item is in conflict.
    fn heads(&self, id: SecretId) -> Result<Vec<VersionInfo>, StoreError>;

    /// Every stored version of the item, newest first by local storage order.
    fn history(&self, id: SecretId) -> Result<Vec<VersionInfo>, StoreError>;

    fn version(&self, id: SecretId, version: VersionId) -> Result<Option<ItemVersion>, StoreError>;

    /// The best common ancestor of two versions, the base of a three-way
    /// merge: one that is not an ancestor of another common one. Among
    /// several, the greatest id, so every device picks the same. `None` when
    /// they share no stored history.
    fn merge_base(
        &self,
        id: SecretId,
        a: VersionId,
        b: VersionId,
    ) -> Result<Option<VersionId>, StoreError>;

    fn commit(&mut self, change: Change) -> Result<(), StoreError>;
}
