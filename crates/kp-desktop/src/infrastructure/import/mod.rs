//! Host composition, bounded file I/O, and conversion into the current vault.

pub mod catalog;
mod mapping;

pub use mapping::PreparedImport;

use super::{GnuPgOpenPgp, SqliteStorage};
use anyhow::{Context, Result};
use kryptopass_core::{ImportBatch, ImportOptions, ImportReceipt, ImportReceiver, SecretsService};
use kryptopass_import::importers::proton::{OpenPgpDecryptor, ProtonPassImporter};
use kryptopass_import::{Detection, ImportError, ImportLimits, ImporterRegistry};
use kryptopass_secure_memory::SensitiveBytes;
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

impl OpenPgpDecryptor for GnuPgOpenPgp {
    fn decrypt(
        &self,
        ciphertext: &[u8],
        plaintext: &mut dyn Write,
    ) -> std::result::Result<(), ImportError> {
        let limits = ImportLimits::default();
        let maximum_ciphertext =
            usize::try_from(limits.input_bytes).map_err(|_| ImportError::Limit)?;
        let maximum_plaintext =
            usize::try_from(limits.decoded_bytes).map_err(|_| ImportError::Limit)?;
        // GPGME requires a Send writer; the library's callback accepts any Write sink.
        let bytes = self
            .decrypt_bounded(ciphertext, maximum_ciphertext, maximum_plaintext)
            .map_err(|_| ImportError::Decryption)?;
        plaintext
            .write_all(&bytes)
            .map_err(|_| ImportError::Decryption)
    }
}

/// This is the host's composition point; presentation uses the registry descriptors.
pub fn registry() -> Result<ImporterRegistry> {
    let mut registry = ImporterRegistry::default();
    registry.register(Box::new(ProtonPassImporter::with_decryptor(GnuPgOpenPgp)))?;
    Ok(registry)
}

pub fn review_export(path: &Path, importer_id: &str) -> Result<PreparedImport> {
    let registry = registry()?;
    let importer = registry.get(importer_id).ok_or(ImportError::Unsupported)?;
    let limits = ImportLimits::default();
    let file = File::open(path).context("could not open import source")?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(ImportError::InvalidExport.into());
    }
    if metadata.len() > limits.input_bytes {
        return Err(ImportError::Limit.into());
    }
    let mut input =
        SensitiveBytes::try_with_capacity(usize::try_from(metadata.len().min(64 * 1024))?)?;
    let maximum = limits
        .input_bytes
        .checked_add(1)
        .ok_or(ImportError::Limit)?;
    std::io::copy(&mut file.take(maximum), &mut input).context("could not read import source")?;
    if u64::try_from(input.len())? > limits.input_bytes {
        return Err(ImportError::Limit.into());
    }
    if importer.detect(&input, &limits) == Detection::Unsupported {
        return Err(ImportError::Unsupported.into());
    }
    Ok(mapping::prepare(importer.import(&input, &limits)?)?)
}

/// The application supplies its current store and policy; core receives the batch.
pub fn receive_import(
    batch: ImportBatch,
    options: ImportOptions,
    destination: &Path,
) -> Result<ImportReceipt> {
    let storage = SqliteStorage::open_existing(destination)?;
    SecretsService::new(storage, GnuPgOpenPgp).receive_import(batch, options)
}

#[cfg(test)]
#[path = "../../../tests/unit/infrastructure/import_review.rs"]
mod tests;
