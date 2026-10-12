//! Software-license payload.

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::ensure_field_size;

const MAX_FIELD_BYTES: usize = 16 * 1024;
const MAX_NOTES_BYTES: usize = 256 * 1024;

/// Software-license fields stored only in the encrypted payload.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LicenseContent {
    pub product: String,
    pub license_key: String,
    pub holder: String,
    pub email: String,
    pub version: String,
    pub purchased: String,
    pub expires: String,
    pub notes: String,
}

impl LicenseContent {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            !self.product.is_empty(),
            "license product name cannot be empty"
        );
        ensure!(!self.license_key.is_empty(), "license key cannot be empty");
        ensure_field_size("license product", &self.product, MAX_FIELD_BYTES)?;
        ensure_field_size("license key", &self.license_key, MAX_FIELD_BYTES)?;
        ensure_field_size("license holder", &self.holder, MAX_FIELD_BYTES)?;
        ensure_field_size("license email", &self.email, MAX_FIELD_BYTES)?;
        ensure_field_size("license version", &self.version, MAX_FIELD_BYTES)?;
        ensure_field_size("license purchase date", &self.purchased, 256)?;
        ensure_field_size("license expiration date", &self.expires, 256)?;
        ensure_field_size("license notes", &self.notes, MAX_NOTES_BYTES)
    }
}
