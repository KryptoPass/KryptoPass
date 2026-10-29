//! Encrypting to recipients and decrypting with the local secret keys.
//!
//! Output is capped while GnuPG writes it, so a hostile ciphertext cannot make
//! the process buffer more than the caller allows; plaintext goes straight
//! into protected memory. Ciphertexts without integrity protection or with
//! algorithms GnuPG flags as unsupported are refused even when they decrypt.

use std::io::{self, Write};

use gpgme::{Data, EncryptFlags, Key, PinentryMode};
use kryptopass_secure_memory::SensitiveBytes;
use tracing::{debug, instrument, warn};

use crate::backend::Session;
use crate::error::{Error, InvalidInput, Result, Step, Unconfirmed, gpg};
use crate::keyring::Keyring;
use crate::types::Fingerprint;

/// Most recipients one encryption accepts.
const MAX_RECIPIENTS: usize = 64;

/// Whether decryption may ask the person for a passphrase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unlock {
    /// gpg-agent may open pinentry.
    Ask,
    /// Only a passphrase gpg-agent already caches; never prompts. For
    /// background work, which must not pop dialogs.
    Cached,
}

impl Keyring {
    /// Checks that `recipient` can receive encrypted data: present, valid,
    /// and with an encryption subkey that is not revoked or expired.
    #[instrument(level = "debug", skip(self), fields(recipient = %recipient.key_id()))]
    pub fn check_recipient(&self, recipient: &Fingerprint) -> Result<()> {
        let mut session = self.session()?;
        encryption_key(&mut session, recipient).map(drop)
    }

    /// Encrypts to every recipient and to no one else: GnuPG's configured
    /// `encrypt-to` keys are ignored, so the recipients are exactly the list.
    #[instrument(
        level = "debug",
        skip(self, plaintext, recipients),
        fields(bytes = plaintext.len(), recipients = recipients.len())
    )]
    pub fn encrypt(
        &self,
        plaintext: &[u8],
        recipients: &[Fingerprint],
        max_ciphertext: usize,
    ) -> Result<Vec<u8>> {
        if recipients.is_empty() || recipients.len() > MAX_RECIPIENTS {
            return Err(InvalidInput::RecipientCount.into());
        }
        let mut session = self.session()?;
        let keys = recipients
            .iter()
            .map(|recipient| encryption_key(&mut session, recipient))
            .collect::<Result<Vec<_>>>()?;

        let mut output = Capped::new(Vec::new(), max_ciphertext);
        let outcome = {
            let mut data = Data::from_writer(&mut output)
                .map_err(|error| gpg(Step::Encrypt)(error.error()))?;
            session.context.encrypt_with_flags(
                keys.iter(),
                plaintext,
                &mut data,
                EncryptFlags::NO_ENCRYPT_TO,
            )
        };
        if output.overflowed {
            return Err(Error::TooLarge);
        }
        let result = match outcome {
            Ok(result) => result,
            Err(error) => return Err(refusal(&keys).unwrap_or_else(|| gpg(Step::Encrypt)(error))),
        };
        if let Some(invalid) = result.invalid_recipients().next() {
            let refused = invalid
                .fingerprint()
                .ok()
                .and_then(|raw| Fingerprint::parse(raw).ok());
            return Err(refusal(&keys).unwrap_or(match refused {
                Some(fingerprint) => Error::CannotEncrypt(fingerprint),
                None => Error::Unconfirmed(Unconfirmed::EmptyOutput),
            }));
        }
        let ciphertext = output.inner;
        if ciphertext.is_empty() {
            return Err(Error::Unconfirmed(Unconfirmed::EmptyOutput));
        }
        debug!(bytes = ciphertext.len(), "Keyring: encrypted");
        Ok(ciphertext)
    }

    /// Decrypts into protected memory, refusing more than `max_plaintext`.
    pub fn decrypt(
        &self,
        ciphertext: &[u8],
        unlock: Unlock,
        max_plaintext: usize,
    ) -> Result<SensitiveBytes> {
        let mut plaintext = SensitiveBytes::new();
        self.decrypt_into(ciphertext, unlock, &mut plaintext, max_plaintext)?;
        Ok(plaintext)
    }

    /// Decrypts into `output`, such as a buffer the caller keeps locked in
    /// memory. On error the output may hold part of the plaintext; the
    /// caller wipes it.
    #[instrument(
        level = "debug",
        skip(self, ciphertext, output),
        fields(bytes = ciphertext.len(), unlock = ?unlock)
    )]
    pub fn decrypt_into<W: Write + Send>(
        &self,
        ciphertext: &[u8],
        unlock: Unlock,
        output: W,
        max_plaintext: usize,
    ) -> Result<()> {
        if ciphertext.is_empty() {
            return Err(InvalidInput::EmptyCiphertext.into());
        }
        let mut session = self.session()?;
        if unlock == Unlock::Cached {
            session
                .context
                .set_pinentry_mode(PinentryMode::Error)
                .map_err(gpg(Step::Decrypt))?;
        }
        let mut capped = Capped::new(output, max_plaintext);
        let outcome = {
            let mut data = Data::from_writer(&mut capped)
                .map_err(|error| gpg(Step::Decrypt)(error.error()))?;
            session.context.decrypt(ciphertext, &mut data)
        };
        if capped.overflowed {
            return Err(Error::TooLarge);
        }
        let result = outcome.map_err(|error| decryption_error(error, unlock))?;
        if result.is_legacy_cipher_no_mdc() {
            return Err(Error::UnprotectedCiphertext);
        }
        if result.unsupported_algorithm_raw().is_some() {
            return Err(Error::UnsupportedAlgorithm);
        }
        debug!(bytes = capped.written, "Keyring: decrypted");
        Ok(())
    }
}

impl Keyring {
    /// Makes gpg-agent forget the cached passphrase of every subkey of
    /// `fingerprint`, so the next decryption asks again. The cache is keyed
    /// by keygrip, so this affects every application using the key.
    #[instrument(level = "debug", skip(self), fields(key = %fingerprint.key_id()))]
    pub fn forget_passphrase(&self, fingerprint: &Fingerprint) -> Result<()> {
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        let keygrips: Vec<String> = key
            .subkeys()
            .filter_map(|subkey| subkey.keygrip().ok().map(str::to_owned))
            .filter(|keygrip| {
                keygrip.len() == 40 && keygrip.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .collect();
        if keygrips.is_empty() {
            return Err(Error::NoSecretMaterial(fingerprint.clone()));
        }
        self.gpg(&session).clear_passphrases(&keygrips)?;
        Ok(())
    }
}

/// The key of `fingerprint` if it can be encrypted to.
fn encryption_key(session: &mut Session, fingerprint: &Fingerprint) -> Result<Key> {
    let key = session.require(fingerprint)?;
    if key.is_revoked() || key.is_disabled() || key.is_invalid() {
        return Err(Error::KeyUnusable(fingerprint.clone()));
    }
    if key.is_expired() {
        return Err(Error::KeyExpired(fingerprint.clone()));
    }
    let usable = key.can_encrypt()
        && key
            .subkeys()
            .any(|subkey| subkey.can_encrypt() && !subkey.is_bad());
    if usable {
        Ok(key)
    } else {
        Err(Error::CannotEncrypt(fingerprint.clone()))
    }
}

/// GnuPG refuses to encrypt to the person's own key until it is marked as
/// theirs (ultimate trust). That is the usual cause and the one the keyring
/// manager can fix, so it is named instead of a generic failure.
fn refusal(keys: &[Key]) -> Option<Error> {
    let untrusted = keys
        .iter()
        .find(|key| key.has_secret() && key.owner_trust() != gpgme::Validity::Ultimate)?;
    let fingerprint = crate::backend::primary_fingerprint(untrusted)?;
    warn!(key = %fingerprint.key_id(), "Keyring: own key is not trusted as the user's");
    Some(Error::OwnKeyNotTrusted(fingerprint))
}

fn decryption_error(error: gpgme::Error, unlock: Unlock) -> Error {
    let code = error.code();
    if code == gpgme::Error::CANCELED.code() {
        Error::Canceled
    } else if code == gpgme::Error::NO_SECKEY.code() {
        Error::NoSecretKey
    } else if unlock == Unlock::Cached
        && (code == gpgme::Error::NO_PIN_ENTRY.code()
            || code == gpgme::Error::BAD_PASSPHRASE.code())
    {
        Error::Locked
    } else if code == gpgme::Error::BAD_PASSPHRASE.code() {
        Error::BadPassphrase
    } else {
        gpg(Step::Decrypt)(error)
    }
}

/// A writer that refuses to pass on more than `max` bytes and remembers it,
/// so the caller can report the limit instead of GPGME's generic I/O error.
struct Capped<W> {
    inner: W,
    max: usize,
    written: usize,
    overflowed: bool,
}

impl<W> Capped<W> {
    fn new(inner: W, max: usize) -> Self {
        Self {
            inner,
            max,
            written: 0,
            overflowed: false,
        }
    }
}

impl<W: Write> Write for Capped<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let total = self.written.saturating_add(bytes.len());
        if total > self.max {
            self.overflowed = true;
            return Err(io::Error::other("output exceeds the size limit"));
        }
        self.inner.write_all(bytes)?;
        self.written = total;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
