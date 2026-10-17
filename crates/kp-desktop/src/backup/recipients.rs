//! Explicit backup recipient selection from the desktop UI.

use std::collections::HashSet;

use anyhow::{Context, Result, ensure};
use kryptopass_core::{KeyFingerprint, RecipientSet};

pub(super) const MAX_SELECTED_RECIPIENTS: usize = 256;

/// Parses the explicit recipient selection sent by the UI.
pub fn parse_selected_recipients(json: &str) -> Result<RecipientSet> {
    ensure!(
        json.len() <= 20 * 1024,
        "backup recipient selection is too large"
    );
    let values: Vec<String> = serde_json::from_str(json)
        .context("backup recipients must be a JSON array of fingerprints")?;
    ensure!(
        !values.is_empty() && values.len() <= MAX_SELECTED_RECIPIENTS,
        "select between one and 256 backup recipients"
    );
    let fingerprints = values
        .iter()
        .map(|value| {
            let fingerprint = KeyFingerprint::parse(value)
                .map_err(|_| anyhow::anyhow!("backup recipient fingerprint is invalid"))?;
            ensure!(
                fingerprint.as_str() == value,
                "backup recipient is not canonical"
            );
            Ok(fingerprint)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        fingerprints.iter().collect::<HashSet<_>>().len() == fingerprints.len(),
        "backup recipients must be unique"
    );
    RecipientSet::new(fingerprints)
        .map_err(|_| anyhow::anyhow!("backup recipient selection is invalid"))
}
