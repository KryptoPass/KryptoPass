//! The vault's encryption port on this keyring.
//!
//! Only compiled with the `vault` feature, so the keyring manager does not
//! depend on the vault model.

use kryptopass_core::util::MAX_CIPHERTEXT_BYTES;
use kryptopass_core::vault::{Cipher, CipherError};
use kryptopass_core::{KeyFingerprint, MAX_CONTENT_BYTES, RecipientSet, SensitiveBytes};

use crate::crypto::Unlock;
use crate::error::Error;
use crate::keyring::Keyring;
use crate::types::Fingerprint;

/// Encrypts vault items to their recipients with this keyring.
#[derive(Clone, Debug)]
pub struct KeyringCipher {
    keyring: Keyring,
    unlock: Unlock,
}

impl KeyringCipher {
    /// `unlock` decides whether decrypting may open pinentry: [`Unlock::Ask`]
    /// for the person's own actions, [`Unlock::Cached`] for background work.
    pub fn new(keyring: Keyring, unlock: Unlock) -> Self {
        Self { keyring, unlock }
    }
}

impl Cipher for KeyringCipher {
    fn check_recipients(&self, recipients: &RecipientSet) -> Result<(), CipherError> {
        for recipient in recipients.as_slice() {
            self.keyring
                .check_recipient(&fingerprint(recipient)?)
                .map_err(cipher_error)?;
        }
        Ok(())
    }

    fn encrypt(&self, plaintext: &[u8], recipients: &RecipientSet) -> Result<Vec<u8>, CipherError> {
        if plaintext.len() > MAX_CONTENT_BYTES {
            return Err(CipherError::Backend(anyhow::anyhow!(
                "plaintext exceeds the content limit"
            )));
        }
        let recipients = recipients
            .as_slice()
            .iter()
            .map(fingerprint)
            .collect::<Result<Vec<_>, _>>()?;
        self.keyring
            .encrypt(plaintext, &recipients, MAX_CIPHERTEXT_BYTES)
            .map_err(cipher_error)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<SensitiveBytes, CipherError> {
        if ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(CipherError::Backend(anyhow::anyhow!(
                "ciphertext exceeds the size limit"
            )));
        }
        self.keyring
            .decrypt(ciphertext, self.unlock, MAX_CONTENT_BYTES)
            .map_err(cipher_error)
    }

    fn secret_keys(&self) -> Result<Vec<KeyFingerprint>, CipherError> {
        let keys = self.keyring.list_secret_keys().map_err(cipher_error)?;
        Ok(keys
            .iter()
            .filter(|key| key.has_secret)
            .filter_map(|key| core_fingerprint(&key.fingerprint))
            .collect())
    }
}

/// Both types hold a validated, uppercase fingerprint; the parse cannot fail
/// unless one of them changes its rules.
fn fingerprint(recipient: &KeyFingerprint) -> Result<Fingerprint, CipherError> {
    Fingerprint::parse(recipient.as_str())
        .map_err(|error| CipherError::Backend(anyhow::Error::new(error)))
}

fn core_fingerprint(fingerprint: &Fingerprint) -> Option<KeyFingerprint> {
    KeyFingerprint::parse(fingerprint.as_str()).ok()
}

/// Cases the vault tells apart become variants; the rest keep their cause
/// as the source of a backend error.
fn cipher_error(error: Error) -> CipherError {
    match &error {
        Error::OwnKeyNotTrusted(key) => match core_fingerprint(key) {
            Some(key) => CipherError::UntrustedRecipient(key),
            None => CipherError::Backend(error.into()),
        },
        Error::KeyNotFound(key)
        | Error::KeyUnusable(key)
        | Error::KeyExpired(key)
        | Error::CannotEncrypt(key) => match core_fingerprint(key) {
            Some(key) => CipherError::UnusableRecipient(key),
            None => CipherError::Backend(error.into()),
        },
        Error::NoSecretKey => CipherError::NotARecipient,
        Error::Locked | Error::BadPassphrase => CipherError::KeyUnavailable,
        Error::Canceled => CipherError::Canceled,
        _ => CipherError::Backend(error.into()),
    }
}
