//! Provider-independent input and application policy for additive imports.

use crate::{FolderId, RecipientSet, SecretContent, encode_content};
use anyhow::Result;
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const MAX_IMPORT_RECORDS: usize = 10_000;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ImportValidationError {
    #[error("import batch exceeds the limit")]
    Limit,
    #[error("import batch contains an invalid hierarchy")]
    Hierarchy,
    #[error("import batch contains invalid content")]
    Content,
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ImportFolder {
    pub name: String,
    #[zeroize(skip)]
    pub parent: Option<usize>,
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ImportItem {
    pub title: String,
    pub content: SecretContent,
    #[zeroize(skip)]
    pub folder: usize,
    #[zeroize(skip)]
    pub favorite: bool,
    #[zeroize(skip)]
    pub trashed: bool,
}

/// Common domain format accepted from any provider; plaintext never enters UI.
#[derive(Default)]
pub struct ImportBatch {
    pub folders: Vec<ImportFolder>,
    pub items: Vec<ImportItem>,
}

/// The application chooses placement; providers cannot select a destination.
#[derive(Clone, Copy)]
pub enum ImportFolderPolicy {
    PreserveHierarchy { parent: Option<FolderId> },
    PlaceAll { folder: Option<FolderId> },
}

pub struct ImportOptions {
    pub recipients: RecipientSet,
    pub folders: ImportFolderPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportReceipt {
    pub folders_added: usize,
    pub items_added: usize,
}

/// Core's application boundary. Success means the entire batch was appended.
pub trait ImportReceiver {
    fn receive_import(
        &mut self,
        batch: ImportBatch,
        options: ImportOptions,
    ) -> Result<ImportReceipt>;
}

impl ImportBatch {
    pub fn validate(&self) -> Result<(), ImportValidationError> {
        if self.folders.len().saturating_add(self.items.len()) > MAX_IMPORT_RECORDS {
            return Err(ImportValidationError::Limit);
        }
        for (index, folder) in self.folders.iter().enumerate() {
            if folder.name.trim().is_empty() || folder.name.len() > 256 {
                return Err(ImportValidationError::Content);
            }
            let mut next = folder.parent;
            let mut depth = 0;
            while let Some(parent) = next {
                if parent >= self.folders.len() || parent == index || depth >= self.folders.len() {
                    return Err(ImportValidationError::Hierarchy);
                }
                next = self.folders[parent].parent;
                depth += 1;
            }
        }
        for item in &self.items {
            if item.folder >= self.folders.len() {
                return Err(ImportValidationError::Hierarchy);
            }
            if item.title.trim().is_empty() || item.title.trim().len() > 512 {
                return Err(ImportValidationError::Content);
            }
            encode_content(&item.content).map_err(|_| ImportValidationError::Content)?;
        }
        Ok(())
    }
}
