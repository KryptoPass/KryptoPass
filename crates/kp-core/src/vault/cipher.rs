//! Encryption boundary: who can read an item, and the bytes behind it.

use thiserror::Error;

use crate::SensitiveBytes;
use crate::domain::{KeyFingerprint, RecipientSet};

/// Why encryption or decryption did not happen.
#[derive(Debug, Error)]
pub enum CipherError {
    /// The recipient's key is missing, revoked, expired or cannot encrypt.
    #[error("recipient key is not usable")]
    UnusableRecipient(KeyFingerprint),
    /// The person's own key is not marked as theirs, so GnuPG refuses it;
    /// the keyring manager can fix that.
    #[error("own key is not trusted as the user's")]
    UntrustedRecipient(KeyFingerprint),
    /// A secret key that can decrypt is here but locked, or its passphrase
    /// was wrong.
    #[error("secret key is unavailable")]
    KeyUnavailable,
    /// None of the recipients' secret keys is on this device. Not a fault:
    /// the payload is kept as it is, for the devices that can read it.
    #[error("no secret key on this device can decrypt")]
    NotARecipient,
    /// The person dismissed the passphrase prompt.
    #[error("passphrase prompt was canceled")]
    Canceled,
    #[error("cipher backend failed")]
    Backend(#[source] anyhow::Error),
}

/// Encrypts item payloads to a set of recipients and decrypts them back.
pub trait Cipher {
    /// Fails with [`CipherError::UnusableRecipient`] or
    /// [`CipherError::UntrustedRecipient`] when a key cannot be used.
    fn check_recipients(&self, recipients: &RecipientSet) -> Result<(), CipherError>;

    fn encrypt(&self, plaintext: &[u8], recipients: &RecipientSet) -> Result<Vec<u8>, CipherError>;

    fn decrypt(&self, ciphertext: &[u8]) -> Result<SensitiveBytes, CipherError>;

    /// The keys whose secret part is on this device, by primary fingerprint.
    /// Any of them decrypts what was encrypted to it; none is special.
    fn secret_keys(&self) -> Result<Vec<KeyFingerprint>, CipherError>;
}
