//! An item's versions: its history and the sides of a conflict.

use super::error::VaultError;
use super::item::Item;
use super::record::{ItemMeta, VersionId, VersionInfo};
use super::{Cipher, Store, Vault, logged};
use crate::domain::SecretId;

impl<S: Store, C: Cipher> Vault<S, C> {
    /// Versions without children, the head among them. More than one means
    /// the item is in conflict; see [`Vault::open_conflict`].
    pub fn item_heads(&self, id: SecretId) -> Result<Vec<VersionInfo>, VaultError> {
        logged("item_heads", self.store.heads(id).map_err(Into::into))
    }

    /// Every stored version, newest first, without decrypting.
    pub fn item_history(&self, id: SecretId) -> Result<Vec<VersionInfo>, VaultError> {
        logged("item_history", self.store.history(id).map_err(Into::into))
    }

    /// The common ancestor two versions diverged from, for a three-way merge.
    pub fn merge_base(
        &self,
        id: SecretId,
        a: VersionId,
        b: VersionId,
    ) -> Result<Option<VersionId>, VaultError> {
        logged(
            "merge_base",
            self.store.merge_base(id, a, b).map_err(Into::into),
        )
    }

    /// Decrypts one version, migrated like [`Vault::open_item`]. Its `meta`
    /// carries that version's state and id as `head`, with the item's
    /// current revision, so an edit made from it is checked against now.
    pub fn open_version(&self, id: SecretId, version: VersionId) -> Result<Item, VaultError> {
        logged("open_version", self.try_open_version(id, version))
    }

    fn try_open_version(&self, id: SecretId, version: VersionId) -> Result<Item, VaultError> {
        let current = self.store.item_meta(id)?.ok_or(VaultError::ItemNotFound)?;
        let stored = self
            .store
            .version(id, version)?
            .ok_or(VaultError::ItemNotFound)?;
        let meta = ItemMeta {
            head: version,
            state: stored.info.state,
            ..current
        };
        let (content, migrated_from) = self.decrypt(&meta, &stored.ciphertext)?;
        tracing::debug!(item = %id.0, version = %version.0, "item version opened");
        Ok(Item {
            meta,
            content,
            migrated_from,
        })
    }
}
