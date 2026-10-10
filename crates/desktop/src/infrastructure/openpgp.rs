//! OpenPGP operations backed by the user's GnuPG keyring.

use anyhow::{Result, bail};
use gpgme::{Context, Data, EncryptFlags, Key, PinentryMode, Protocol};
use kryptopass_core::{KeyFingerprint, MAX_CONTENT_BYTES, OpenPgp, RecipientSet};
use std::io::{self, Write};
use zeroize::Zeroizing;

use super::MAX_CIPHERTEXT_BYTES;

/// GPGME adapter that creates an isolated context for each operation.
#[derive(Debug, Default, Clone, Copy)]
pub struct GnuPgOpenPgp;

impl OpenPgp for GnuPgOpenPgp {
    fn validate_recipients(&self, recipients: &RecipientSet) -> Result<()> {
        if recipients.as_slice().is_empty() {
            bail!("at least one OpenPGP recipient is required");
        }

        let mut context = open_context()?;
        for fingerprint in recipients.as_slice() {
            load_encryption_key(&mut context, fingerprint)?;
        }

        Ok(())
    }

    fn encrypt(&self, plaintext: &[u8], recipients: &RecipientSet) -> Result<Vec<u8>> {
        ensure_payload_size(plaintext.len(), MAX_CONTENT_BYTES, "secret content")?;
        if recipients.as_slice().is_empty() {
            bail!("at least one OpenPGP recipient is required");
        }

        let mut context = open_context()?;
        let keys = recipients
            .as_slice()
            .iter()
            .map(|fingerprint| load_encryption_key(&mut context, fingerprint))
            .collect::<Result<Vec<_>>>()?;

        let mut ciphertext = BoundedWriter::new(MAX_CIPHERTEXT_BYTES);
        let mut output = Data::from_writer(&mut ciphertext)
            .map_err(|_| anyhow::anyhow!("could not prepare bounded OpenPGP output"))?;
        let encryption_result = context.encrypt_with_flags(
            keys.iter(),
            plaintext,
            &mut output,
            EncryptFlags::NO_ENCRYPT_TO,
        );
        drop(output);
        let result = encryption_result.map_err(|_| anyhow::anyhow!("OpenPGP encryption failed"))?;

        if result.invalid_recipients().next().is_some() || ciphertext.is_empty() {
            bail!("OpenPGP encryption did not accept every selected recipient");
        }

        Ok(ciphertext.into_bytes())
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        if ciphertext.is_empty() {
            bail!("OpenPGP ciphertext is empty");
        }

        ensure_payload_size(ciphertext.len(), MAX_CIPHERTEXT_BYTES, "encrypted payload")?;

        let mut context = open_context()?;
        let mut plaintext = BoundedWriter::with_capacity(MAX_CONTENT_BYTES, MAX_CONTENT_BYTES);
        let mut output = Data::from_writer(&mut plaintext)
            .map_err(|_| anyhow::anyhow!("could not prepare bounded OpenPGP output"))?;
        context
            .decrypt(ciphertext, &mut output)
            .map_err(|_| anyhow::anyhow!("OpenPGP decryption failed"))?;
        drop(output);

        Ok(Zeroizing::new(plaintext.into_bytes()))
    }
}

fn ensure_payload_size(size: usize, maximum: usize, label: &str) -> Result<()> {
    if size > maximum {
        bail!("{label} exceeds the maximum size");
    }
    Ok(())
}

/// A GPGME output sink that refuses to retain more than its configured limit.
/// Its buffer is zeroized on errors and when ownership is not transferred.
struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}

impl BoundedWriter {
    fn new(maximum: usize) -> Self {
        Self::with_capacity(0, maximum)
    }

    fn with_capacity(capacity: usize, maximum: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity.min(maximum)),
            maximum,
        }
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn into_bytes(mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let new_length =
            self.bytes.len().checked_add(buffer.len()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "output size overflow")
            })?;
        if new_length > self.maximum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OpenPGP output exceeds the maximum size",
            ));
        }

        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for BoundedWriter {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.bytes.zeroize();
    }
}

fn open_context() -> Result<Context> {
    let mut context = Context::from_protocol(Protocol::OpenPgp)
        .map_err(|_| anyhow::anyhow!("could not initialize OpenPGP"))?;
    context
        .set_pinentry_mode(PinentryMode::Ask)
        .map_err(|_| anyhow::anyhow!("could not configure OpenPGP pinentry"))?;

    Ok(context)
}

fn load_encryption_key(context: &mut Context, fingerprint: &KeyFingerprint) -> Result<Key> {
    let key = context
        .get_key(fingerprint.as_str())
        .map_err(|_| anyhow::anyhow!("selected OpenPGP recipient is unavailable"))?;

    let stored_fingerprint = key
        .fingerprint()
        .map_err(|_| anyhow::anyhow!("selected OpenPGP recipient has an invalid identity"))?;

    if !stored_fingerprint.eq_ignore_ascii_case(fingerprint.as_str())
        || key.protocol() != Protocol::OpenPgp
        || key.is_bad()
        || !key.can_encrypt()
    {
        bail!("selected OpenPGP recipient cannot encrypt");
    }

    Ok(key)
}
