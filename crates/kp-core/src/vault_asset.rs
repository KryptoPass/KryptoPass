//! Immutable, content-addressed image resources shared through the Sync ledger.
//!
//! An asset is its own ledger record, encrypted to the vault's primary key like
//! any entry. Its record ID derives from the vault ID and the image hash and is
//! marked as a UUID version 8, so both apps can tell assets from entries (UUID
//! version 4) without decrypting anything. Receivers only validate the PNG;
//! they never re-encode it, since that would change its hash.

use crate::vault_object::VAULT_ICON_MEDIA_TYPE_PNG;
use crate::{png_sha256, validate_normalized_png};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use uuid::Uuid;

/// Exact format discriminator of an asset's plaintext.
pub const VAULT_ASSET_FORMAT: &str = "org.kryptopass.vault-asset.v1";

/// Maximum plaintext size of one asset object.
pub const MAX_VAULT_ASSET_BYTES: usize = 768 * 1024;

/// Senders keep at most this many assets in one ledger.
pub const MAX_LEDGER_ASSETS: usize = 512;

/// Senders keep the ciphertext of all assets in one ledger under this total.
pub const MAX_LEDGER_ASSET_CIPHERTEXT_BYTES: usize = 16 * 1024 * 1024;

const ASSET_ID_DOMAIN: &[u8] = b"org.kryptopass.vault-asset-id.v1\0";

/// Payload-free failures of the asset codec.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VaultAssetError {
    Invalid,
    LimitExceeded,
}

impl fmt::Debug for VaultAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for VaultAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid_vault_asset",
            Self::LimitExceeded => "vault_asset_limit_exceeded",
        })
    }
}

impl std::error::Error for VaultAssetError {}

/// A validated icon image.
pub struct VaultAsset {
    pub sha256: [u8; 32],
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
}

impl fmt::Debug for VaultAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultAsset")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.png.len())
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AssetJson {
    format: String,
    sha256: String,
    media_type: String,
    width: u32,
    height: u32,
    data: String,
}

/// Lowercase hexadecimal form of a hash, as written in objects and assets.
pub fn sha256_hex(digest: &[u8; 32]) -> String {
    use std::fmt::Write;
    digest
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Parses 64 lowercase hexadecimal characters.
pub fn parse_sha256_hex(value: &str) -> Option<[u8; 32]> {
    if !crate::vault_object::valid_sha256_hex(value) {
        return None;
    }
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(digest)
}

/// Ledger record ID of the asset with this hash in this vault (UUID version 8).
pub fn asset_record_id(vault_id: &[u8; 16], sha256: &[u8; 32]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(ASSET_ID_DOMAIN);
    digest.update(vault_id);
    digest.update(sha256);
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Whether a ledger record ID names an asset rather than an entry.
pub fn is_asset_record_id(id: &Uuid) -> bool {
    id.get_version_num() == 8
}

/// Encodes a strictly valid normalized PNG as an asset plaintext.
pub fn encode_vault_asset(png: &[u8]) -> Result<Vec<u8>, VaultAssetError> {
    let (width, height) = validate_normalized_png(png).map_err(|_| VaultAssetError::Invalid)?;
    let encoded = serde_json::to_vec(&AssetJson {
        format: VAULT_ASSET_FORMAT.to_owned(),
        sha256: sha256_hex(&png_sha256(png)),
        media_type: VAULT_ICON_MEDIA_TYPE_PNG.to_owned(),
        width,
        height,
        data: STANDARD.encode(png),
    })
    .map_err(|_| VaultAssetError::Invalid)?;
    if encoded.len() > MAX_VAULT_ASSET_BYTES {
        return Err(VaultAssetError::LimitExceeded);
    }
    Ok(encoded)
}

/// Decodes an asset and checks that its bytes match `expected_sha256`, the
/// hash its record ID was derived from.
pub fn decode_vault_asset(
    bytes: &[u8],
    expected_sha256: &[u8; 32],
) -> Result<VaultAsset, VaultAssetError> {
    if bytes.is_empty() || bytes.len() > MAX_VAULT_ASSET_BYTES {
        return Err(VaultAssetError::LimitExceeded);
    }
    let parsed: AssetJson = serde_json::from_slice(bytes).map_err(|_| VaultAssetError::Invalid)?;
    if parsed.format != VAULT_ASSET_FORMAT
        || parsed.media_type != VAULT_ICON_MEDIA_TYPE_PNG
        || parse_sha256_hex(&parsed.sha256).as_ref() != Some(expected_sha256)
    {
        return Err(VaultAssetError::Invalid);
    }
    let png = STANDARD
        .decode(parsed.data.as_bytes())
        .map_err(|_| VaultAssetError::Invalid)?;
    if &png_sha256(&png) != expected_sha256 {
        return Err(VaultAssetError::Invalid);
    }
    let (width, height) = validate_normalized_png(&png).map_err(|_| VaultAssetError::Invalid)?;
    if width != parsed.width || height != parsed.height {
        return Err(VaultAssetError::Invalid);
    }
    Ok(VaultAsset {
        sha256: *expected_sha256,
        width,
        height,
        png,
    })
}
