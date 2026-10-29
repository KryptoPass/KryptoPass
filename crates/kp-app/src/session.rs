//! An unlocked vault: one file, the templates and the keyring.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kryptopass_core::vault::{TemplateRegistry, Vault};
use kryptopass_keyring::{Fingerprint, Keyring, KeyringCipher, Unlock};
use kryptopass_store::SqliteStore;

use crate::error::AppError;

/// A vault on the SQLite store, encrypting with GnuPG.
pub type AppVault = Vault<SqliteStore, KeyringCipher>;

/// What the person has open. Its vault may ask for the passphrase in
/// pinentry; background work uses [`VaultSession::background`], which never
/// prompts.
pub struct VaultSession {
    path: PathBuf,
    keyring: Keyring,
    vault: AppVault,
}

impl VaultSession {
    /// Opens the vault at `path`, creating it if it is new.
    pub fn open(
        path: impl Into<PathBuf>,
        keyring: Keyring,
        templates: Arc<TemplateRegistry>,
    ) -> Result<Self, AppError> {
        let path = path.into();
        let store = SqliteStore::open(&path).map_err(AppError::Store)?;
        let cipher = KeyringCipher::new(keyring.clone(), Unlock::Ask);
        tracing::info!("vault session opened");
        Ok(Self {
            path,
            keyring,
            vault: Vault::new(store, cipher, templates),
        })
    }

    pub fn vault(&self) -> &AppVault {
        &self.vault
    }

    pub fn vault_mut(&mut self) -> &mut AppVault {
        &mut self.vault
    }

    pub fn templates(&self) -> &Arc<TemplateRegistry> {
        self.vault.templates()
    }

    pub fn keyring(&self) -> &Keyring {
        &self.keyring
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A second connection to the same file that only decrypts with
    /// passphrases gpg-agent already caches, for search and Sync. SQLite's
    /// journal lets it read while the session writes.
    pub fn background(&self) -> Result<AppVault, AppError> {
        background_vault(&self.path, &self.keyring, Arc::clone(self.templates()))
    }

    /// Closes the vault and makes gpg-agent forget the passphrase of every
    /// key set to forget on lock, so reopening asks again. The store closes
    /// even if reading the settings or forgetting fails.
    pub fn lock(self) -> Result<(), AppError> {
        let Self { keyring, vault, .. } = self;
        let settings = vault.key_settings();
        drop(vault.into_parts());
        let keys = settings?
            .into_iter()
            .filter(|settings| settings.forget_on_lock)
            .map(|settings| Fingerprint::parse(settings.fingerprint.as_str()))
            .map(|key| key.map_err(kryptopass_keyring::Error::from))
            .collect::<Result<Vec<_>, _>>()?;
        for key in &keys {
            keyring.forget_passphrase(key)?;
        }
        tracing::info!(forgotten = keys.len(), "vault session locked");
        Ok(())
    }
}

pub(crate) fn background_vault(
    path: &Path,
    keyring: &Keyring,
    templates: Arc<TemplateRegistry>,
) -> Result<AppVault, AppError> {
    let store = SqliteStore::open_existing(path).map_err(AppError::Store)?;
    let cipher = KeyringCipher::new(keyring.clone(), Unlock::Cached);
    Ok(Vault::new(store, cipher, templates))
}
