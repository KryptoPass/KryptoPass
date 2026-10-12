//! Secure-note payload and validation.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::ensure_field_size;

const MAX_NOTE_VALUE_BYTES: usize = 256 * 1024;
const MAX_NOTES_BYTES: usize = 256 * 1024;

/// Secure note fields stored only in the encrypted payload.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct NoteContent {
    pub value: String,
    pub notes: String,
}

impl NoteContent {
    pub(super) fn validate(&self) -> Result<()> {
        ensure_field_size("note value", &self.value, MAX_NOTE_VALUE_BYTES)?;
        ensure_field_size("notes", &self.notes, MAX_NOTES_BYTES)
    }
}
