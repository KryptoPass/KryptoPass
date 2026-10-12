//! Standalone authenticator payload.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::otp::{TotpConfig, validate_totp_config};

use super::ensure_field_size;

const MAX_NOTES_BYTES: usize = 256 * 1024;

/// TOTP configuration stored independently from a login.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TotpContent {
    pub config: TotpConfig,
    pub notes: String,
}

impl TotpContent {
    pub(super) fn validate(&self) -> Result<()> {
        validate_totp_config(&self.config)?;
        ensure_field_size("TOTP notes", &self.notes, MAX_NOTES_BYTES)
    }
}
