//! Proton container names, ZIP selection, and OpenPGP framing.

use super::OpenPgpDecryptor;
use crate::{Detection, ImportError, ImportLimits};
use kryptopass_secure_memory::SensitiveBytes;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::io::{self, Cursor, Read, Write};

const PAYLOAD_NAMES: &[&str] = &[
    "Proton Pass/data.json",
    "Proton Pass/data.pgp",
    "data.json",
    "data.pgp",
];

pub(super) fn detect(input: &[u8], limits: &ImportLimits) -> Detection {
    if ImportLimits::check_bytes(input.len(), limits.input_bytes).is_err() {
        return Detection::Unsupported;
    }
    if input.starts_with(b"PK") {
        let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(input)) else {
            return Detection::Unsupported;
        };
        if u32::try_from(archive.len()).map_or(true, |count| count > limits.archive_entries) {
            return Detection::Unsupported;
        }
        let Ok(index) = select_payload(&mut archive) else {
            return Detection::Unsupported;
        };
        let detection = match archive.by_index(index) {
            Ok(entry) if entry.name().starts_with("Proton Pass/") => Detection::Match,
            Ok(_) => Detection::Possible,
            Err(_) => Detection::Unsupported,
        };
        return detection;
    }
    if is_openpgp(input) {
        return Detection::Possible;
    }
    if ImportLimits::check_bytes(input.len(), limits.decoded_bytes).is_err() {
        return Detection::Unsupported;
    }
    #[derive(Deserialize)]
    struct Probe<'a> {
        version: String,
        #[serde(borrow)]
        vaults: &'a RawValue,
    }
    match serde_json::from_slice::<Probe<'_>>(input) {
        Ok(probe) if !probe.version.is_empty() && probe.vaults.get().starts_with('{') => {
            Detection::Match
        }
        _ => Detection::Unsupported,
    }
}

pub(super) fn decode(
    input: &[u8],
    decryptor: Option<&dyn OpenPgpDecryptor>,
    limits: &ImportLimits,
) -> Result<SensitiveBytes, ImportError> {
    ImportLimits::check_bytes(input.len(), limits.input_bytes)?;
    if !input.starts_with(b"PK") {
        return decode_payload(input, decryptor, limits);
    }
    let mut archive =
        zip::ZipArchive::new(Cursor::new(input)).map_err(|_| ImportError::InvalidExport)?;
    if u32::try_from(archive.len()).map_err(|_| ImportError::Limit)? > limits.archive_entries {
        return Err(ImportError::Limit);
    }
    let index = select_payload(&mut archive)?;
    let entry = archive
        .by_index(index)
        .map_err(|_| ImportError::InvalidExport)?;
    if entry.size() > limits.decoded_bytes {
        return Err(ImportError::Limit);
    }
    let capacity = usize::try_from(entry.size())
        .map_err(|_| ImportError::Limit)?
        .checked_add(1)
        .ok_or(ImportError::Limit)?;
    let mut bytes = SensitiveBytes::try_with_capacity(capacity).map_err(|_| ImportError::Limit)?;
    let maximum = limits
        .decoded_bytes
        .checked_add(1)
        .ok_or(ImportError::Limit)?;
    std::io::copy(&mut entry.take(maximum), &mut bytes).map_err(|_| ImportError::InvalidExport)?;
    ImportLimits::check_bytes(bytes.len(), limits.decoded_bytes)?;
    decode_payload(&bytes, decryptor, limits)
}

fn select_payload(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<usize, ImportError> {
    let mut selected = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| ImportError::InvalidExport)?;
        if PAYLOAD_NAMES.contains(&entry.name()) && selected.replace(index).is_some() {
            return Err(ImportError::InvalidExport);
        }
    }
    selected.ok_or(ImportError::InvalidExport)
}

fn decode_payload(
    input: &[u8],
    decryptor: Option<&dyn OpenPgpDecryptor>,
    limits: &ImportLimits,
) -> Result<SensitiveBytes, ImportError> {
    if input
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'{')
    {
        ImportLimits::check_bytes(input.len(), limits.decoded_bytes)?;
        return Ok(SensitiveBytes::from(input));
    }
    if !is_openpgp(input) {
        return Err(ImportError::Unsupported);
    }
    let decryptor = decryptor.ok_or(ImportError::DecryptionRequired)?;
    let mut output = DecryptionOutput {
        bytes: SensitiveBytes::new(),
        maximum: limits.decoded_bytes,
        exceeded: false,
    };
    let result = decryptor.decrypt(input, &mut output);
    if output.exceeded {
        return Err(ImportError::Limit);
    }
    result?;
    let plaintext = output.bytes;
    if plaintext
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        != Some(b'{')
    {
        return Err(ImportError::InvalidExport);
    }
    Ok(plaintext)
}

struct DecryptionOutput {
    bytes: SensitiveBytes,
    maximum: u64,
    exceeded: bool,
}

impl Write for DecryptionOutput {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let next = self
            .bytes
            .len()
            .checked_add(input.len())
            .and_then(|length| u64::try_from(length).ok());
        if next.is_none_or(|length| length > self.maximum) {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "decrypted export exceeds limit",
            ));
        }
        self.bytes.write(input)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn is_openpgp(input: &[u8]) -> bool {
    let offset = input
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(input.len());
    let input = &input[offset..];
    if input.starts_with(b"-----BEGIN PGP MESSAGE-----") {
        return true;
    }
    input.first().is_some_and(|byte| {
        if byte & 0x80 == 0 {
            return false;
        }
        let tag = if byte & 0x40 != 0 {
            byte & 0x3f
        } else {
            (byte >> 2) & 0x0f
        };
        matches!(tag, 1 | 3 | 9 | 18 | 20)
    })
}
