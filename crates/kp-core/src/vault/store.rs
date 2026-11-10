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
use super::settings::KeySettings;
use super::state::{OpenedFolder, OpenedState};
use crate::EntryIconAsset;
use crate::domain::{Folder, FolderId, RecipientSet, SecretId, SecretLink};
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
    /// Subfolders move to the removed folder's parent. The folder must hold
    /// no items: the vault moves them first, since each move is a new
    /// version with its state encrypted.
    Reparent,
    /// The whole subtree and its items are deleted for good, trashed ones
    /// included; each item leaves a deletion mark.
    Delete,
}

/// Adds a local version of an item, or creates the item with its first one.
///
/// The record's `meta.head` is the new version's id and its revision is the
/// one the item will have: 0 for a new item, the next one for an update. The
/// store checks `base` and the parents, signs and stores the version and
/// moves the head to it.
#[derive(Clone, Debug)]
pub struct PutItem {
    /// The head's state and its content encrypted to `meta.state.recipients`.
    pub record: ItemRecord,
    /// The head's state (see `seal_state`) encrypted to the same recipients.
    pub sealed_state: Vec<u8>,
    pub base: Base,
    /// Ignored for a new item, which has no parents.
    pub parents: Parents,
    /// Wall-clock milliseconds since the Unix epoch. The store derives the
    /// version's clock from it and the parents' clocks, inside the change.
    pub now_millis: u64,
    /// Image to store when the icon is a newly chosen picture.
    pub icon_asset: Option<EntryIconAsset>,
    pub link: LinkChange,
}

/// A new item in a [`Change::Append`].
#[derive(Clone, Debug)]
pub struct AppendItem {
    /// Encrypted reconstructed previous contents, oldest first.
    pub previous: Vec<(VersionId, Vec<u8>)>,
    pub record: ItemRecord,
    pub sealed_state: Vec<u8>,
    /// When the item was last written, in milliseconds since the Unix epoch:
    /// at its source for an import. The first version's clock starts there.
    pub written_millis: u64,
}

/// One atomic write. Either all of it persists or none of it does.
///
/// Changes that move or delete items must give every item they touch a new
/// revision, so an editor that opened one sees it as stale.
#[derive(Clone, Debug)]
pub enum Change {
    /// Adds a local version; also how flags, folder and recipients change.
    PutItem(Box<PutItem>),
    /// Deletes an item for good, only at `base`: every version, blob and its
    /// link go, and only a signed deletion mark stays so Sync cannot bring
    /// it back.
    RemoveItem { id: SecretId, base: u64 },
    /// Adds a folder after its siblings, with its first version;
    /// `sort_position` is assigned by the store.
    AddFolder {
        folder: Folder,
        sealed: SealedFolder,
    },
    /// Replaces name, appearance and default recipients with a new version
    /// that joins every current one. Parent and position change only through
    /// [`Change::MoveFolder`].
    UpdateFolder {
        folder: Folder,
        sealed: SealedFolder,
    },
    /// Moves before `before`, or after the last sibling when it is `None`.
    /// A new parent is a new version, `sealed`; the order among siblings is
    /// this device's own. Items do not change: they reference the folder.
    MoveFolder {
        id: FolderId,
        destination: Option<FolderId>,
        before: Option<FolderId>,
        sealed: Option<SealedFolder>,
    },
    RemoveFolder {
        id: FolderId,
        contents: FolderContents,
    },
    /// Adds a batch without replacing anything; any collision rejects it all.
    /// Each item gets its record as a first version. Folders come parents
    /// first.
    Append {
        folders: Vec<(Folder, SealedFolder)>,
        items: Vec<AppendItem>,
        links: Vec<SecretLink>,
    },
    /// Replaces the recipients of new items outside a folder with defaults;
    /// `None` clears them.
    SetDefaultRecipients(Option<RecipientSet>),
    /// Indexes versions this device opened: their state becomes listable.
    /// Versions already indexed or no longer stored are skipped.
    Index(Vec<IndexedVersion>),
    /// Indexes folder versions this device opened, as `Index` does for items.
    IndexFolders(Vec<IndexedFolder>),
    /// Replaces one key's settings. Settings equal to the defaults are not
    /// kept, so forgetting a key leaves nothing behind.
    SetKeySettings(KeySettings),
}

/// A folder's state encrypted for a new version, and who it is encrypted to.
#[derive(Clone, Debug)]
pub struct SealedFolder {
    pub state: Vec<u8>,
    pub recipients: RecipientSet,
}

/// The record a stored version belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealedRecord {
    Item(SecretId),
    Folder(FolderId),
}

/// A stored version not indexed yet, with its encrypted state.
#[derive(Clone, Debug)]
pub struct SealedVersion {
    pub id: VersionId,
    pub record: SealedRecord,
    pub recipients: RecipientSet,
    pub state: Vec<u8>,
}

/// The opened state of a folder version, to index.
#[derive(Clone, Debug)]
pub struct IndexedFolder {
    pub id: VersionId,
    pub folder: FolderId,
    pub state: OpenedFolder,
}

/// The opened state of a version, to index.
#[derive(Clone, Debug)]
pub struct IndexedVersion {
    pub id: VersionId,
    pub item: SecretId,
    pub state: OpenedState,
}

/// An item stored here whose current versions this device cannot open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueItem {
    pub id: SecretId,
    /// Recipients of its newest head.
    pub recipients: RecipientSet,
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

    /// Recipients of new items that neither name theirs nor go into a folder
    /// with defaults.
    fn default_recipients(&self) -> Result<Option<RecipientSet>, StoreError>;

    /// Settings of every key that has any besides the defaults.
    fn key_settings(&self) -> Result<Vec<KeySettings>, StoreError>;

    /// Item and folder versions stored but not indexed, such as ones
    /// received through Sync; the vault opens those it can and indexes them.
    fn unindexed(&self) -> Result<Vec<SealedVersion>, StoreError>;

    /// Items with no current version this device can open.
    fn opaque_items(&self) -> Result<Vec<OpaqueItem>, StoreError>;

    fn commit(&mut self, change: Change) -> Result<(), StoreError>;
}
