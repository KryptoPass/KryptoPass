//! What this device can read: indexing versions that arrived sealed, and the
//! items it cannot open.
//!
//! Versions received through Sync are stored as they came, encrypted. Their
//! state becomes listable once this device opens it, which happens here, for
//! every version encrypted to a key whose secret part is on this device.

use super::error::VaultError;
use super::state::{open_folder, open_state};
use super::store::{Change, IndexedFolder, IndexedVersion, OpaqueItem, SealedRecord};
use super::{Cipher, CipherError, Store, Vault, logged};
use crate::domain::KeyFingerprint;

/// What one [`Vault::reindex_report`] pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reindex {
    /// Versions that became listable.
    pub indexed: usize,
    /// A secret key of this device whose passphrase the pass needed and did not have, when
    /// that is why it stopped. What it could not open is not lost or failed: it waits for a
    /// later pass, and Sync is not affected, because the exchange never decrypts.
    pub waiting_for: Option<KeyFingerprint>,
}

impl<S: Store, C: Cipher> Vault<S, C> {
    /// Opens and indexes every stored version this device can read but has
    /// not indexed yet, such as ones received through Sync or readable since
    /// a key was imported. Returns how many it indexed.
    ///
    /// Versions for other keys stay sealed. With the passphrase locked, the
    /// rest waits for a later call; a version whose state is malformed is
    /// left out and reported.
    pub fn reindex(&mut self) -> Result<usize, VaultError> {
        Ok(self.reindex_report()?.indexed)
    }

    /// Like [`Vault::reindex`], and says which key it was waiting for when a locked passphrase
    /// stopped it, so the person can be asked for that one.
    pub fn reindex_report(&mut self) -> Result<Reindex, VaultError> {
        logged("reindex", self.try_reindex())
    }

    /// Items stored here none of whose current versions this device opens,
    /// shown as not readable on this device.
    pub fn opaque_items(&self) -> Result<Vec<OpaqueItem>, VaultError> {
        logged("opaque_items", Ok(self.store.opaque_items()?))
    }

    fn try_reindex(&mut self) -> Result<Reindex, VaultError> {
        let keys = self.secret_keys()?;
        let mut indexed = Vec::new();
        let mut folders = Vec::new();
        let mut corrupt = 0_usize;
        let mut waiting_for = None;
        for sealed in self.store.unindexed()? {
            if !sealed.recipients.includes_any(&keys) {
                continue;
            }
            let plaintext = match self.cipher.decrypt(&sealed.state) {
                Ok(plaintext) => plaintext,
                Err(CipherError::NotARecipient) => continue,
                Err(CipherError::KeyUnavailable | CipherError::Canceled) => {
                    // The key of this device that this version is encrypted to.
                    waiting_for = sealed
                        .recipients
                        .as_slice()
                        .iter()
                        .find(|key| keys.contains(*key))
                        .cloned();
                    break;
                }
                Err(error) => return Err(error.into()),
            };
            let opened = match sealed.record {
                SealedRecord::Item(item) => open_state(plaintext.as_slice()).map(|state| {
                    indexed.push(IndexedVersion {
                        id: sealed.id,
                        item,
                        state,
                    });
                }),
                SealedRecord::Folder(folder) => open_folder(plaintext.as_slice()).map(|state| {
                    folders.push(IndexedFolder {
                        id: sealed.id,
                        folder,
                        state,
                    });
                }),
            };
            if opened.is_err() {
                corrupt += 1;
            }
        }
        let count = indexed.len() + folders.len();
        // Folders first, so items indexed with them find their folder.
        if !folders.is_empty() {
            self.store.commit(Change::IndexFolders(folders))?;
        }
        if !indexed.is_empty() {
            self.store.commit(Change::Index(indexed))?;
        }
        if corrupt > 0 {
            tracing::warn!(corrupt, "versions with an unreadable state left unindexed");
        }
        tracing::debug!(
            indexed = count,
            waiting = waiting_for.is_some(),
            "versions indexed"
        );
        Ok(Reindex {
            indexed: count,
            waiting_for,
        })
    }
}
