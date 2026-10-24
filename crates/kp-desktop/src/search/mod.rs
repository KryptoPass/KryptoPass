//! Vault-specific adapters for the independent private-search application service.

mod access;
mod projection;

pub(crate) use access::{VaultSearchSession, attach, is_unlocked, set_locked};

use crate::{infrastructure::GnuPgOpenPgp, storage::SqliteStorage};
use kryptopass_core::{KeyFingerprint, SecretId, SecretKind, Storage};
use kryptopass_search::{Projection, RecordKey, SearchError, SearchSource};
use kryptopass_secure_memory::ProtectedBytes;
use std::path::PathBuf;

#[cfg(test)]
#[path = "../../tests/unit/search_source.rs"]
mod tests;

trait ProjectionDecryptor: Send + 'static {
    fn decrypt(&self, ciphertext: &[u8], plaintext: &mut ProtectedBytes)
    -> Result<(), SearchError>;
}

impl ProjectionDecryptor for GnuPgOpenPgp {
    fn decrypt(
        &self,
        ciphertext: &[u8],
        plaintext: &mut ProtectedBytes,
    ) -> Result<(), SearchError> {
        self.decrypt_for_search(ciphertext, plaintext)
            .map_err(|_| SearchError::Unavailable)
    }
}

/// The database path and primary key are pinned to one unlocked vault session.
pub(crate) struct VaultSearchSource<D = GnuPgOpenPgp> {
    path: PathBuf,
    primary: KeyFingerprint,
    storage: Option<SqliteStorage>,
    plaintext: Option<ProtectedBytes>,
    decryptor: D,
}

impl VaultSearchSource {
    pub fn new(path: PathBuf, primary: KeyFingerprint) -> Self {
        Self::with_decryptor(path, primary, GnuPgOpenPgp)
    }
}

impl<D> VaultSearchSource<D> {
    fn with_decryptor(path: PathBuf, primary: KeyFingerprint, decryptor: D) -> Self {
        Self {
            path,
            primary,
            storage: None,
            plaintext: None,
            decryptor,
        }
    }
}

impl<D: ProjectionDecryptor> SearchSource for VaultSearchSource<D> {
    fn load(&mut self, record: RecordKey) -> Result<Projection, SearchError> {
        if self.storage.is_none() {
            self.storage = Some(
                SqliteStorage::open_existing(&self.path).map_err(|_| SearchError::Unavailable)?,
            );
        }
        let storage = self.storage.as_ref().ok_or(SearchError::Unavailable)?;
        let id = SecretId(uuid::Uuid::from_bytes(record.id));
        let secret = storage
            .find_secret(id)
            .map_err(|_| SearchError::Unavailable)?
            .ok_or(SearchError::StaleRevision)?;
        if secret.revision != record.revision || secret.trashed || secret.kind != SecretKind::Login
        {
            return Err(SearchError::StaleRevision);
        }
        if !secret.recipients.as_slice().contains(&self.primary) {
            return Err(SearchError::Unavailable);
        }
        if self.plaintext.is_none() {
            self.plaintext = Some(
                ProtectedBytes::try_with_capacity(4096)
                    .map_err(|_| SearchError::ProtectionUnavailable)?,
            );
        }
        let plaintext = self.plaintext.as_mut().ok_or(SearchError::Unavailable)?;
        let projected = self
            .decryptor
            .decrypt(&secret.ciphertext, plaintext)
            .and_then(|()| projection::login_username(plaintext.as_slice()));
        plaintext.clear();
        let projected = projected?;
        let current = storage
            .find_secret(id)
            .map_err(|_| SearchError::Unavailable)?
            .ok_or(SearchError::StaleRevision)?;
        if current.revision != record.revision
            || current.trashed
            || current.recipients != secret.recipients
        {
            return Err(SearchError::StaleRevision);
        }
        Ok(projected)
    }
}
