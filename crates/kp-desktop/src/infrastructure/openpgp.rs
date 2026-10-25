//! OpenPGP operations backed by the user's GnuPG keyring.

use anyhow::{Result, bail};
use gpgme::{
    Context, Data, EncryptFlags, ExportMode, Key, PassphraseRequest, PinentryMode, Protocol,
};
use kryptopass_core::{KeyFingerprint, MAX_CONTENT_BYTES, OpenPgp, RecipientSet};
use kryptopass_secure_memory::{ProtectedBytes, SensitiveBytes};
#[cfg(debug_assertions)]
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{self, Write};
#[cfg(debug_assertions)]
use std::{fs, path::PathBuf};
#[cfg(debug_assertions)]
use zeroize::Zeroize;

use super::MAX_CIPHERTEXT_BYTES;

const MAX_BOUNDED_INITIAL_CAPACITY: usize = 64 * 1024;

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/openpgp.rs"]
mod tests;

/// GPGME adapter that creates an isolated context for each operation.
#[derive(Debug, Default, Clone, Copy)]
pub struct GnuPgOpenPgp;

impl GnuPgOpenPgp {
    /// Background indexing cannot prompt or retain output in an unlocked region.
    pub(crate) fn decrypt_for_search(
        &self,
        ciphertext: &[u8],
        plaintext: &mut ProtectedBytes,
    ) -> Result<()> {
        plaintext.clear();
        let mut context = open_context()?;
        context
            .set_pinentry_mode(PinentryMode::Error)
            .map_err(|_| anyhow::anyhow!("could not configure background decryption"))?;
        let mut output = SearchOutput {
            bytes: plaintext,
            written: 0,
        };
        decrypt_into(
            &mut context,
            ciphertext,
            MAX_CIPHERTEXT_BYTES,
            &mut output,
            None,
        )
    }
}

struct SearchOutput<'a> {
    bytes: &'a mut ProtectedBytes,
    written: usize,
}

impl Write for SearchOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let length = self
            .written
            .checked_add(bytes.len())
            .filter(|length| *length <= MAX_CONTENT_BYTES)
            .ok_or_else(|| io::Error::other("background plaintext limit exceeded"))?;
        if length > self.bytes.capacity() {
            let capacity = length
                .max(self.bytes.capacity().saturating_mul(2))
                .min(MAX_CONTENT_BYTES);
            let mut replacement = ProtectedBytes::try_with_capacity(capacity)?;
            replacement.try_extend_from_slice(self.bytes.as_slice())?;
            // Both reservations are independently protected during replacement.
            *self.bytes = replacement;
        }
        self.bytes.try_extend_from_slice(bytes)?;
        self.written = length;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

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
        encrypt_bounded_in_context(
            &mut open_context()?,
            plaintext,
            recipients,
            MAX_CONTENT_BYTES,
            MAX_CIPHERTEXT_BYTES,
            EncryptFlags::NO_ENCRYPT_TO,
        )
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<SensitiveBytes> {
        decrypt_bounded_in_context(
            &mut open_context()?,
            ciphertext,
            MAX_CIPHERTEXT_BYTES,
            MAX_CONTENT_BYTES,
            None,
        )
    }
}

/// GnuPG adapter for background work. It never opens pinentry, so decryption
/// succeeds only while gpg-agent still caches the passphrase, as it does while
/// the user keeps the vault unlocked.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct NonInteractiveGnuPgOpenPgp;

impl OpenPgp for NonInteractiveGnuPgOpenPgp {
    fn validate_recipients(&self, recipients: &RecipientSet) -> Result<()> {
        GnuPgOpenPgp.validate_recipients(recipients)
    }

    fn encrypt(&self, plaintext: &[u8], recipients: &RecipientSet) -> Result<Vec<u8>> {
        GnuPgOpenPgp.encrypt(plaintext, recipients)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<SensitiveBytes> {
        let mut context = open_context()?;
        context
            .set_pinentry_mode(PinentryMode::Error)
            .map_err(|_| anyhow::anyhow!("could not configure background decryption"))?;
        decrypt_bounded_in_context(
            &mut context,
            ciphertext,
            MAX_CIPHERTEXT_BYTES,
            MAX_CONTENT_BYTES,
            None,
        )
    }
}

/// OpenPGP adapter pinned to one private, isolated keyring directory.
/// Used only by the public synthetic Sync fixture; never points at the user's
/// normal GnuPG home.
#[cfg(debug_assertions)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SyntheticFixtureManifest {
    fixture_id: String,
    fingerprint: String,
    sha256: String,
    size: usize,
    passphrase: String,
    warning: String,
}

#[cfg(debug_assertions)]
pub(crate) struct SyntheticFixture {
    fingerprint: KeyFingerprint,
    fingerprint_bytes: Vec<u8>,
    secret_key_bytes: Vec<u8>,
    passphrase: Vec<u8>,
}

/// Backend-only material for one key transfer: the synthetic fixture or the
/// vault identity. This type intentionally has no `Debug` implementation and
/// is never exposed through a CXX/QML property.
pub(crate) struct SyntheticTransferMaterial {
    pub(crate) fingerprint: Vec<u8>,
    pub(crate) protected_secret_key: zeroize::Zeroizing<Vec<u8>>,
    pub(crate) encrypted_challenge: Vec<u8>,
    pub(crate) challenge_sha256: [u8; 32],
}

/// Largest protected secret key accepted for a transfer, equal to the wire limit.
const MAX_TRANSFER_KEY_BYTES: usize = 1024 * 1024;
/// Largest encrypted challenge accepted by the offer.
const MAX_TRANSFER_CHALLENGE_BYTES: usize = 16 * 1024;

/// Exports the protected secret key of the selected identity from the user's
/// GnuPG keyring and encrypts a fresh challenge to it.
///
/// The cached passphrase is forgotten first, so gpg-agent always asks for it in
/// pinentry before exporting: the export needs the owner at this desk even
/// while the vault is unlocked. Afterwards the vault asks for the passphrase
/// again on its next decryption. The exported block stays protected by that
/// same passphrase; no temporary export file is created.
pub(crate) fn prepare_private_key_transfer(
    fingerprint: &KeyFingerprint,
) -> Result<SyntheticTransferMaterial> {
    let fingerprint_bytes = decode_fingerprint(fingerprint.as_str())?;
    if fingerprint_bytes.len() != 20 {
        bail!("only OpenPGP v4 identities can be sent to the phone");
    }
    let mut context = open_context()?;
    let key = context
        .get_secret_key(fingerprint.as_str())
        .map_err(|_| anyhow::anyhow!("the selected identity has no secret key in GnuPG"))?;
    let stored = key
        .fingerprint()
        .map_err(|_| anyhow::anyhow!("the selected identity is invalid"))?;
    if !stored.eq_ignore_ascii_case(fingerprint.as_str())
        || key.protocol() != Protocol::OpenPgp
        || key.is_bad()
        || key.is_invalid()
        || !key.can_encrypt()
        || !key
            .subkeys()
            .any(|subkey| subkey.can_encrypt() && !subkey.is_bad())
    {
        bail!("the selected identity cannot be sent");
    }
    // Fail closed: without a fresh prompt the export would not prove presence.
    GnuPgOpenPgp.forget_cached_passphrase(fingerprint)?;

    let mut exported = BoundedWriter::with_capacity(MAX_TRANSFER_KEY_BYTES, MAX_TRANSFER_KEY_BYTES);
    {
        let mut output = Data::from_writer(&mut exported)
            .map_err(|_| anyhow::anyhow!("could not prepare the key export"))?;
        context
            .export_keys([&key], ExportMode::SECRET, &mut output)
            .map_err(|_| anyhow::anyhow!("GnuPG did not export the selected identity"))?;
    }
    if exported.is_empty() {
        bail!("GnuPG did not export the selected identity");
    }
    let protected_secret_key = zeroize::Zeroizing::new(exported.into_bytes());

    let mut challenge = zeroize::Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut challenge[..])
        .map_err(|_| anyhow::anyhow!("could not create the transfer challenge"))?;
    let recipients = RecipientSet::new(vec![fingerprint.clone()])
        .map_err(|_| anyhow::anyhow!("the selected identity is not a valid recipient"))?;
    let encrypted_challenge = encrypt_bounded_in_context(
        &mut context,
        &challenge[..],
        &recipients,
        challenge.len(),
        MAX_TRANSFER_CHALLENGE_BYTES,
        EncryptFlags::NO_ENCRYPT_TO,
    )?;
    let challenge_sha256 = Sha256::digest(&challenge[..]).into();

    Ok(SyntheticTransferMaterial {
        fingerprint: fingerprint_bytes,
        protected_secret_key,
        encrypted_challenge,
        challenge_sha256,
    })
}

/// Returns the validated public identity of the local debug fixture.
/// The private fixture bytes are checked and zeroized before returning.
#[cfg(debug_assertions)]
pub(crate) fn synthetic_transfer_fingerprint() -> Result<String> {
    let fixture = SyntheticFixture::load()?;
    Ok(fixture.fingerprint().as_str().to_owned())
}

/// Loads the private synthetic fixture, prepares a protected-key export, and
/// encrypts a fresh challenge to that exact key in its isolated GnuPG home.
/// The secret key stays in zeroizing backend memory until the transfer owner
/// takes it; no temporary export file is created.
#[cfg(debug_assertions)]
pub(crate) fn prepare_synthetic_transfer(
    expected_fingerprint: &str,
) -> Result<SyntheticTransferMaterial> {
    let fixture = SyntheticFixture::load()?;
    if !fixture
        .fingerprint()
        .as_str()
        .eq_ignore_ascii_case(expected_fingerprint)
    {
        bail!("synthetic OpenPGP fixture changed after local approval");
    }
    let fingerprint = fixture.fingerprint_bytes().to_vec();
    let engine_home =
        PathBuf::from(env!("KP_SYNTHETIC_FIXTURE_DIR")).join("synthetic-transfer-openpgp-v1");
    let fingerprint_key = fixture.fingerprint().clone();
    let (crypto, protected_secret_key) =
        IsolatedGnuPgOpenPgp::open_synthetic_fixture_for_transfer(engine_home, fixture)?;

    let mut challenge = zeroize::Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut challenge[..])
        .map_err(|_| anyhow::anyhow!("could not create synthetic OpenPGP challenge"))?;
    let recipients = RecipientSet::new(vec![fingerprint_key])
        .map_err(|_| anyhow::anyhow!("synthetic OpenPGP recipient is invalid"))?;
    let encrypted_challenge = crypto.encrypt(&challenge[..], &recipients)?;
    let challenge_sha256 = Sha256::digest(&challenge[..]).into();

    Ok(SyntheticTransferMaterial {
        fingerprint,
        protected_secret_key,
        encrypted_challenge,
        challenge_sha256,
    })
}

#[cfg(debug_assertions)]
impl SyntheticFixture {
    pub(crate) fn load() -> Result<Self> {
        let directory = PathBuf::from(env!("KP_SYNTHETIC_FIXTURE_DIR"));
        ensure_private_fixture_directory(&directory)?;
        let manifest_path = PathBuf::from(env!("KP_SYNTHETIC_FIXTURE_MANIFEST"));
        let key_path = directory.join("secret-key.pgp");
        let manifest_bytes = zeroize::Zeroizing::new(read_private_fixture_file(&manifest_path)?);
        let mut key_bytes = read_private_fixture_file(&key_path)?;
        let mut manifest: SyntheticFixtureManifest = match serde_json::from_slice(&manifest_bytes) {
            Ok(manifest) => manifest,
            Err(_) => {
                key_bytes.zeroize();
                bail!("synthetic OpenPGP fixture metadata is invalid");
            }
        };
        if manifest.fixture_id != "kp-synthetic-key-v1"
            || !manifest
                .warning
                .to_ascii_uppercase()
                .contains("PUBLIC SYNTHETIC")
            || !manifest.warning.to_ascii_uppercase().contains("NEVER USE")
            || manifest.size != key_bytes.len()
            || manifest.sha256.len() != 64
            || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || format!("{:x}", Sha256::digest(&key_bytes)) != manifest.sha256.to_ascii_lowercase()
            || manifest.passphrase.is_empty()
        {
            key_bytes.zeroize();
            manifest.passphrase.zeroize();
            bail!("synthetic OpenPGP fixture metadata does not match its key");
        }
        let fingerprint = match KeyFingerprint::parse(&manifest.fingerprint) {
            Ok(fingerprint) => fingerprint,
            Err(_) => {
                key_bytes.zeroize();
                manifest.passphrase.zeroize();
                bail!("synthetic OpenPGP fixture fingerprint is invalid");
            }
        };
        let fingerprint_bytes = match decode_fingerprint(fingerprint.as_str()) {
            Ok(bytes) => bytes,
            Err(error) => {
                key_bytes.zeroize();
                manifest.passphrase.zeroize();
                return Err(error);
            }
        };
        Ok(Self {
            fingerprint,
            fingerprint_bytes,
            secret_key_bytes: key_bytes,
            passphrase: manifest.passphrase.into_bytes(),
        })
    }

    pub(crate) fn fingerprint(&self) -> &KeyFingerprint {
        &self.fingerprint
    }

    pub(crate) fn fingerprint_bytes(&self) -> &[u8] {
        &self.fingerprint_bytes
    }
}

#[cfg(debug_assertions)]
impl Drop for SyntheticFixture {
    fn drop(&mut self) {
        self.secret_key_bytes.zeroize();
        self.passphrase.zeroize();
    }
}

#[cfg(debug_assertions)]
pub(crate) struct IsolatedGnuPgOpenPgp {
    engine_home: PathBuf,
    passphrase: Vec<u8>,
}

#[cfg(debug_assertions)]
impl IsolatedGnuPgOpenPgp {
    pub(crate) fn open_synthetic_fixture(
        engine_home: PathBuf,
        fixture: SyntheticFixture,
    ) -> Result<Self> {
        let (crypto, protected_secret_key) =
            Self::open_synthetic_fixture_for_transfer(engine_home, fixture)?;
        drop(protected_secret_key);
        Ok(crypto)
    }

    fn open_synthetic_fixture_for_transfer(
        engine_home: PathBuf,
        mut fixture: SyntheticFixture,
    ) -> Result<(Self, zeroize::Zeroizing<Vec<u8>>)> {
        ensure_private_engine_home(&engine_home)?;
        let mut context = open_context_at(&engine_home, PinentryMode::Loopback)?;
        let protected_secret_key =
            zeroize::Zeroizing::new(std::mem::take(&mut fixture.secret_key_bytes));
        let key = match context.get_secret_key(fixture.fingerprint.as_str()) {
            Ok(key) => key,
            Err(_) => {
                let mut key_data = Data::from_bytes(protected_secret_key.as_slice())
                    .map_err(|_| anyhow::anyhow!("could not load synthetic OpenPGP fixture"))?;
                context
                    .import(&mut key_data)
                    .map_err(|_| anyhow::anyhow!("could not import synthetic OpenPGP fixture"))?;
                context
                    .get_secret_key(fixture.fingerprint.as_str())
                    .map_err(|_| anyhow::anyhow!("synthetic OpenPGP key is unavailable"))?
            }
        };
        let fingerprint = key
            .fingerprint()
            .map_err(|_| anyhow::anyhow!("synthetic OpenPGP key identity is invalid"))?;
        if !fingerprint.eq_ignore_ascii_case(fixture.fingerprint.as_str())
            || key.protocol() != Protocol::OpenPgp
            || key.is_bad()
            || key.is_revoked()
            || key.is_expired()
            || key.is_disabled()
            || !key.can_encrypt()
            || !key
                .subkeys()
                .any(|subkey| subkey.can_encrypt() && !subkey.is_bad())
        {
            bail!("synthetic OpenPGP key does not match the public fixture");
        }
        Ok((
            Self {
                engine_home,
                passphrase: std::mem::take(&mut fixture.passphrase),
            },
            protected_secret_key,
        ))
    }

    fn open_context(&self) -> Result<Context> {
        open_context_at(&self.engine_home, PinentryMode::Loopback)
    }
}

#[cfg(debug_assertions)]
impl OpenPgp for IsolatedGnuPgOpenPgp {
    fn validate_recipients(&self, recipients: &RecipientSet) -> Result<()> {
        if recipients.as_slice().is_empty() {
            bail!("at least one OpenPGP recipient is required");
        }
        let mut context = self.open_context()?;
        for fingerprint in recipients.as_slice() {
            load_encryption_key(&mut context, fingerprint)?;
        }
        Ok(())
    }

    fn encrypt(&self, plaintext: &[u8], recipients: &RecipientSet) -> Result<Vec<u8>> {
        encrypt_bounded_in_context(
            &mut self.open_context()?,
            plaintext,
            recipients,
            MAX_CONTENT_BYTES,
            MAX_CIPHERTEXT_BYTES,
            // This isolated home contains only the exact public fixture pinned above.
            // Its imported certificate has no ownertrust; normal keyrings keep their policy.
            EncryptFlags::NO_ENCRYPT_TO | EncryptFlags::ALWAYS_TRUST,
        )
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<SensitiveBytes> {
        decrypt_bounded_in_context(
            &mut self.open_context()?,
            ciphertext,
            MAX_CIPHERTEXT_BYTES,
            MAX_CONTENT_BYTES,
            Some(&self.passphrase),
        )
    }
}

#[cfg(debug_assertions)]
impl Drop for IsolatedGnuPgOpenPgp {
    fn drop(&mut self) {
        self.passphrase.zeroize();
    }
}

impl GnuPgOpenPgp {
    /// Local keys with a secret part that can still encrypt; no passphrase is requested.
    pub(crate) fn usable_secret_fingerprints(&self) -> Result<Vec<KeyFingerprint>> {
        let mut context = open_context()?;
        let keys = context
            .secret_keys()
            .map_err(|_| anyhow::anyhow!("could not list OpenPGP secret keys"))?;
        let mut fingerprints = Vec::new();
        for key in keys.flatten() {
            if key.is_revoked()
                || key.is_expired()
                || key.is_disabled()
                || key.is_invalid()
                || !key.can_encrypt()
            {
                continue;
            }
            if let Some(fingerprint) = key
                .fingerprint()
                .ok()
                .and_then(|value| KeyFingerprint::parse(value).ok())
            {
                fingerprints.push(fingerprint);
            }
        }
        Ok(fingerprints)
    }

    /// Asks gpg-agent to forget the cached passphrase of every subkey of `fingerprint`.
    ///
    /// The cache is keyed by keygrip, so this affects every application that uses the key.
    pub(crate) fn forget_cached_passphrase(&self, fingerprint: &KeyFingerprint) -> Result<()> {
        let mut context = open_context()?;
        let key = context
            .get_secret_key(fingerprint.as_str())
            .map_err(|_| anyhow::anyhow!("the primary secret key is not in the keyring"))?;
        let keygrips: Vec<String> = key
            .subkeys()
            .filter_map(|subkey| subkey.keygrip().ok().map(str::to_owned))
            .filter(|keygrip| {
                keygrip.len() == 40 && keygrip.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .collect();
        if keygrips.is_empty() {
            bail!("the primary key has no keygrip to forget");
        }
        let mut command = std::process::Command::new("gpg-connect-agent");
        for keygrip in &keygrips {
            command.arg(format!("CLEAR_PASSPHRASE --mode=normal {keygrip}"));
        }
        let status = command
            .arg("/bye")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|_| anyhow::anyhow!("could not reach gpg-agent"))?;
        if !status.success() {
            bail!("gpg-agent did not accept the request to forget the passphrase");
        }
        Ok(())
    }

    pub(crate) fn encrypt_bounded(
        &self,
        plaintext: &[u8],
        recipients: &RecipientSet,
        maximum_plaintext: usize,
        maximum_ciphertext: usize,
    ) -> Result<Vec<u8>> {
        encrypt_bounded_in_context(
            &mut open_context()?,
            plaintext,
            recipients,
            maximum_plaintext,
            maximum_ciphertext,
            EncryptFlags::NO_ENCRYPT_TO,
        )
    }

    pub(crate) fn decrypt_bounded(
        &self,
        ciphertext: &[u8],
        maximum_ciphertext: usize,
        maximum_plaintext: usize,
    ) -> Result<SensitiveBytes> {
        decrypt_bounded_in_context(
            &mut open_context()?,
            ciphertext,
            maximum_ciphertext,
            maximum_plaintext,
            None,
        )
    }
}

struct SensitiveOutput {
    bytes: SensitiveBytes,
    maximum: usize,
}

impl Write for SensitiveOutput {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let length =
            self.bytes.len().checked_add(buffer.len()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "output size overflow")
            })?;
        if length > self.maximum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OpenPGP output exceeds the maximum size",
            ));
        }
        self.bytes.try_extend_from_slice(buffer)?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
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

#[cfg(debug_assertions)]
fn open_context_at(home: &std::path::Path, pinentry: PinentryMode) -> Result<Context> {
    let home = home
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("isolated OpenPGP path is not valid UTF-8"))?;
    let mut context = Context::from_protocol(Protocol::OpenPgp)
        .map_err(|_| anyhow::anyhow!("could not initialize isolated OpenPGP"))?;
    context
        .set_engine_home_dir(home)
        .map_err(|_| anyhow::anyhow!("could not isolate OpenPGP keyring"))?;
    context
        .set_pinentry_mode(pinentry)
        .map_err(|_| anyhow::anyhow!("could not configure isolated OpenPGP"))?;
    Ok(context)
}

#[cfg(debug_assertions)]
fn ensure_private_engine_home(path: &std::path::Path) -> Result<()> {
    use std::fs;

    if fs::symlink_metadata(path).is_err() {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(path)
            .map_err(|_| anyhow::anyhow!("could not create isolated OpenPGP home"))?;
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| anyhow::anyhow!("isolated OpenPGP home is unavailable"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("isolated OpenPGP home is not a real directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 {
            bail!("isolated OpenPGP home permissions are not private");
        }
    }
    Ok(())
}

fn encrypt_bounded_in_context(
    context: &mut Context,
    plaintext: &[u8],
    recipients: &RecipientSet,
    maximum_plaintext: usize,
    maximum_ciphertext: usize,
    flags: EncryptFlags,
) -> Result<Vec<u8>> {
    ensure_payload_size(plaintext.len(), maximum_plaintext, "secret content")?;
    if recipients.as_slice().is_empty() {
        bail!("at least one OpenPGP recipient is required");
    }
    let keys = recipients
        .as_slice()
        .iter()
        .map(|fingerprint| load_encryption_key(context, fingerprint))
        .collect::<Result<Vec<_>>>()?;

    let mut ciphertext = BoundedWriter::new(maximum_ciphertext);
    let mut output = Data::from_writer(&mut ciphertext)
        .map_err(|_| anyhow::anyhow!("could not prepare bounded OpenPGP output"))?;
    let encryption_result = context.encrypt_with_flags(keys.iter(), plaintext, &mut output, flags);
    drop(output);
    // The cause is rarely the plaintext: GnuPG refuses recipients it cannot use. Name the
    // keys that are the user's own but not trusted as such, since the keyring manager can
    // fix exactly that, and say so in the error instead of hiding it.
    let untrusted_own: Vec<String> = keys
        .iter()
        .filter(|key| key.has_secret() && key.owner_trust() != gpgme::Validity::Ultimate)
        .filter_map(|key| key.fingerprint().ok().map(str::to_owned))
        .collect();
    let refusal = |detail: String| -> anyhow::Error {
        if let Some(fingerprint) = untrusted_own.first() {
            tracing::warn!(
                key = %short_fingerprint(fingerprint),
                detail = %detail,
                "OpenPGP encryption refused: the key is here with its private part but GnuPG does not trust it as the user's own"
            );
            anyhow::anyhow!(
                "{}",
                crate::i18n::format(
                    "GnuPG no puede cifrar con la clave %1 porque no está marcada como tuya. Ábrela en Claves y pulsa «Marcar como mía».",
                    std::slice::from_ref(fingerprint),
                )
            )
        } else {
            tracing::warn!(detail = %detail, recipients = keys.len(), "OpenPGP encryption failed");
            anyhow::anyhow!("OpenPGP encryption failed: {detail}")
        }
    };
    let result = match encryption_result {
        Ok(result) => result,
        Err(error) => return Err(refusal(error.to_string())),
    };
    if let Some(invalid) = result.invalid_recipients().next() {
        let detail = match invalid.reason() {
            Some(error) => format!("recipient refused: {error}"),
            None => "recipient refused".to_owned(),
        };
        return Err(refusal(detail));
    }
    if ciphertext.is_empty() {
        return Err(refusal("empty ciphertext".to_owned()));
    }
    Ok(ciphertext.into_bytes())
}

fn decrypt_bounded_in_context(
    context: &mut Context,
    ciphertext: &[u8],
    maximum_ciphertext: usize,
    maximum_plaintext: usize,
    passphrase: Option<&[u8]>,
) -> Result<SensitiveBytes> {
    let mut plaintext = SensitiveOutput {
        bytes: SensitiveBytes::try_with_capacity(
            maximum_plaintext.min(MAX_BOUNDED_INITIAL_CAPACITY),
        )?,
        maximum: maximum_plaintext,
    };
    decrypt_into(
        context,
        ciphertext,
        maximum_ciphertext,
        &mut plaintext,
        passphrase,
    )?;
    Ok(plaintext.bytes)
}

fn decrypt_into<W: Write + Send>(
    context: &mut Context,
    ciphertext: &[u8],
    maximum_ciphertext: usize,
    plaintext: &mut W,
    passphrase: Option<&[u8]>,
) -> Result<()> {
    if ciphertext.is_empty() {
        bail!("OpenPGP ciphertext is empty");
    }
    ensure_payload_size(ciphertext.len(), maximum_ciphertext, "encrypted payload")?;
    let mut output = Data::from_writer(plaintext)
        .map_err(|_| anyhow::anyhow!("could not prepare bounded OpenPGP output"))?;
    let decrypted = if let Some(passphrase) = passphrase {
        context.with_passphrase_provider(
            move |_: PassphraseRequest<'_>, target: &mut dyn Write| {
                target.write_all(passphrase)?;
                Ok(())
            },
            |context| context.decrypt(ciphertext, &mut output),
        )
    } else {
        context.decrypt(ciphertext, &mut output)
    };
    let result = decrypted.map_err(|_| anyhow::anyhow!("OpenPGP decryption failed"))?;
    drop(output);

    if result.is_legacy_cipher_no_mdc() {
        bail!("legacy OpenPGP ciphertext without integrity protection is not accepted");
    }
    if result.unsupported_algorithm_raw().is_some() {
        bail!("OpenPGP ciphertext uses an unsupported algorithm");
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn ensure_private_fixture_directory(path: &std::path::Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| anyhow::anyhow!("synthetic OpenPGP fixture cache is unavailable"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("synthetic OpenPGP fixture cache is not a real directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o777 != 0o700 {
            bail!("synthetic OpenPGP fixture cache is not private");
        }
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn read_private_fixture_file(path: &std::path::Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| anyhow::anyhow!("synthetic OpenPGP fixture file is unavailable"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("synthetic OpenPGP fixture file is not a regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o777 != 0o600 {
            bail!("synthetic OpenPGP fixture file is not private");
        }
    }
    let maximum = match path.file_name().and_then(|name| name.to_str()) {
        Some("fixture.json") => 8 * 1024,
        Some("secret-key.pgp") => 1024 * 1024,
        _ => bail!("synthetic OpenPGP fixture filename is invalid"),
    };
    if metadata.len() == 0 || metadata.len() > maximum {
        bail!("synthetic OpenPGP fixture file size is invalid");
    }
    fs::read(path).map_err(|_| anyhow::anyhow!("could not read synthetic OpenPGP fixture file"))
}

fn decode_fingerprint(fingerprint: &str) -> Result<Vec<u8>> {
    fingerprint
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let byte = std::str::from_utf8(pair)
                .map_err(|_| anyhow::anyhow!("OpenPGP fingerprint is invalid"))?;
            u8::from_str_radix(byte, 16)
                .map_err(|_| anyhow::anyhow!("OpenPGP fingerprint is invalid"))
        })
        .collect()
}

fn load_encryption_key(context: &mut Context, fingerprint: &KeyFingerprint) -> Result<Key> {
    let key = context
        .get_key(fingerprint.as_str())
        .map_err(|_| anyhow::anyhow!("selected OpenPGP recipient is unavailable"))?;

    let stored_fingerprint = key
        .fingerprint()
        .map_err(|_| anyhow::anyhow!("selected OpenPGP recipient has an invalid identity"))?;

    let has_usable_encryption_subkey = key
        .subkeys()
        .any(|subkey| subkey.can_encrypt() && !subkey.is_bad());

    if !stored_fingerprint.eq_ignore_ascii_case(fingerprint.as_str())
        || key.protocol() != Protocol::OpenPgp
        || key.is_bad()
        || !key.can_encrypt()
        || !has_usable_encryption_subkey
    {
        bail!("selected OpenPGP recipient cannot encrypt");
    }

    Ok(key)
}

/// Receiving the vault identity from a paired phone (key transfer v3).
pub(crate) mod inbound_key;

/// Last 16 hex digits of a fingerprint: enough to tell keys apart in a log.
fn short_fingerprint(fingerprint: &str) -> &str {
    &fingerprint[fingerprint.len().saturating_sub(16)..]
}
