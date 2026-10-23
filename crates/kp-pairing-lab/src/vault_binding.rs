//! Explicit local vault selection. Network messages never create or select a vault.

use crate::{LabError, LabHost, hex};
use kryptopass_devices::SyncSessionBinding;
use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity, SqliteTrustStore};
use kryptopass_sync::{Id, Ledger, SyncError, TrustSource};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const DIRECTORY: &str = "sync-vaults-v1";
const MAX_CONFIG_BYTES: u64 = 1024;

/// Public local selection, not a capability or proof of OpenPGP key possession.
#[derive(Clone)]
pub struct VaultBindingInfo {
    pub vault_id: Id,
    pub fingerprint: Vec<u8>,
    pub ledger_path: PathBuf,
}

struct ProfileTrust {
    local: PublicIdentity,
    directory: PathBuf,
}

impl TrustSource for ProfileTrust {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        if device_id == self.local.device_id {
            return Ok(Some(self.local));
        }
        let store = SqliteTrustStore::open_existing(&self.directory, self.local.device_id)
            .map_err(|_| SyncError::Storage)?;
        store.lookup(device_id)
    }
}

impl LabHost {
    /// Starts the unique authenticated owner with the persisted local vault, if opted in.
    pub fn start_device_session(
        &self,
        devices: kryptopass_devices::DeviceDirectory,
        session: kryptopass_pairing::EstablishedSession<crate::LabTransport>,
        peer: PublicIdentity,
        options: kryptopass_devices::SessionOptions,
    ) -> Result<kryptopass_devices::SessionHandle, LabError> {
        let local_id = self.identity.device_id();
        let binding = self.sync_session_binding(peer.device_id)?;
        kryptopass_devices::start_session_with_extensions(
            devices,
            session,
            local_id,
            peer,
            options,
            binding,
            cfg!(debug_assertions),
        )
        .map_err(|_| LabError::Session)
    }

    /// Enables a caller-selected vault only after a local action for an authorized peer.
    /// Creates a new empty ledger if absent; never replaces an existing ledger or binding.
    pub fn configure_sync_vault(
        &self,
        peer: DeviceId,
        vault_id: Id,
        fingerprint: &[u8],
    ) -> Result<(), LabError> {
        self.lookup(peer)?;
        if vault_id == [0; 16]
            || !matches!(fingerprint.len(), 20 | 32)
            || fingerprint.iter().all(|v| *v == 0)
        {
            return Err(LabError::Profile);
        }
        let directory = self.directory.join(DIRECTORY);
        private_directory(&directory)?;
        let config_path = directory.join(format!("{}.json", hex(peer.as_bytes())));
        if config_path.exists() {
            let existing = self.sync_vault_info(peer)?.ok_or(LabError::Profile)?;
            return if existing.vault_id == vault_id && existing.fingerprint == fingerprint {
                self.open_selected_ledger(&existing).map(|_| ())
            } else {
                Err(LabError::Profile)
            };
        }
        let ledger_path = directory.join(format!("{}.sqlite3", hex(&vault_id)));
        let info = VaultBindingInfo {
            vault_id,
            fingerprint: fingerprint.to_vec(),
            ledger_path,
        };
        if fs::symlink_metadata(&info.ledger_path).is_ok() {
            self.open_selected_ledger(&info)?;
        } else {
            Ledger::create(&info.ledger_path, vault_id).map_err(|_| LabError::Profile)?;
        }
        let encoded = serde_json::to_vec(&json!({
            "version": 1,
            "local_device_id": hex(self.identity.device_id().as_bytes()),
            "peer_device_id": hex(peer.as_bytes()),
            "vault_id": hex(&vault_id),
            "fingerprint": hex(fingerprint),
        }))
        .map_err(|_| LabError::Profile)?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(config_path).map_err(|_| LabError::Profile)?;
        file.write_all(&encoded)
            .and_then(|()| file.sync_all())
            .map_err(|_| LabError::Profile)?;
        fs::File::open(directory)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| LabError::Profile)
    }

    /// Loads an existing binding; absence leaves ordinary sessions Sync-disabled.
    pub fn sync_vault_info(&self, peer: DeviceId) -> Result<Option<VaultBindingInfo>, LabError> {
        self.lookup(peer)?;
        let directory = self.directory.join(DIRECTORY);
        let path = directory.join(format!("{}.json", hex(peer.as_bytes())));
        let metadata = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(LabError::Profile),
        };
        check_private(&directory, true)?;
        check_private(&path, false)?;
        if metadata.len() > MAX_CONFIG_BYTES {
            return Err(LabError::Profile);
        }
        let value: Value = serde_json::from_slice(&fs::read(path).map_err(|_| LabError::Profile)?)
            .map_err(|_| LabError::Profile)?;
        let object = value.as_object().ok_or(LabError::Profile)?;
        if object.len() != 5
            || value["version"].as_u64() != Some(1)
            || value["local_device_id"].as_str() != Some(&hex(self.identity.device_id().as_bytes()))
            || value["peer_device_id"].as_str() != Some(&hex(peer.as_bytes()))
        {
            return Err(LabError::Profile);
        }
        let vault: Id = decode_hex(value["vault_id"].as_str().ok_or(LabError::Profile)?)?
            .try_into()
            .map_err(|_| LabError::Profile)?;
        let fingerprint = decode_hex(value["fingerprint"].as_str().ok_or(LabError::Profile)?)?;
        if vault == [0; 16]
            || !matches!(fingerprint.len(), 20 | 32)
            || fingerprint.iter().all(|v| *v == 0)
        {
            return Err(LabError::Profile);
        }
        Ok(Some(VaultBindingInfo {
            vault_id: vault,
            fingerprint,
            ledger_path: directory.join(format!("{}.sqlite3", hex(&vault))),
        }))
    }

    /// Opens verified existing history for exactly this locally selected peer/vault/key.
    pub fn sync_session_binding(
        &self,
        peer: DeviceId,
    ) -> Result<Option<SyncSessionBinding>, LabError> {
        let Some(info) = self.sync_vault_info(peer)? else {
            return Ok(None);
        };
        let ledger = self.open_selected_ledger(&info)?;
        Ok(Some(
            SyncSessionBinding::new(ledger, Box::new(self.sync_trust()), info.fingerprint)
                .with_automatic_rounds(),
        ))
    }

    /// Runs a bounded backend operation without projecting the private signing identity to UI.
    pub fn with_sync_vault<R>(
        &self,
        peer: DeviceId,
        operation: impl FnOnce(
            &mut Ledger,
            &DeviceIdentity,
            &dyn TrustSource,
            &[u8],
        ) -> Result<R, SyncError>,
    ) -> Result<R, LabError> {
        let info = self.sync_vault_info(peer)?.ok_or(LabError::Profile)?;
        let mut ledger = self.open_selected_ledger(&info)?;
        operation(
            &mut ledger,
            &self.identity,
            &self.sync_trust(),
            &info.fingerprint,
        )
        .map_err(|_| LabError::Profile)
    }

    fn sync_trust(&self) -> ProfileTrust {
        ProfileTrust {
            local: self.identity.public_identity(),
            directory: self.directory.clone(),
        }
    }

    fn open_selected_ledger(&self, info: &VaultBindingInfo) -> Result<Ledger, LabError> {
        Ledger::open_existing(&info.ledger_path, info.vault_id, &self.sync_trust())
            .map_err(|_| LabError::Profile)
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, LabError> {
    if value.len() > 64
        || value.len() % 2 != 0
        || !value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(LabError::Profile);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(|_| LabError::Profile)?;
            u8::from_str_radix(text, 16).map_err(|_| LabError::Profile)
        })
        .collect()
}

fn private_directory(path: &Path) -> Result<(), LabError> {
    if fs::symlink_metadata(path).is_err() {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path).map_err(|_| LabError::Profile)?;
    }
    check_private(path, true)
}

fn check_private(path: &Path, directory: bool) -> Result<(), LabError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| LabError::Profile)?;
    if (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
        || metadata.file_type().is_symlink()
    {
        return Err(LabError::Profile);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != rustix::process::getuid().as_raw() || metadata.mode() & 0o077 != 0 {
            return Err(LabError::Profile);
        }
    }
    Ok(())
}
