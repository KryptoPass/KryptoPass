//! Import I/O stays outside the provider crate and never writes plaintext files.

use super::{GnuPgOpenPgp, SqliteStorage};
use anyhow::{Context, Result};
use kryptopass_core::{ImportBatch, ImportOptions, ImportReceipt, ImportReceiver, SecretsService};
use kryptopass_import::{
    ExportImporter, ImportError, ImportPlan, MAX_EXPORT_BYTES,
    container::{ExportDecryptor, decode_export},
    proton::ProtonPassImporter,
};
use kryptopass_secure_memory::SensitiveBytes;
use std::{fs::File, io::Read, path::Path};

impl ExportDecryptor for GnuPgOpenPgp {
    fn decrypt_export(
        &self,
        ciphertext: &[u8],
        maximum: usize,
    ) -> std::result::Result<SensitiveBytes, ImportError> {
        self.decrypt_bounded(ciphertext, MAX_EXPORT_BYTES, maximum)
            .map_err(|_| ImportError::Decryption)
    }
}

pub fn review_proton_export(path: &Path) -> Result<ImportPlan> {
    let file = File::open(path).context("could not open import source")?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(ImportError::InvalidExport.into());
    }
    if metadata.len() > MAX_EXPORT_BYTES as u64 {
        return Err(ImportError::Limit.into());
    }
    let mut input = SensitiveBytes::try_with_capacity(metadata.len() as usize + 1)?;
    std::io::copy(&mut file.take(MAX_EXPORT_BYTES as u64 + 1), &mut input)
        .context("could not read import source")?;
    let json = decode_export(&input, &GnuPgOpenPgp)?;
    Ok(ProtonPassImporter.parse(&json)?)
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
