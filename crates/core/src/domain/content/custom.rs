//! Flexible user-defined key/value records.

use std::collections::HashSet;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::ensure_field_size;

const MAX_RECORDS: usize = 100;
const MAX_KEY_BYTES: usize = 512;
const MAX_VALUE_BYTES: usize = 256 * 1024;
const MAX_NOTES_BYTES: usize = 256 * 1024;

/// User-defined records stored inside one encrypted secret.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomContent {
    pub records: Vec<CustomRecord>,
    pub notes: String,
}

/// One independently editable key/value record.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomRecord {
    pub id: String,
    pub key: String,
    pub value: String,
    pub multiline: bool,
    pub concealed: bool,
}

impl CustomContent {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            self.records.len() <= MAX_RECORDS,
            "too many custom secret records"
        );
        ensure_field_size("custom secret notes", &self.notes, MAX_NOTES_BYTES)?;
        let mut ids = HashSet::with_capacity(self.records.len());
        for record in &self.records {
            let id = Uuid::parse_str(&record.id).context("invalid custom record ID")?;
            ensure!(!id.is_nil(), "invalid custom record ID");
            ensure!(
                id.to_string() == record.id,
                "custom record ID is not canonical"
            );
            ensure!(ids.insert(id), "duplicate custom record ID");
            ensure!(
                !record.key.is_empty() && record.key.len() <= MAX_KEY_BYTES,
                "custom record key is empty or exceeds the maximum size"
            );
            ensure_field_size("custom record value", &record.value, MAX_VALUE_BYTES)?;
        }
        Ok(())
    }
}
