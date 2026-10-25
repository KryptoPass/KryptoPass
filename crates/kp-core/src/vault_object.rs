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
    /// Folder names from the vault root down to the record's folder; empty at the root.
    /// Display-only on phones: Desktop owns the folder tree and never moves a record
    /// because of this field.
    pub folder: Vec<String>,
    /// Icon reference; absent on the wire means automatic.
    pub icon: IconState,
    pub content: SecretContent,
}

/// Wire form of an entry icon. Images are addressed by the SHA-256 of their
/// normalized PNG, so it never carries a device-local asset ID.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum VaultIcon {
    Theme {
        name: String,
    },
    Image {
        sha256: String,
        #[serde(rename = "mediaType")]
        media_type: String,
    },
    /// Reserved: readers treat it as automatic; no writer emits it yet.
    Favicon {
        host: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
}

impl fmt::Debug for VaultIcon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Theme { .. } => "VaultIcon::Theme",
            Self::Image { .. } => "VaultIcon::Image",
            Self::Favicon { .. } => "VaultIcon::Favicon",
        })
    }
}

/// Only media type accepted for icon images in v1.
pub const VAULT_ICON_MEDIA_TYPE_PNG: &str = "image/png";

impl VaultIcon {
    /// Whether every field follows the v1 wire rules.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Theme { name } => valid_theme_name(name),
            Self::Image { sha256, media_type } => {
                valid_sha256_hex(sha256) && media_type == VAULT_ICON_MEDIA_TYPE_PNG
            }
            Self::Favicon { host, sha256 } => {
                crate::valid_favicon_host(host) && sha256.as_deref().is_none_or(valid_sha256_hex)
            }
        }
    }
}

/// Icon of a decoded object. `Unsupported` keeps an icon this version cannot
/// read: such an object can be shown but never re-encoded.
#[derive(Clone, PartialEq, Eq, Default)]
pub enum IconState {
    #[default]
    Automatic,
    Known(VaultIcon),
    Unsupported,
}

impl fmt::Debug for IconState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Automatic => f.write_str("IconState::Automatic"),
            Self::Known(icon) => write!(f, "IconState::Known({icon:?})"),
            Self::Unsupported => f.write_str("IconState::Unsupported"),
        }
    }
}

/// Theme icon names on the wire: `[a-z0-9-]{1,128}`.
pub fn valid_theme_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// 64 lowercase hexadecimal characters.
pub fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn parse_icon(raw: Option<&RawValue>) -> Result<IconState, VaultObjectError> {
    let Some(raw) = raw else {
        return Ok(IconState::Automatic);
    };
    // The icon must be a well-formed object; inside it, anything this version
    // does not know only marks the icon as unsupported.
    let generic: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(raw.get()).map_err(|_| VaultObjectError::Invalid)?;
    if generic.len() > 3 {
        return Ok(IconState::Unsupported);
    }
    match serde_json::from_str::<VaultIcon>(raw.get()) {
        Ok(icon) if icon.is_valid() => Ok(IconState::Known(icon)),
        _ => Ok(IconState::Unsupported),
    }
}

impl fmt::Debug for VaultObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VaultObject([redacted])")
    }
}

impl Drop for VaultObject {
    fn drop(&mut self) {
        self.title.zeroize();
        self.folder.iter_mut().for_each(Zeroize::zeroize);
    }
}

/// Maximum folder depth carried in one object.
pub const MAX_VAULT_OBJECT_FOLDER_DEPTH: usize = 32;

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
    #[serde(default)]
    folder: Vec<String>,
    #[serde(default, borrow)]
    icon: Option<&'a RawValue>,
    #[serde(borrow)]
    content: &'a RawValue,
}

impl Drop for BorrowedVaultObject<'_> {
    fn drop(&mut self) {
        self.title.zeroize();
        self.folder.iter_mut().for_each(Zeroize::zeroize);
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
        // Omitted at the root so root objects keep their original bytes.
        if !object.folder.is_empty() {
            writer
                .write_all(b",\"folder\":")
                .map_err(|_| VaultObjectError::LimitExceeded)?;
            write_json(&mut writer, &object.folder)?;
        }
        // Omitted when automatic so objects without an icon keep their original bytes.
        if let IconState::Known(icon) = &object.icon {
            writer
                .write_all(b",\"icon\":")
                .map_err(|_| VaultObjectError::LimitExceeded)?;
            write_json(&mut writer, icon)?;
        }
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
        || !valid_folder(&parsed.folder)
    {
        return Err(VaultObjectError::Invalid);
    }
    let icon = parse_icon(parsed.icon)?;
    let content =
        decode_content(parsed.content.get().as_bytes()).map_err(|_| VaultObjectError::Invalid)?;
    Ok(VaultObject {
        record_id: expected_record_id,
        key_fingerprint: expected_fingerprint.clone(),
        title: std::mem::take(&mut parsed.title),
        favorite: parsed.favorite,
        trashed: parsed.trashed,
        folder: std::mem::take(&mut parsed.folder),
        icon,
        content,
    })
}

fn write_json<T: Serialize>(writer: &mut impl Write, value: T) -> Result<(), VaultObjectError> {
    serde_json::to_writer(writer, &value).map_err(|_| VaultObjectError::LimitExceeded)
}

fn validate_object(object: &VaultObject) -> Result<(), VaultObjectError> {
    if object.record_id.0.is_nil() || !valid_title(&object.title) || !valid_folder(&object.folder) {
        return Err(VaultObjectError::Invalid);
    }
    match &object.icon {
        IconState::Automatic => {}
        IconState::Known(icon) if icon.is_valid() => {}
        // An icon this version cannot read is never rewritten, so it cannot be lost.
        IconState::Known(_) | IconState::Unsupported => return Err(VaultObjectError::Invalid),
    }
    Ok(())
}

fn valid_title(title: &str) -> bool {
    !title.trim().is_empty() && title.len() <= 512
}

fn valid_folder(folder: &[String]) -> bool {
    folder.len() <= MAX_VAULT_OBJECT_FOLDER_DEPTH && folder.iter().all(|name| valid_title(name))
}
