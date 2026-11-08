//! The per-device side of Sync: which keys each paired device syncs, and making
//! what arrived listable.
//!
//! The exchange itself runs inside a device session over the vault's store
//! (`kp-sync`, `kp-devices`). A device syncs what is encrypted to its keys (K
//! in the contract, `docs/sync/contrato.md`); only this device's choice is
//! kept here, and the other side routes with its own.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use kryptopass_core::vault::Reindex;

use crate::error::AppError;
use crate::session::AppVault;

const KEYS_FILE: &str = "sync-keys-v1.json";

/// The keys chosen for each paired device, by device id. A device without keys
/// does not sync.
pub struct SyncSelection {
    path: PathBuf,
}

impl SyncSelection {
    /// The selection kept in the private device profile `directory`.
    pub fn in_profile(directory: &Path) -> Self {
        Self {
            path: directory.join(KEYS_FILE),
        }
    }

    fn read(&self) -> Result<BTreeMap<String, Vec<String>>> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("the Sync selection is corrupt"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(error).context("could not read the Sync selection"),
        }
    }

    fn write(&self, keys: &BTreeMap<String, Vec<String>>) -> Result<()> {
        let bytes = serde_json::to_vec(keys).context("could not encode the Sync selection")?;
        let staged = self
            .path
            .with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| -> Result<()> {
            let mut file = options
                .open(&staged)
                .context("could not write the Sync selection")?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .context("could not write the Sync selection")?;
            fs::rename(&staged, &self.path).context("could not publish the Sync selection")
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staged);
        }
        result
    }

    /// The keys `device` syncs, in hex; empty when it does not sync.
    pub fn keys(&self, device: &str) -> Result<Vec<String>> {
        Ok(self.read()?.remove(device).unwrap_or_default())
    }

    /// Replaces the keys `device` syncs; none stops syncing with it. History
    /// and what it already received stay.
    pub fn set(&self, device: &str, keys: Vec<String>) -> Result<()> {
        let mut selection = self.read()?;
        if keys.is_empty() {
            selection.remove(device);
        } else {
            selection.insert(device.to_owned(), keys);
        }
        self.write(&selection)?;
        tracing::info!(
            keys = selection.get(device).map_or(0, Vec::len),
            "Sync selection changed"
        );
        Ok(())
    }

    /// Adds a key to a device that already syncs. Returns whether it was added;
    /// a device that does not sync stays so, since that choice is the person's.
    pub fn add(&self, device: &str, key: &str) -> Result<bool> {
        let mut selection = self.read()?;
        let Some(keys) = selection.get_mut(device) else {
            return Ok(false);
        };
        if keys
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(key))
        {
            return Ok(false);
        }
        keys.push(key.to_owned());
        self.write(&selection)?;
        Ok(true)
    }

    /// Forgets a device that is no longer paired.
    pub fn forget(&self, device: &str) -> Result<()> {
        self.set(device, Vec::new())
    }
}

/// Indexes the versions that arrived and that this device can open, with
/// passphrases gpg-agent caches. Returns how many became listable.
pub fn index_received(vault: &mut AppVault) -> Result<usize, AppError> {
    Ok(vault.reindex()?)
}

/// Like [`index_received`], and also says which key's passphrase the pass was waiting for.
/// A locked key never stops Sync: it only leaves what arrived for that key unlisted.
pub fn index_received_report(vault: &mut AppVault) -> Result<Reindex, AppError> {
    Ok(vault.reindex_report()?)
}
