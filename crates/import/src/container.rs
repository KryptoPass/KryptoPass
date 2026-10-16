//! Container handling shared by provider adapters; archive entries stay in memory.

use crate::{ImportError, MAX_EXPORT_BYTES};
use kryptopass_secure_memory::SensitiveBytes;
use std::io::{Cursor, Read};

/// Hosts must enforce the limit while decrypting, not after allocating output.
pub trait ExportDecryptor {
    fn decrypt_export(
        &self,
        ciphertext: &[u8],
        maximum: usize,
    ) -> Result<SensitiveBytes, ImportError>;
}

/// Accepts JSON, armored/binary OpenPGP, and Proton's ZIP containing data.json
/// or data.pgp. Ambiguous archives are rejected; attachments are never extracted.
pub fn decode_export(
    input: &[u8],
    decryptor: &impl ExportDecryptor,
) -> Result<SensitiveBytes, ImportError> {
    if input.len() > MAX_EXPORT_BYTES {
        return Err(ImportError::Limit);
    }
    if input.starts_with(b"PK") {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(input)).map_err(|_| ImportError::InvalidExport)?;
        if archive.len() > crate::MAX_RECORDS {
            return Err(ImportError::Limit);
        }
        let mut selected = None;
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|_| ImportError::InvalidExport)?;
            if matches!(
                entry.name(),
                "Proton Pass/data.json" | "Proton Pass/data.pgp" | "data.json" | "data.pgp"
            ) && selected.replace(index).is_some()
            {
                return Err(ImportError::InvalidExport);
            }
        }
        let index = selected.ok_or(ImportError::InvalidExport)?;
        let entry = archive
            .by_index(index)
            .map_err(|_| ImportError::InvalidExport)?;
        if entry.size() > MAX_EXPORT_BYTES as u64 {
            return Err(ImportError::Limit);
        }
        let mut bytes = SensitiveBytes::try_with_capacity(entry.size() as usize + 1)
            .map_err(|_| ImportError::Limit)?;
        std::io::copy(&mut entry.take(MAX_EXPORT_BYTES as u64 + 1), &mut bytes)
            .map_err(|_| ImportError::InvalidExport)?;
        if bytes.len() > MAX_EXPORT_BYTES {
            return Err(ImportError::Limit);
        }
        return decode_payload(&bytes, decryptor);
    }
    decode_payload(input, decryptor)
}

fn decode_payload(
    input: &[u8],
    decryptor: &impl ExportDecryptor,
) -> Result<SensitiveBytes, ImportError> {
    let prefix = input
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace());
    if prefix == Some(b'{') {
        return Ok(SensitiveBytes::from(input));
    }
    if input.starts_with(b"-----BEGIN PGP MESSAGE-----")
        || input.first().is_some_and(|byte| byte & 0x80 != 0)
    {
        let bytes = decryptor.decrypt_export(input, MAX_EXPORT_BYTES)?;
        if bytes.len() > MAX_EXPORT_BYTES {
            return Err(ImportError::Limit);
        }
        if bytes
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            != Some(b'{')
        {
            return Err(ImportError::InvalidExport);
        }
        return Ok(bytes);
    }
    Err(ImportError::InvalidExport)
}
