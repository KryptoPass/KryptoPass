//! Recipients and keys: which keys new items go to, which keys this device
//! can read with, and per-key settings.
//!
//! There is no vault key. Every key is equal: an item is readable by any
//! device holding the secret part of one of its recipients, and a device may
//! hold several keys, or none of an item's.

use std::collections::HashSet;

use super::error::VaultError;
use super::item::Recipients;
use super::store::Change;
use super::{Cipher, Store, Vault, logged};
use crate::domain::{Folder, KeyFingerprint, RecipientSet};

/// Settings of one key, by its primary fingerprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeySettings {
    pub fingerprint: KeyFingerprint,
    /// Make gpg-agent forget this key's passphrase when the vault locks.
    pub forget_on_lock: bool,
}

impl KeySettings {
    /// A key's settings before anyone changes them.
    pub fn new(fingerprint: KeyFingerprint) -> Self {
        Self {
            fingerprint,
            forget_on_lock: false,
        }
    }

    /// Whether these are the settings of a key nobody configured.
    pub fn is_default(&self) -> bool {
        !self.forget_on_lock
    }
}

impl<S: Store, C: Cipher> Vault<S, C> {
    /// Recipients of new items outside a folder with defaults.
    pub fn default_recipients(&self) -> Result<Option<RecipientSet>, VaultError> {
        logged("default_recipients", Ok(self.store.default_recipients()?))
    }

    /// Replaces the default recipients after checking every key can be
    /// encrypted to. Existing items keep theirs.
    pub fn set_default_recipients(
        &mut self,
        recipients: Option<RecipientSet>,
    ) -> Result<(), VaultError> {
        logged(
            "set_default_recipients",
            self.try_set_default_recipients(recipients),
        )
    }

    /// Settings of every configured key; keys left out have the defaults.
    pub fn key_settings(&self) -> Result<Vec<KeySettings>, VaultError> {
        logged("key_settings", Ok(self.store.key_settings()?))
    }

    /// The settings of one key, the defaults when it has none stored.
    pub fn settings_of(&self, key: &KeyFingerprint) -> Result<KeySettings, VaultError> {
        Ok(self
            .key_settings()?
            .into_iter()
            .find(|settings| &settings.fingerprint == key)
            .unwrap_or_else(|| KeySettings::new(key.clone())))
    }

    pub fn set_key_settings(&mut self, settings: KeySettings) -> Result<(), VaultError> {
        logged(
            "set_key_settings",
            self.store
                .commit(Change::SetKeySettings(settings))
                .map_err(VaultError::from),
        )
    }

    /// Keys whose secret part is on this device. An item is readable here
    /// when one of its recipients is among them; see
    /// [`RecipientSet::includes_any`].
    pub fn secret_keys(&self) -> Result<HashSet<KeyFingerprint>, VaultError> {
        logged(
            "secret_keys",
            self.cipher
                .secret_keys()
                .map(|keys| keys.into_iter().collect())
                .map_err(VaultError::from),
        )
    }

    fn try_set_default_recipients(
        &mut self,
        recipients: Option<RecipientSet>,
    ) -> Result<(), VaultError> {
        if let Some(recipients) = &recipients {
            self.cipher.check_recipients(recipients)?;
        }
        let count = recipients.as_ref().map_or(0, |set| set.as_slice().len());
        self.store
            .commit(Change::SetDefaultRecipients(recipients))?;
        tracing::debug!(recipients = count, "default recipients changed");
        Ok(())
    }

    /// The recipients of a new item: explicit ones, else the folder's
    /// defaults, else the vault's.
    pub(super) fn resolve_recipients(
        &self,
        recipients: Recipients,
        folder: Option<&Folder>,
    ) -> Result<RecipientSet, VaultError> {
        match recipients {
            Recipients::Explicit(recipients) => Ok(recipients),
            Recipients::Default => {
                match folder.and_then(|folder| folder.default_recipients.clone()) {
                    Some(recipients) => Ok(recipients),
                    None => self
                        .store
                        .default_recipients()?
                        .ok_or(VaultError::NoRecipients),
                }
            }
        }
    }
}
