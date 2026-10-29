//! Creating, reading, editing and deleting items.

use std::sync::Arc;

use super::error::VaultError;
use super::icon;
use super::item::{Item, ItemEdit, NewItem, Recipients};
use super::record::{ItemMeta, ItemRecord, ItemState, VersionId};
use super::store::{Base, Change, LinkChange, Parents, PutItem};
use super::{Cipher, Store, Vault, logged, next_revision};
use crate::EntryIconAsset;
use crate::domain::item::{ItemContent, decode_item, encode_item_with};
use crate::domain::{Folder, FolderId, RecipientSet, SecretId, SecretLink, SecretLinkError};
use crate::util::{clean_line, unix_now};

const MAX_TITLE_BYTES: usize = 512;

impl<S: Store, C: Cipher> Vault<S, C> {
    /// Metadata of every item, for lists and search, without decrypting.
    pub fn list_items(&self) -> Result<Vec<ItemMeta>, VaultError> {
        logged("list_items", self.store.items().map_err(Into::into))
    }

    /// Decrypts an item for viewing or editing, migrated to the current
    /// template version in memory.
    ///
    /// Stored content is not validated against the current format rules, so a
    /// rule made stricter later never locks a person out of their data; the
    /// editor validates when saving.
    pub fn open_item(&self, id: SecretId) -> Result<Item, VaultError> {
        logged("open_item", self.try_open_item(id))
    }

    /// Creates an item and returns its metadata at revision 0.
    pub fn create_item(&mut self, new: NewItem) -> Result<ItemMeta, VaultError> {
        logged("create_item", self.try_create_item(new))
    }

    /// Applies an edit and returns the metadata at the new revision.
    pub fn update_item(&mut self, edit: ItemEdit) -> Result<ItemMeta, VaultError> {
        logged("update_item", self.try_update_item(edit))
    }

    /// Every pair of linked items.
    pub fn list_links(&self) -> Result<Vec<SecretLink>, VaultError> {
        logged("list_links", self.store.links().map_err(Into::into))
    }

    /// The image of a picture icon, for display.
    pub fn icon_asset(&self, id: uuid::Uuid) -> Result<Option<EntryIconAsset>, VaultError> {
        logged("icon_asset", self.store.icon_asset(id).map_err(Into::into))
    }

    /// Deletes an item for good, only if it is still at revision `base`.
    pub fn remove_item(&mut self, id: SecretId, base: u64) -> Result<(), VaultError> {
        logged("remove_item", self.try_remove_item(id, base))
    }

    fn try_open_item(&self, id: SecretId) -> Result<Item, VaultError> {
        let ItemRecord { meta, ciphertext } = self.record(id)?;
        let (content, migrated_from) = self.decrypt(&meta, &ciphertext)?;
        tracing::debug!(item = %id.0, revision = meta.revision, "item opened");
        Ok(Item {
            meta,
            content,
            migrated_from,
        })
    }

    fn try_create_item(&mut self, new: NewItem) -> Result<ItemMeta, VaultError> {
        let NewItem {
            folder,
            title,
            mut content,
            recipients,
            icon: icon_choice,
            link,
        } = new;
        let templates = Arc::clone(&self.templates);
        let template = templates.current(&content)?;
        let title = clean_line(&title, MAX_TITLE_BYTES).ok_or(VaultError::InvalidTitle)?;
        content.normalize(template);
        let plaintext = encode_item_with(&content, template, &self.rules)?;

        let folder_record = folder.map(|id| self.require_folder(id)).transpose()?;
        let recipients = resolve_recipients(recipients, folder_record.as_ref())?;
        self.cipher.check_recipients(&recipients)?;

        let id = SecretId::new();
        if link.is_some() {
            self.check_link(id, false, link)?;
        }
        let (icon, icon_asset) = icon::resolve(Some(icon_choice), None, true, || {
            Ok(icon::automatic(&content, template))
        })?;
        let ciphertext = self.cipher.encrypt(plaintext.as_slice(), &recipients)?;

        let meta = ItemMeta {
            id,
            head: VersionId::new(),
            revision: 0,
            conflicted: false,
            state: ItemState {
                folder,
                title,
                template: template.id.clone(),
                recipients,
                icon,
                favorite: false,
                trashed: false,
            },
        };
        self.store.commit(Change::PutItem(Box::new(PutItem {
            record: ItemRecord {
                meta: meta.clone(),
                ciphertext,
            },
            base: Base::New,
            parents: Parents::Head,
            created_at: unix_now(),
            icon_asset,
            link: link.map_or(LinkChange::Keep, |target| LinkChange::Set(Some(target))),
        })))?;
        tracing::debug!(
            item = %id.0,
            template = meta.state.template.as_str(),
            "item created"
        );
        Ok(meta)
    }

    fn try_update_item(&mut self, edit: ItemEdit) -> Result<ItemMeta, VaultError> {
        if edit.is_empty() {
            // Nothing to write: a new revision would make other editors stale.
            let meta = self
                .store
                .item_meta(edit.id)?
                .ok_or(VaultError::ItemNotFound)?;
            return if meta.revision == edit.base {
                Ok(meta)
            } else {
                Err(VaultError::StaleRevision)
            };
        }
        let ItemEdit {
            id,
            base,
            title,
            folder,
            content,
            icon: icon_choice,
            link,
            favorite,
            trashed,
            resolve,
        } = edit;
        let ItemRecord {
            mut meta,
            ciphertext,
        } = self.record(id)?;
        // Fail before decrypting or encrypting; the store checks again on commit.
        if meta.revision != base {
            return Err(VaultError::StaleRevision);
        }

        if let Some(title) = title {
            meta.state.title =
                clean_line(&title, MAX_TITLE_BYTES).ok_or(VaultError::InvalidTitle)?;
        }
        if let Some(folder) = folder {
            if let Some(folder) = folder {
                self.require_folder(folder)?;
            }
            meta.state.folder = folder;
        }
        if let Some(favorite) = favorite {
            meta.state.favorite = favorite;
        }
        if let Some(trashed) = trashed {
            meta.state.trashed = trashed;
        }
        if let LinkChange::Set(target) = link {
            self.check_link(id, meta.state.trashed, target)?;
        }

        let templates = Arc::clone(&self.templates);
        // New content is written at the current template version, which also
        // completes a migration done when the item was opened.
        let resealed = match content {
            Some(mut content) => {
                if *content.template() != meta.state.template {
                    return Err(VaultError::TemplateChanged);
                }
                let template = templates.current(&content)?;
                content.normalize(template);
                let plaintext = encode_item_with(&content, template, &self.rules)?;
                let ciphertext = self
                    .cipher
                    .encrypt(plaintext.as_slice(), &meta.state.recipients)?;
                Some((ciphertext, content, template))
            }
            None => None,
        };

        let (icon, icon_asset) = match &resealed {
            Some((_, content, template)) => {
                icon::resolve(icon_choice, meta.state.icon.as_ref(), true, || {
                    Ok(icon::automatic(content, template))
                })?
            }
            None => icon::resolve(icon_choice, meta.state.icon.as_ref(), false, || {
                let (content, _) = self.decrypt(&meta, &ciphertext)?;
                let template = templates.template(meta.state.template.as_str())?;
                Ok(icon::automatic(&content, template))
            })?,
        };
        meta.state.icon = icon;
        meta.revision = next_revision(base)?;
        meta.head = VersionId::new();
        // Resolving an item that is not in conflict is an ordinary edit.
        let heads = if resolve {
            self.store.heads(id)?
        } else {
            Vec::new()
        };
        let parents = if heads.len() > 1 {
            Parents::Merge(heads.into_iter().map(|head| head.id).collect())
        } else {
            Parents::Head
        };

        let ciphertext = resealed.map_or(ciphertext, |(ciphertext, _, _)| ciphertext);
        self.store.commit(Change::PutItem(Box::new(PutItem {
            record: ItemRecord { meta, ciphertext },
            base: Base::Revision(base),
            parents,
            created_at: unix_now(),
            icon_asset,
            link,
        })))?;
        // Read back: the store decides whether the item is still in conflict.
        let meta = self.store.item_meta(id)?.ok_or(VaultError::ItemNotFound)?;
        tracing::debug!(
            item = %id.0,
            revision = meta.revision,
            conflicted = meta.conflicted,
            resolved = resolve,
            "item updated"
        );
        Ok(meta)
    }

    fn try_remove_item(&mut self, id: SecretId, base: u64) -> Result<(), VaultError> {
        let meta = self.store.item_meta(id)?.ok_or(VaultError::ItemNotFound)?;
        if meta.revision != base {
            return Err(VaultError::StaleRevision);
        }
        self.store.commit(Change::RemoveItem { id, base })?;
        tracing::debug!(item = %id.0, "item removed");
        Ok(())
    }

    pub(super) fn record(&self, id: SecretId) -> Result<ItemRecord, VaultError> {
        self.store.item(id)?.ok_or(VaultError::ItemNotFound)
    }

    pub(super) fn require_folder(&self, id: FolderId) -> Result<Folder, VaultError> {
        self.store.folder(id)?.ok_or(VaultError::FolderNotFound)
    }

    /// Decrypts, decodes, checks the content belongs to the record and
    /// migrates it to the current template version. Returns the version it
    /// was migrated from, if any.
    pub(super) fn decrypt(
        &self,
        meta: &ItemMeta,
        ciphertext: &[u8],
    ) -> Result<(ItemContent, Option<u32>), VaultError> {
        let plaintext = self.cipher.decrypt(ciphertext)?;
        let mut content = decode_item(plaintext.as_slice()).inspect_err(|error| {
            tracing::warn!(item = %meta.id.0, %error, "stored item content is unreadable");
        })?;
        if *content.template() != meta.state.template {
            tracing::warn!(item = %meta.id.0, "stored item content names another template");
            return Err(VaultError::CorruptContent);
        }
        let template = self.templates.template(meta.state.template.as_str())?;
        let migrated_from = content.migrate(template).inspect_err(|error| {
            tracing::warn!(item = %meta.id.0, %error, "stored item content was not migrated");
        })?;
        if let Some(from) = migrated_from {
            tracing::info!(
                item = %meta.id.0,
                from,
                to = template.version,
                "item content migrated in memory"
            );
        }
        Ok((content, migrated_from))
    }

    /// Checks that `id` may pair with `target`: neither trashed, not itself,
    /// and the target not paired with another item. Unpairing always works.
    fn check_link(
        &self,
        id: SecretId,
        trashed: bool,
        target: Option<SecretId>,
    ) -> Result<(), VaultError> {
        let Some(target) = target else {
            return Ok(());
        };
        if self.store.link_of(id)? == Some(target) {
            return Ok(());
        }
        if trashed || target == id {
            return Err(SecretLinkError::InvalidTarget.into());
        }
        let other = self
            .store
            .item_meta(target)?
            .ok_or(SecretLinkError::InvalidTarget)?;
        if other.state.trashed {
            return Err(SecretLinkError::InvalidTarget.into());
        }
        match self.store.link_of(target)? {
            Some(companion) if companion != id => Err(SecretLinkError::Occupied.into()),
            _ => Ok(()),
        }
    }
}

fn resolve_recipients(
    recipients: Recipients,
    folder: Option<&Folder>,
) -> Result<RecipientSet, VaultError> {
    match recipients {
        Recipients::Explicit(recipients) => Ok(recipients),
        Recipients::FolderDefault => folder
            .and_then(|folder| folder.default_recipients.clone())
            .ok_or(VaultError::NoRecipients),
    }
}
