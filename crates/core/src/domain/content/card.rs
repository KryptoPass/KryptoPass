//! Payment-card payload and input validation.

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::ensure_field_size;

const MAX_HOLDER_BYTES: usize = 1024;
const MAX_NUMBER_BYTES: usize = 64;
const MAX_SECRET_FIELD_BYTES: usize = 64;
const MAX_NOTES_BYTES: usize = 256 * 1024;

/// Payment-card fields stored only in the encrypted payload.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardContent {
    pub holder: String,
    pub number: String,
    pub expiry_month: u8,
    pub expiry_year: u16,
    pub cvv: String,
    pub pin: String,
    pub notes: String,
}

impl CardContent {
    pub(super) fn validate(&self) -> Result<()> {
        ensure_field_size("card holder", &self.holder, MAX_HOLDER_BYTES)?;
        ensure_field_size("card number", &self.number, MAX_NUMBER_BYTES)?;
        ensure!(
            (1..=12).contains(&self.expiry_month),
            "card expiry month must be between 1 and 12"
        );
        ensure!(
            (2000..=9999).contains(&self.expiry_year),
            "card expiry year is invalid"
        );
        let digits = self.number.bytes().filter(|byte| byte.is_ascii_digit()).count();
        ensure!(
            self.number
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b' ' | b'-'))
                && (12..=19).contains(&digits),
            "card number must contain 12 to 19 digits and only spaces or hyphens as separators"
        );
        ensure!(
            self.cvv.is_empty()
                || ((3..=4).contains(&self.cvv.len())
                    && self.cvv.bytes().all(|byte| byte.is_ascii_digit())),
            "card security code must contain 3 or 4 digits"
        );
        ensure!(
            self.pin.len() <= MAX_SECRET_FIELD_BYTES
                && self.pin.bytes().all(|byte| byte.is_ascii_digit()),
            "card PIN must contain only digits and fit within the maximum size"
        );
        ensure_field_size("card notes", &self.notes, MAX_NOTES_BYTES)
    }

    /// Returns the number with only spaces and hyphens removed.
    pub fn normalized_number(&self) -> String {
        self.number
            .chars()
            .filter(|character| !matches!(character, ' ' | '-'))
            .collect()
    }
}
