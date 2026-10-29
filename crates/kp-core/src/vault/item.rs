//! Inputs and outputs of item operations.

use super::record::ItemMeta;
use super::store::LinkChange;
use crate::EntryIconAsset;
use crate::domain::item::ItemContent;
use crate::domain::{FolderId, RecipientSet, SecretId};

/// Who can decrypt a new item.
#[derive(Clone, Debug)]
pub enum Recipients {
    Explicit(RecipientSet),
    /// The default recipients of the destination folder.
    FolderDefault,
}

/// The icon a person picks for an item.
#[derive(Clone, Debug)]
pub enum IconChoice {
    /// Derived from the content, such as the favicon of the first website.
    Automatic,
    /// A freedesktop theme icon name.
    Theme(String),
    /// A picture, stored as a normalized PNG with the item.
    Image(EntryIconAsset),
}

/// An item to create.
pub struct NewItem {
    pub folder: Option<FolderId>,
    pub title: String,
    pub content: ItemContent,
    pub recipients: Recipients,
    pub icon: IconChoice,
    /// Item to pair with.
    pub link: Option<SecretId>,
}

/// A decrypted item, for the detail view or the editor.
pub struct Item {
    pub meta: ItemMeta,
    /// At the template's current version, migrated in memory if needed.
    pub content: ItemContent,
    /// The version the stored content had when it was migrated on opening.
    /// Storage still holds that version until the item is saved.
    pub migrated_from: Option<u32>,
}

/// Changes to an existing item, based on the revision that was opened.
///
/// Only what is set changes. Without new content the payload is not
/// decrypted, so flags, title and folder change without touching the key.
#[must_use]
pub struct ItemEdit {
    pub(super) id: SecretId,
    pub(super) base: u64,
    pub(super) title: Option<String>,
    pub(super) folder: Option<Option<FolderId>>,
    pub(super) content: Option<ItemContent>,
    pub(super) icon: Option<IconChoice>,
    pub(super) link: LinkChange,
    pub(super) favorite: Option<bool>,
    pub(super) trashed: Option<bool>,
    pub(super) resolve: bool,
}

impl ItemEdit {
    /// Whether the edit changes nothing, so saving it would only bump the revision.
    pub(super) fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.folder.is_none()
            && self.content.is_none()
            && self.icon.is_none()
            && self.link == LinkChange::Keep
            && self.favorite.is_none()
            && self.trashed.is_none()
            && !self.resolve
    }

    /// An edit of `id` as it was at revision `base`.
    pub fn new(id: SecretId, base: u64) -> Self {
        Self {
            id,
            base,
            title: None,
            folder: None,
            content: None,
            icon: None,
            link: LinkChange::Keep,
            favorite: None,
            trashed: None,
            resolve: false,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Moves the item; `None` is the root.
    pub fn folder(mut self, folder: Option<FolderId>) -> Self {
        self.folder = Some(folder);
        self
    }

    /// New content. It must come from the same template as the item.
    pub fn content(mut self, content: ItemContent) -> Self {
        self.content = Some(content);
        self
    }

    pub fn icon(mut self, icon: IconChoice) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Pairs with `target`, or unpairs with `None`.
    pub fn link(mut self, target: Option<SecretId>) -> Self {
        self.link = LinkChange::Set(target);
        self
    }

    pub fn favorite(mut self, favorite: bool) -> Self {
        self.favorite = Some(favorite);
        self
    }

    pub fn trashed(mut self, trashed: bool) -> Self {
        self.trashed = Some(trashed);
        self
    }

    /// Saves the result as the merge of every diverging version, which ends
    /// the conflict. The edit should carry the content the person settled on.
    pub fn resolve_conflict(mut self) -> Self {
        self.resolve = true;
        self
    }
}
