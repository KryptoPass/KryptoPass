//! What this device can read: indexing versions that arrived sealed, and the
//! items it cannot open.
//!
//! Versions received through Sync are stored as they came, encrypted. Their
//! state becomes listable once this device opens it, which happens here, for
//! every version encrypted to a key whose secret part is on this device.

use super::error::VaultError;
use super::state::open_state;
use super::store::{Change, IndexedVersion, OpaqueItem};
use super::{Cipher, CipherError, Store, Vault, logged};

impl<S: Store, C: Cipher> Vault<S, C> {
    /// Opens and indexes every stored version this device can read but has
    /// not indexed yet, such as ones received through Sync or readable since
    /// a key was imported. Returns how many it indexed.
    ///
    /// Versions for other keys stay sealed. With the passphrase locked, the
    /// rest waits for a later call; a version whose state is malformed is
    /// left out and reported.
    pub fn reindex(&mut self) -> Result<usize, VaultError> {
        logged("reindex", self.try_reindex())
    }

    /// Items stored here none of whose current versions this device opens,
    /// shown as not readable on this device.
    pub fn opaque_items(&self) -> Result<Vec<OpaqueItem>, VaultError> {
        logged("opaque_items", Ok(self.store.opaque_items()?))
    }

    fn try_reindex(&mut self) -> Result<usize, VaultError> {
        let keys = self.secret_keys()?;
        let mut indexed = Vec::new();
        let mut corrupt = 0_usize;
        for sealed in self.store.unindexed()? {
            if !sealed.recipients.includes_any(&keys) {
                continue;
            }
            let plaintext = match self.cipher.decrypt(&sealed.state) {
                Ok(plaintext) => plaintext,
                Err(CipherError::NotARecipient) => continue,
                Err(CipherError::KeyUnavailable | CipherError::Canceled) => break,
                Err(error) => return Err(error.into()),
            };
            match open_state(plaintext.as_slice()) {
                Ok(state) => indexed.push(IndexedVersion {
                    id: sealed.id,
                    item: sealed.item,
                    state,
                }),
                Err(_) => corrupt += 1,
            }
        }
        let count = indexed.len();
        if count > 0 {
            self.store.commit(Change::Index(indexed))?;
        }
        if corrupt > 0 {
            tracing::warn!(corrupt, "versions with an unreadable state left unindexed");
        }
        tracing::debug!(indexed = count, "versions indexed");
        Ok(count)
    }
}
