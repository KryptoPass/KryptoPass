//! Import I/O stays outside the provider crate and never writes plaintext files.

use super::{GnuPgOpenPgp, SqliteStorage};
use anyhow::{Context, Result};
use kryptopass_core::{ImportBatch, ImportOptions, ImportReceipt, ImportReceiver, SecretsService};
use kryptopass_import::{
    ExportImporter, ImportError, ImportPlan, MAX_EXPORT_BYTES,
    container::{ExportDecryptor, decode_export},
    proton::ProtonPassImporter,
};
use std::{fs::File, io::Read, path::Path};
use zeroize::Zeroizing;

impl ExportDecryptor for GnuPgOpenPgp {
    fn decrypt_export(
        &self,
        ciphertext: &[u8],
        maximum: usize,
    ) -> std::result::Result<Zeroizing<Vec<u8>>, ImportError> {
        self.decrypt_bounded(ciphertext, MAX_EXPORT_BYTES, maximum)
            .map_err(|_| ImportError::Decryption)
    }
}

pub fn review_proton_export(path: &Path) -> Result<ImportPlan> {
    let file = File::open(path).context("could not open import source")?;
    if !file.metadata()?.is_file() {
        return Err(ImportError::InvalidExport.into());
    }
    let mut input = Zeroizing::new(Vec::new());
    file.take(MAX_EXPORT_BYTES as u64 + 1)
        .read_to_end(&mut input)
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
