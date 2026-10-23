//! Versioned encrypted object envelope shared by Desktop and Android.
//!
//! Only record metadata and the schema-v4 [`SecretContent`] payload are part
//! of this plaintext object. Sync revision IDs, parents, counters and
//! signatures belong to `kp-sync` and are deliberately kept outside it.

use crate::{
    KeyFingerprint, SecretContent, SecretId, SensitiveBytes, decode_content, encode_content,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::fmt;
use std::io::Write;
use zeroize::Zeroize;

/// Exact format discriminator used by the Android local vault codec.
pub const VAULT_OBJECT_FORMAT: &str = "org.kryptopass.vault-object.v1";

/// Maximum size of the complete plaintext envelope, in bytes.
pub const MAX_VAULT_OBJECT_BYTES: usize = 1024 * 1024;

/// A decrypted, short-lived projection ready to be encrypted as one sync object.
pub struct VaultObject {
    pub record_id: SecretId,
    pub key_fingerprint: KeyFingerprint,
    pub title: String,
    pub favorite: bool,
    pub trashed: bool,
    pub content: SecretContent,
}

impl fmt::Debug for VaultObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VaultObject([redacted])")
    }
}

impl Drop for VaultObject {
    fn drop(&mut self) {
        self.title.zeroize();
    }
}

/// Payload-free errors for the strict encrypted-object codec.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VaultObjectError {
    /// The outer object or nested content violates its schema.
    Invalid,
    /// The complete object exceeds the one MiB plaintext limit.
    LimitExceeded,
}

impl fmt::Debug for VaultObjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VaultObjectError([redacted])")
    }
}

impl fmt::Display for VaultObjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid_vault_object",
            Self::LimitExceeded => "vault_object_limit_exceeded",
        })
    }
}

impl std::error::Error for VaultObjectError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BorrowedVaultObject<'a> {
    format: &'a str,
    record_id: &'a str,
    key_fingerprint: &'a str,
    title: String,
    favorite: bool,
    trashed: bool,
    #[serde(borrow)]
    content: &'a RawValue,
}

impl Drop for BorrowedVaultObject<'_> {
    fn drop(&mut self) {
        self.title.zeroize();
    }
}

/// Encodes the exact Android v1 JSON envelope into protected memory.
///
/// The nested content bytes come from the shared schema-v4 codec, including
/// every supported content field and login-history value.
pub fn encode_vault_object(object: &VaultObject) -> Result<SensitiveBytes, VaultObjectError> {
    validate_object(object)?;
    let content = encode_content(&object.content).map_err(|_| VaultObjectError::Invalid)?;
    let mut output = SensitiveBytes::new();
    {
        let mut writer = output.bounded_writer(MAX_VAULT_OBJECT_BYTES);
        writer
            .write_all(b"{\"format\":")
            .map_err(|_| VaultObjectError::LimitExceeded)?;
        write_json(&mut writer, VAULT_OBJECT_FORMAT)?;
        writer
            .write_all(b",\"recordId\":")
            .map_err(|_| VaultObjectError::LimitExceeded)?;
        let record_id = object.record_id.0.to_string();
        write_json(&mut writer, &record_id)?;
        writer
            .write_all(b",\"keyFingerprint\":")
            .map_err(|_| VaultObjectError::LimitExceeded)?;
        write_json(&mut writer, object.key_fingerprint.as_str())?;
        writer
            .write_all(b",\"title\":")
            .map_err(|_| VaultObjectError::LimitExceeded)?;
        write_json(&mut writer, &object.title)?;
        writer
            .write_all(b",\"favorite\":")
            .map_err(|_| VaultObjectError::LimitExceeded)?;
        write_json(&mut writer, object.favorite)?;
        writer
            .write_all(b",\"trashed\":")
            .map_err(|_| VaultObjectError::LimitExceeded)?;
        write_json(&mut writer, object.trashed)?;
        writer
            .write_all(b",\"content\":")
            .map_err(|_| VaultObjectError::LimitExceeded)?;
        writer
            .write_all(content.as_slice())
            .map_err(|_| VaultObjectError::LimitExceeded)?;
        writer
            .write_all(b"}")
            .map_err(|_| VaultObjectError::LimitExceeded)?;
    }
    if output.len() > MAX_VAULT_OBJECT_BYTES {
        return Err(VaultObjectError::LimitExceeded);
    }
    Ok(output)
}

/// Decodes a strict Android v1 envelope bound to the expected record and key.
pub fn decode_vault_object(
    bytes: &[u8],
    expected_record_id: SecretId,
    expected_fingerprint: &KeyFingerprint,
) -> Result<VaultObject, VaultObjectError> {
    if bytes.is_empty() || bytes.len() > MAX_VAULT_OBJECT_BYTES {
        return Err(VaultObjectError::LimitExceeded);
    }
    if expected_record_id.0.is_nil() {
        return Err(VaultObjectError::Invalid);
    }
    let mut parsed: BorrowedVaultObject<'_> =
        serde_json::from_slice(bytes).map_err(|_| VaultObjectError::Invalid)?;
    let expected_id = expected_record_id.0.to_string();
    if parsed.format != VAULT_OBJECT_FORMAT
        || parsed.record_id != expected_id
        || parsed.key_fingerprint != expected_fingerprint.as_str()
        || !valid_title(&parsed.title)
    {
        return Err(VaultObjectError::Invalid);
    }
    let content =
        decode_content(parsed.content.get().as_bytes()).map_err(|_| VaultObjectError::Invalid)?;
    Ok(VaultObject {
        record_id: expected_record_id,
        key_fingerprint: expected_fingerprint.clone(),
        title: std::mem::take(&mut parsed.title),
        favorite: parsed.favorite,
        trashed: parsed.trashed,
        content,
    })
}

fn write_json<T: Serialize>(writer: &mut impl Write, value: T) -> Result<(), VaultObjectError> {
    serde_json::to_writer(writer, &value).map_err(|_| VaultObjectError::LimitExceeded)
}

fn validate_object(object: &VaultObject) -> Result<(), VaultObjectError> {
    if object.record_id.0.is_nil() || !valid_title(&object.title) {
        return Err(VaultObjectError::Invalid);
    }
    Ok(())
}

fn valid_title(title: &str) -> bool {
    !title.trim().is_empty() && title.len() <= 512
}
