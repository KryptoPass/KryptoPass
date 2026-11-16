//! Importing exports of other password managers.
//!
//! Three steps, so the person can review before anything is stored:
//! [`read_export`] reads and parses a file with a registered importer,
//! [`prepare_import`] converts its entries to template content, and
//! [`Vault::import`](kryptopass_core::vault::Vault::import) stores the batch.

mod convert;

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use kryptopass_core::SensitiveBytes;
use kryptopass_import::importers::proton::{OpenPgpDecryptor, ProtonPassImporter};
use kryptopass_import::{Detection, ImportError, ImportLimits, ImportedEntries, ImporterRegistry};
use kryptopass_keyring::{Keyring, Unlock};

use crate::error::AppError;

pub use convert::{Adaptations, PreparedImport, prepare_import};

/// Every importer the app offers. Encrypted exports are decrypted with
/// `keyring`, which may ask for the export's passphrase in pinentry.
pub fn importers(keyring: &Keyring) -> Result<ImporterRegistry, AppError> {
    let mut registry = ImporterRegistry::default();
    registry
        .register(Box::new(ProtonPassImporter::with_decryptor(
            KeyringDecryptor(keyring.clone()),
        )))
        .map_err(|_| AppError::Import(ImportError::Unsupported))?;
    Ok(registry)
}

/// Reads `path`, bounded by the import limits, and parses it with the
/// importer `importer_id`.
pub fn read_export(
    registry: &ImporterRegistry,
    importer_id: &str,
    path: &Path,
) -> Result<ImportedEntries, AppError> {
    let importer = registry
        .get(importer_id)
        .ok_or(AppError::Import(ImportError::Unsupported))?;
    let limits = ImportLimits::default();
    let file = File::open(path).map_err(|_| AppError::ImportFile)?;
    let metadata = file.metadata().map_err(|_| AppError::ImportFile)?;
    if !metadata.is_file() {
        return Err(AppError::Import(ImportError::InvalidExport));
    }
    if metadata.len() > limits.input_bytes {
        return Err(AppError::Import(ImportError::Limit));
    }
    let capacity = usize::try_from(metadata.len().min(64 * 1024)).unwrap_or(64 * 1024);
    let mut input =
        SensitiveBytes::try_with_capacity(capacity).map_err(|_| AppError::ImportFile)?;
    // One byte past the limit tells a file that grew since the check.
    std::io::copy(
        &mut file.take(limits.input_bytes.saturating_add(1)),
        &mut input,
    )
    .map_err(|_| AppError::ImportFile)?;
    if u64::try_from(input.len()).map_or(true, |read| read > limits.input_bytes) {
        return Err(AppError::Import(ImportError::Limit));
    }
    if importer.detect(&input, &limits) == Detection::Unsupported {
        return Err(AppError::Import(ImportError::Unsupported));
    }
    let entries = importer.import(&input, &limits)?;
    tracing::info!(
        importer = importer_id,
        entries = entries.entries().len(),
        folders = entries.folders().len(),
        "export read"
    );
    Ok(entries)
}

/// Decrypts encrypted exports with GnuPG, within the import limits.
struct KeyringDecryptor(Keyring);

impl OpenPgpDecryptor for KeyringDecryptor {
    fn decrypt(&self, ciphertext: &[u8], plaintext: &mut dyn Write) -> Result<(), ImportError> {
        let limits = ImportLimits::default();
        if u64::try_from(ciphertext.len()).map_or(true, |len| len > limits.input_bytes) {
            return Err(ImportError::Limit);
        }
        let maximum = usize::try_from(limits.decoded_bytes).map_err(|_| ImportError::Limit)?;
        let bytes = self
            .0
            .decrypt(ciphertext, Unlock::Ask, maximum)
            .map_err(|_| ImportError::Decryption)?;
        plaintext
            .write_all(bytes.as_slice())
            .map_err(|_| ImportError::Decryption)
    }
}
