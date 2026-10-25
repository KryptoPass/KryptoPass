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

pub(crate) const DIRECTORY: &str = "sync-vaults-v1";
const MAX_CONFIG_BYTES: u64 = 1024;
/// Most keys one phone can share with a vault; the file stays within `MAX_CONFIG_BYTES`.
pub const MAX_SYNC_KEYS: usize = 8;

/// Public local selection, not a capability or proof of OpenPGP key possession.
#[derive(Clone)]
pub struct VaultBindingInfo {
    pub vault_id: Id,
    /// Key announced in `SyncHello`; for older bindings also the one `vault_id` derives from.
    pub fingerprint: Vec<u8>,
    /// Every key whose records take part, sorted.
    pub fingerprints: Vec<Vec<u8>>,
    /// The vault ID is random and the peer's announced key is not compared. Older
    /// bindings tie both to one key and keep the strict check.
    pub key_agnostic: bool,
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

    fn lookup_history(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        if let Some(identity) = self.lookup(device_id)? {
            return Ok(Some(identity));
        }
        read_forgotten_author(&self.directory, device_id).map_err(|_| SyncError::Storage)
    }
}

fn author_path(profile: &Path, device_id: DeviceId) -> PathBuf {
    profile
        .join(DIRECTORY)
        .join(format!("{}.author.json", hex(device_id.as_bytes())))
}

/// Keeps the public keys of a peer that is being forgotten, so the history it
/// signed in the Sync ledger remains verifiable. This never authorizes it.
pub(crate) fn keep_forgotten_author(
    profile: &Path,
    identity: &PublicIdentity,
) -> Result<(), LabError> {
    let directory = profile.join(DIRECTORY);
    private_directory(&directory)?;
    let path = author_path(profile, identity.device_id);
    let encoded = serde_json::to_vec(&json!({
        "version": 1,
        "signing": hex(&identity.signing_public_key),
        "static": hex(&identity.static_public_key),
    }))
    .map_err(|_| LabError::Profile)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| LabError::Profile)?;
    file.write_all(&encoded)
        .and_then(|()| file.sync_all())
        .map_err(|_| LabError::Profile)?;
    fs::File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| LabError::Profile)
}

fn read_forgotten_author(
    profile: &Path,
    device_id: DeviceId,
) -> Result<Option<PublicIdentity>, LabError> {
    let path = author_path(profile, device_id);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(LabError::Profile),
    };
    check_private(&path, false)?;
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(LabError::Profile);
    }
    let value: Value = serde_json::from_slice(&fs::read(path).map_err(|_| LabError::Profile)?)
        .map_err(|_| LabError::Profile)?;
    if value.as_object().map(|object| object.len()) != Some(3)
        || value["version"].as_u64() != Some(1)
    {
        return Err(LabError::Profile);
    }
    let signing: [u8; 32] = decode_hex(value["signing"].as_str().ok_or(LabError::Profile)?)?
        .try_into()
        .map_err(|_| LabError::Profile)?;
    let static_key: [u8; 32] = decode_hex(value["static"].as_str().ok_or(LabError::Profile)?)?
        .try_into()
        .map_err(|_| LabError::Profile)?;
    let identity = PublicIdentity::new(signing, static_key).map_err(|_| LabError::Profile)?;
    // The ID is derived from the signing key, so a swapped file cannot impersonate another author.
    if identity.device_id != device_id {
        return Err(LabError::Profile);
    }
    Ok(Some(identity))
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
    /// Legacy form: the caller supplies a vault ID tied to one key, and both sides must
    /// announce that key in `SyncHello`.
    pub fn configure_sync_vault(
        &self,
        peer: DeviceId,
        vault_id: Id,
        fingerprint: &[u8],
    ) -> Result<(), LabError> {
        let all = self.checked_sync_keys(peer, vault_id, &[fingerprint.to_vec()])?;
        self.create_binding(peer, vault_id, 2, fingerprint, &all)
    }

    /// Creates the Sync binding for `peer` with a fresh random vault ID that has no
    /// relation to any key, and returns it. `fingerprints` only say which records take
    /// part; the first of them (in sorted order) is the key announced in `SyncHello`,
    /// which the peer does not compare for this kind of binding. Fails if a binding exists.
    pub fn create_sync_vault(
        &self,
        peer: DeviceId,
        fingerprints: &[Vec<u8>],
    ) -> Result<Id, LabError> {
        let vault_id = kryptopass_sync::fresh_id().map_err(|_| LabError::Profile)?;
        let all = self.checked_sync_keys(peer, vault_id, fingerprints)?;
        self.create_binding(peer, vault_id, 3, &all[0], &all)?;
        Ok(vault_id)
    }

    /// Replaces the shared keys of an existing key-agnostic binding, keeping its vault
    /// ID and ledger. Legacy bindings cannot change their keys.
    pub fn update_sync_keys(
        &self,
        peer: DeviceId,
        fingerprints: &[Vec<u8>],
    ) -> Result<(), LabError> {
        let existing = self.sync_vault_info(peer)?.ok_or(LabError::Profile)?;
        if !existing.key_agnostic {
            return Err(LabError::Profile);
        }
        let all = self.checked_sync_keys(peer, existing.vault_id, fingerprints)?;
        let directory = self.directory.join(DIRECTORY);
        let path = directory.join(format!("{}.json", hex(peer.as_bytes())));
        let temporary = directory.join(format!("{}.json.tmp", hex(peer.as_bytes())));
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|_| LabError::Profile)?;
        file.write_all(&self.encode_binding(peer, existing.vault_id, 3, &all[0], &all)?)
            .and_then(|()| file.sync_all())
            .map_err(|_| LabError::Profile)?;
        fs::rename(&temporary, &path).map_err(|_| LabError::Profile)?;
        fs::File::open(directory)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| LabError::Profile)
    }

    /// Validates a binding request and returns the sorted, unique key list.
    fn checked_sync_keys(
        &self,
        peer: DeviceId,
        vault_id: Id,
        fingerprints: &[Vec<u8>],
    ) -> Result<Vec<Vec<u8>>, LabError> {
        self.lookup(peer)?;
        let mut all = fingerprints.to_vec();
        all.sort_unstable();
        all.dedup();
        if vault_id == [0; 16]
            || all.is_empty()
            || all.len() != fingerprints.len()
            || all.len() > MAX_SYNC_KEYS
            || all.iter().any(|value| valid_fingerprint_rejects(value))
        {
            return Err(LabError::Profile);
        }
        Ok(all)
    }

    fn encode_binding(
        &self,
        peer: DeviceId,
        vault_id: Id,
        version: u8,
        fingerprint: &[u8],
        all: &[Vec<u8>],
    ) -> Result<Vec<u8>, LabError> {
        serde_json::to_vec(&json!({
            "version": version,
            "local_device_id": hex(self.identity.device_id().as_bytes()),
            "peer_device_id": hex(peer.as_bytes()),
            "vault_id": hex(&vault_id),
            "fingerprint": hex(fingerprint),
            "fingerprints": all.iter().map(|value| hex(value)).collect::<Vec<_>>(),
        }))
        .map_err(|_| LabError::Profile)
    }

    /// Writes a new binding file and creates (or opens) its ledger. Never overwrites.
    fn create_binding(
        &self,
        peer: DeviceId,
        vault_id: Id,
        version: u8,
        fingerprint: &[u8],
        all: &[Vec<u8>],
    ) -> Result<(), LabError> {
        let directory = self.directory.join(DIRECTORY);
        private_directory(&directory)?;
        let config_path = directory.join(format!("{}.json", hex(peer.as_bytes())));
        if config_path.exists() {
            let existing = self.sync_vault_info(peer)?.ok_or(LabError::Profile)?;
            return if existing.vault_id == vault_id
                && existing.fingerprint == fingerprint
                && existing.fingerprints == all
            {
                self.open_selected_ledger(&existing).map(|_| ())
            } else {
                Err(LabError::Profile)
            };
        }
        let ledger_path = directory.join(format!("{}.sqlite3", hex(&vault_id)));
        let info = VaultBindingInfo {
            vault_id,
            fingerprint: fingerprint.to_vec(),
            fingerprints: all.to_vec(),
            key_agnostic: version == 3,
            ledger_path,
        };
        if fs::symlink_metadata(&info.ledger_path).is_ok() {
            self.open_selected_ledger(&info)?;
        } else {
            Ledger::create(&info.ledger_path, vault_id).map_err(|_| LabError::Profile)?;
        }
        let encoded = self.encode_binding(peer, vault_id, version, fingerprint, all)?;
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

    /// Removes the local selection for `peer`. Ledgers are kept: they hold vault history
    /// that other peers or a later selection still use.
    pub fn remove_sync_vault(&self, peer: DeviceId) -> Result<(), LabError> {
        self.lookup(peer)?;
        let directory = self.directory.join(DIRECTORY);
        let path = directory.join(format!("{}.json", hex(peer.as_bytes())));
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                check_private(&path, false)?;
                fs::remove_file(&path).map_err(|_| LabError::Profile)?;
                fs::File::open(directory)
                    .and_then(|dir| dir.sync_all())
                    .map_err(|_| LabError::Profile)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(LabError::Profile),
        }
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
        let version = value["version"].as_u64();
        let expected_fields = match version {
            Some(1) => 5,
            Some(2 | 3) => 6,
            _ => return Err(LabError::Profile),
        };
        if object.len() != expected_fields
            || value["local_device_id"].as_str() != Some(&hex(self.identity.device_id().as_bytes()))
            || value["peer_device_id"].as_str() != Some(&hex(peer.as_bytes()))
        {
            return Err(LabError::Profile);
        }
        let vault: Id = decode_hex(value["vault_id"].as_str().ok_or(LabError::Profile)?)?
            .try_into()
            .map_err(|_| LabError::Profile)?;
        let fingerprint = decode_hex(value["fingerprint"].as_str().ok_or(LabError::Profile)?)?;
        let mut fingerprints = if matches!(version, Some(2 | 3)) {
            value["fingerprints"]
                .as_array()
                .ok_or(LabError::Profile)?
                .iter()
                .map(|item| decode_hex(item.as_str().ok_or(LabError::Profile)?))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![fingerprint.clone()]
        };
        fingerprints.sort_unstable();
        let unique = fingerprints.windows(2).all(|pair| pair[0] != pair[1]);
        if vault == [0; 16]
            || valid_fingerprint_rejects(&fingerprint)
            || fingerprints.is_empty()
            || fingerprints.len() > MAX_SYNC_KEYS
            || !unique
            || fingerprints
                .iter()
                .any(|value| valid_fingerprint_rejects(value))
        {
            return Err(LabError::Profile);
        }
        Ok(Some(VaultBindingInfo {
            vault_id: vault,
            fingerprint,
            fingerprints,
            key_agnostic: version == Some(3),
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
                .with_automatic_rounds()
                .with_key_agnostic_hello(info.key_agnostic),
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
        Ledger::open_existing(&info.ledger_path, info.vault_id, &self.sync_trust()).map_err(
            |error| match error {
                SyncError::UnknownAuthor => LabError::SyncHistoryUntrusted,
                _ => LabError::Profile,
            },
        )
    }
}

/// True when `value` cannot be a key fingerprint.
fn valid_fingerprint_rejects(value: &[u8]) -> bool {
    !matches!(value.len(), 20 | 32) || value.iter().all(|byte| *byte == 0)
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

/// Accepts the Sync directory during a profile reset: private, owned, real.
pub(crate) fn validate_directory(path: &Path) -> Result<(), LabError> {
    check_private(path, true)
}

/// Drops every per-peer selection (`<peer>.json`) after a reset. Ledgers, mirror
/// state and anything else stay: they belong to the vault, not to pairing.
pub(crate) fn remove_all_selections(profile: &Path) -> Result<(), LabError> {
    let directory = profile.join(DIRECTORY);
    if fs::symlink_metadata(&directory).is_err() {
        return Ok(());
    }
    check_private(&directory, true)?;
    for entry in fs::read_dir(&directory).map_err(|_| LabError::Profile)? {
        let entry = entry.map_err(|_| LabError::Profile)?;
        let name = entry.file_name();
        let is_selection = name.to_str().is_some_and(|name| {
            name.len() == 69
                && name.ends_with(".json")
                && name[..64].bytes().all(|byte| byte.is_ascii_hexdigit())
        });
        if is_selection {
            fs::remove_file(entry.path()).map_err(|_| LabError::Profile)?;
        }
    }
    Ok(())
}
