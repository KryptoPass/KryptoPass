//! Importing a batch of folders and items from another password manager.
//!
//! The batch is already in template form (the application converts each
//! source entry); here every item is checked, encrypted to one recipient set
//! and the whole batch is stored in one change, so an import either lands
//! complete or not at all.

use std::collections::HashMap;
use std::sync::Arc;

use super::error::VaultError;
use super::folders::MAX_FOLDER_NAME_BYTES;
use super::icon;
use super::item::Recipients;
use super::items::MAX_TITLE_BYTES;
use super::record::{ItemMeta, ItemRecord, ItemState, VersionId};
use super::store::{AppendItem, Change};
use super::{Cipher, Store, Vault, logged};
use crate::domain::item::{ItemContent, encode_item_with};
use crate::domain::{Folder, FolderAppearance, FolderId, SecretId};
use crate::util::{clean_line, unix_millis};

/// Folders and items to import, folders referring to each other by index.
#[derive(Default)]
pub struct ImportBatch {
    pub folders: Vec<ImportFolder>,
    pub items: Vec<ImportItem>,
}

pub struct ImportFolder {
    pub name: String,
    /// Index of the parent in [`ImportBatch::folders`]; `None` puts the
    /// folder at the destination.
    pub parent: Option<usize>,
}

pub struct ImportItem {
    /// Index in [`ImportBatch::folders`]; `None` puts the item at the
    /// destination.
    pub folder: Option<usize>,
    pub title: String,
    pub content: ItemContent,
    pub favorite: bool,
    pub trashed: bool,
    /// When the source last changed the item, in milliseconds since the Unix
    /// epoch; the import time when the source does not say.
    pub written_millis: Option<u64>,
}

/// What an import created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportReceipt {
    /// The new folder of each [`ImportBatch::folders`] entry, in order.
    pub folders: Vec<FolderId>,
    pub items: Vec<SecretId>,
}

impl<S: Store, C: Cipher> Vault<S, C> {
    /// Imports `batch` under `destination`, or at the root, encrypting every
    /// item to `recipients`. Nothing is stored unless every item is valid and
    /// encrypted.
    pub fn import(
        &mut self,
        batch: ImportBatch,
        recipients: Recipients,
        destination: Option<FolderId>,
    ) -> Result<ImportReceipt, VaultError> {
        logged("import", self.try_import(batch, recipients, destination))
    }

    fn try_import(
        &mut self,
        batch: ImportBatch,
        recipients: Recipients,
        destination: Option<FolderId>,
    ) -> Result<ImportReceipt, VaultError> {
        let destination_folder = destination.map(|id| self.require_folder(id)).transpose()?;
        let recipients = self.resolve_recipients(recipients, destination_folder.as_ref())?;
        self.cipher.check_recipients(&recipients)?;

        let ids: Vec<FolderId> = batch.folders.iter().map(|_| FolderId::new()).collect();
        let folder_of = |index: Option<usize>| -> Result<Option<FolderId>, VaultError> {
            match index {
                Some(index) => ids
                    .get(index)
                    .copied()
                    .map(Some)
                    .ok_or(VaultError::FolderNotFound),
                None => Ok(destination),
            }
        };

        let mut folders = Vec::with_capacity(batch.folders.len());
        for index in parents_first(&batch.folders)? {
            let folder = &batch.folders[index];
            folders.push(Folder {
                id: ids[index],
                parent_id: folder_of(folder.parent)?,
                sort_position: 0,
                name: clean_line(&folder.name, MAX_FOLDER_NAME_BYTES)
                    .ok_or(VaultError::InvalidFolderName)?,
                appearance: FolderAppearance::default(),
                default_recipients: None,
            });
        }

        let templates = Arc::clone(&self.templates);
        let now = unix_millis();
        let mut items = Vec::with_capacity(batch.items.len());
        for item in batch.items {
            let ImportItem {
                folder,
                title,
                mut content,
                favorite,
                trashed,
                written_millis,
            } = item;
            let template = templates.current(&content)?;
            let title = clean_line(&title, MAX_TITLE_BYTES).ok_or(VaultError::InvalidTitle)?;
            content.normalize(template);
            let plaintext = encode_item_with(&content, template, &self.rules)?;
            let ciphertext = self.cipher.encrypt(plaintext.as_slice(), &recipients)?;
            items.push(AppendItem {
                record: ItemRecord {
                    meta: ItemMeta {
                        id: SecretId::new(),
                        head: VersionId::new(),
                        revision: 0,
                        conflicted: false,
                        state: ItemState {
                            folder: folder_of(folder)?,
                            title,
                            template: template.id.clone(),
                            recipients: recipients.clone(),
                            icon: icon::automatic(&content, template),
                            favorite,
                            trashed,
                        },
                    },
                    ciphertext,
                },
                written_millis: written_millis.unwrap_or(now),
            });
        }

        let receipt = ImportReceipt {
            folders: ids,
            items: items.iter().map(|item| item.record.meta.id).collect(),
        };
        self.store.commit(Change::Append {
            folders,
            items,
            links: Vec::new(),
        })?;
        tracing::info!(
            folders = receipt.folders.len(),
            items = receipt.items.len(),
            "import stored"
        );
        Ok(receipt)
    }
}

/// Indices of `folders` with every parent before its children. Fails on a
/// parent index out of range or a cycle.
fn parents_first(folders: &[ImportFolder]) -> Result<Vec<usize>, VaultError> {
    let mut children: HashMap<Option<usize>, Vec<usize>> = HashMap::new();
    for (index, folder) in folders.iter().enumerate() {
        if folder.parent.is_some_and(|parent| parent >= folders.len()) {
            return Err(VaultError::FolderNotFound);
        }
        children.entry(folder.parent).or_default().push(index);
    }
    let mut order = Vec::with_capacity(folders.len());
    let mut pending = children.remove(&None).unwrap_or_default();
    while let Some(index) = pending.pop() {
        order.push(index);
        pending.extend(children.remove(&Some(index)).unwrap_or_default());
    }
    // Folders never reached from the top form a cycle among themselves.
    if order.len() == folders.len() {
        Ok(order)
    } else {
        Err(VaultError::FolderCycle)
    }
}
