//! What the vault keeps for an item, as the store sees it.
//!
//! An item is a graph of immutable versions, like commits: every write adds a
//! version whose parents are the versions it was based on, and nothing is
//! rewritten in place. The item's head is the version shown. More than one
//! version without children means two writers diverged (two devices, or a
//! device and Sync); the item is in conflict until a version with all of them
//! as parents merges them. The common ancestor of two versions is the base of
//! that merge.

use kp_template::TemplateId;
use uuid::Uuid;

use crate::domain::clock::Hlc;
use crate::domain::entry_icon::EntryIcon;
use crate::domain::{FolderId, RecipientSet, SecretId};

/// Identifier of one immutable item version.
///
/// Ordered, so a choice between versions (such as a merge tiebreak) is the
/// same on every device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VersionId(pub Uuid);

impl VersionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for VersionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Who wrote a version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Author {
    /// This device.
    Local,
    /// Another device, by its identifier in Sync.
    Device(String),
}

/// Everything a version records besides its encrypted content.
///
/// Title and flags are part of the version, not of the item, so they have a
/// history and a merge base like the content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemState {
    pub folder: Option<FolderId>,
    pub title: String,
    /// Template that produced the content; fixed for the life of the item.
    pub template: TemplateId,
    pub recipients: RecipientSet,
    pub icon: Option<EntryIcon>,
    pub favorite: bool,
    pub trashed: bool,
}

/// An item as listed: its head state, without the encrypted content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemMeta {
    pub id: SecretId,
    /// Version shown and edited.
    pub head: VersionId,
    /// Local optimistic-concurrency token: every change to the item, its head
    /// or its heads bumps it, so an open editor knows it is stale.
    pub revision: u64,
    /// Whether other versions diverge from the head and wait for a merge.
    pub conflicted: bool,
    pub state: ItemState,
}

/// An item's head as persisted: metadata plus the encrypted payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemRecord {
    pub meta: ItemMeta,
    pub ciphertext: Vec<u8>,
}

/// One version without its payload, for history and merge planning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionInfo {
    pub id: VersionId,
    pub item: SecretId,
    /// Versions this one was based on: none for the first, two or more for a merge.
    pub parents: Vec<VersionId>,
    pub state: ItemState,
    pub author: Author,
    /// When it was written; later than every parent's clock.
    pub clock: Hlc,
}

/// One version with its encrypted payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemVersion {
    pub info: VersionInfo,
    pub ciphertext: Vec<u8>,
}
