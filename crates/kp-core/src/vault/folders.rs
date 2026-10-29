//! The folder tree. Folders organize items; they are not crypto boundaries.

use std::collections::HashMap;

use super::error::VaultError;
use super::store::{Change, FolderContents};
use super::{Cipher, Store, Vault, logged};
use crate::domain::{Folder, FolderAppearance, FolderId, RecipientSet};
use crate::util::{Ancestry, ancestry, clean_line};

const MAX_FOLDER_NAME_BYTES: usize = 256;

/// A folder to create.
pub struct NewFolder {
    pub name: String,
    pub parent: Option<FolderId>,
    pub appearance: FolderAppearance,
    /// Copied into items created in the folder; never re-encrypts existing ones.
    pub default_recipients: Option<RecipientSet>,
}

/// Changes to a folder's name, appearance or default recipients.
#[must_use]
pub struct FolderEdit {
    id: FolderId,
    name: Option<String>,
    appearance: Option<FolderAppearance>,
    default_recipients: Option<Option<RecipientSet>>,
}

impl FolderEdit {
    pub fn new(id: FolderId) -> Self {
        Self {
            id,
            name: None,
            appearance: None,
            default_recipients: None,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn appearance(mut self, appearance: FolderAppearance) -> Self {
        self.appearance = Some(appearance);
        self
    }

    /// Sets the default recipients, or clears them with `None`.
    pub fn default_recipients(mut self, recipients: Option<RecipientSet>) -> Self {
        self.default_recipients = Some(recipients);
        self
    }
}

impl<S: Store, C: Cipher> Vault<S, C> {
    /// All folders, siblings in display order.
    pub fn list_folders(&self) -> Result<Vec<Folder>, VaultError> {
        logged("list_folders", self.store.folders().map_err(Into::into))
    }

    pub fn create_folder(&mut self, new: NewFolder) -> Result<Folder, VaultError> {
        logged("create_folder", self.try_create_folder(new))
    }

    pub fn update_folder(&mut self, edit: FolderEdit) -> Result<Folder, VaultError> {
        logged("update_folder", self.try_update_folder(edit))
    }

    /// Moves a folder under `destination` (`None` is the root), before
    /// `before` or after the last sibling.
    pub fn move_folder(
        &mut self,
        id: FolderId,
        destination: Option<FolderId>,
        before: Option<FolderId>,
    ) -> Result<(), VaultError> {
        logged("move_folder", self.try_move_folder(id, destination, before))
    }

    pub fn remove_folder(
        &mut self,
        id: FolderId,
        contents: FolderContents,
    ) -> Result<(), VaultError> {
        logged("remove_folder", self.try_remove_folder(id, contents))
    }

    fn try_create_folder(&mut self, new: NewFolder) -> Result<Folder, VaultError> {
        let name =
            clean_line(&new.name, MAX_FOLDER_NAME_BYTES).ok_or(VaultError::InvalidFolderName)?;
        if let Some(parent) = new.parent {
            self.require_folder(parent)?;
        }
        if let Some(recipients) = &new.default_recipients {
            self.cipher.check_recipients(recipients)?;
        }
        let folder = Folder {
            id: FolderId::new(),
            parent_id: new.parent,
            sort_position: 0,
            name,
            appearance: new.appearance,
            default_recipients: new.default_recipients,
        };
        self.store.commit(Change::AddFolder(folder.clone()))?;
        tracing::debug!(folder = %folder.id.0, "folder created");
        Ok(folder)
    }

    fn try_update_folder(&mut self, edit: FolderEdit) -> Result<Folder, VaultError> {
        let mut folder = self.require_folder(edit.id)?;
        if let Some(name) = edit.name {
            folder.name =
                clean_line(&name, MAX_FOLDER_NAME_BYTES).ok_or(VaultError::InvalidFolderName)?;
        }
        if let Some(appearance) = edit.appearance {
            folder.appearance = appearance;
        }
        if let Some(recipients) = edit.default_recipients {
            if let Some(recipients) = &recipients {
                self.cipher.check_recipients(recipients)?;
            }
            folder.default_recipients = recipients;
        }
        self.store.commit(Change::UpdateFolder(folder.clone()))?;
        tracing::debug!(folder = %folder.id.0, "folder updated");
        Ok(folder)
    }

    fn try_move_folder(
        &mut self,
        id: FolderId,
        destination: Option<FolderId>,
        before: Option<FolderId>,
    ) -> Result<(), VaultError> {
        let parents: HashMap<FolderId, Option<FolderId>> = self
            .store
            .folders()?
            .into_iter()
            .map(|folder| (folder.id, folder.parent_id))
            .collect();
        if !parents.contains_key(&id) {
            return Err(VaultError::FolderNotFound);
        }
        if let Some(destination) = destination {
            match ancestry(&parents, destination, id) {
                Ancestry::Outside => {}
                Ancestry::Within => return Err(VaultError::FolderCycle),
                Ancestry::Broken => {
                    tracing::warn!(folder = %destination.0, "folder parents are broken");
                    return Err(VaultError::FolderNotFound);
                }
            }
        }
        if let Some(before) = before {
            // The anchor must be a sibling at the destination, and not the folder itself.
            match parents.get(&before) {
                Some(parent) if *parent == destination && before != id => {}
                Some(_) => return Err(VaultError::InvalidPosition),
                None => return Err(VaultError::FolderNotFound),
            }
        }
        self.store.commit(Change::MoveFolder {
            id,
            destination,
            before,
        })?;
        tracing::debug!(folder = %id.0, "folder moved");
        Ok(())
    }

    fn try_remove_folder(
        &mut self,
        id: FolderId,
        contents: FolderContents,
    ) -> Result<(), VaultError> {
        self.require_folder(id)?;
        self.store.commit(Change::RemoveFolder { id, contents })?;
        tracing::debug!(folder = %id.0, ?contents, "folder removed");
        Ok(())
    }
}
