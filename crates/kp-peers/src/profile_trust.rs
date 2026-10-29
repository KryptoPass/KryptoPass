//! The public keys of devices that were forgotten.
//!
//! Forgetting a device removes its trust, but versions it signed stay in the
//! vault and must remain verifiable. Its public keys are kept here and served
//! by [`ProfileTrust::lookup_history`]; they never authorize anything new.

use crate::{PeerError, hex};
use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_store::trust::SqliteTrustStore;
use kryptopass_sync::{SyncError, TrustSource};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) const DIRECTORY: &str = "forgotten-authors-v1";
const MAX_AUTHOR_BYTES: u64 = 1024;

/// Trust for Sync: this device, the authorized peers, and for history only
/// the devices that were forgotten.
pub struct ProfileTrust {
    pub(crate) local: PublicIdentity,
    pub(crate) directory: PathBuf,
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
/// signed in the vault remains verifiable. This never authorizes it.
pub(crate) fn keep_forgotten_author(
    profile: &Path,
    identity: &PublicIdentity,
) -> Result<(), PeerError> {
    let directory = profile.join(DIRECTORY);
    private_directory(&directory)?;
    let path = author_path(profile, identity.device_id);
    let encoded = serde_json::to_vec(&json!({
        "version": 1,
        "signing": hex(&identity.signing_public_key),
        "static": hex(&identity.static_public_key),
    }))
    .map_err(|_| PeerError::Profile)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| PeerError::Profile)?;
    file.write_all(&encoded)
        .and_then(|()| file.sync_all())
        .map_err(|_| PeerError::Profile)?;
    fs::File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| PeerError::Profile)
}

fn read_forgotten_author(
    profile: &Path,
    device_id: DeviceId,
) -> Result<Option<PublicIdentity>, PeerError> {
    let path = author_path(profile, device_id);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(PeerError::Profile),
    };
    check_private(&path, false)?;
    if metadata.len() > MAX_AUTHOR_BYTES {
        return Err(PeerError::Profile);
    }
    let value: Value = serde_json::from_slice(&fs::read(path).map_err(|_| PeerError::Profile)?)
        .map_err(|_| PeerError::Profile)?;
    if value.as_object().map(|object| object.len()) != Some(3)
        || value["version"].as_u64() != Some(1)
    {
        return Err(PeerError::Profile);
    }
    let signing: [u8; 32] = decode_hex(value["signing"].as_str().ok_or(PeerError::Profile)?)?
        .try_into()
        .map_err(|_| PeerError::Profile)?;
    let static_key: [u8; 32] = decode_hex(value["static"].as_str().ok_or(PeerError::Profile)?)?
        .try_into()
        .map_err(|_| PeerError::Profile)?;
    let identity = PublicIdentity::new(signing, static_key).map_err(|_| PeerError::Profile)?;
    // The ID is derived from the signing key, so a swapped file cannot impersonate another author.
    if identity.device_id != device_id {
        return Err(PeerError::Profile);
    }
    Ok(Some(identity))
}

fn decode_hex(value: &str) -> Result<Vec<u8>, PeerError> {
    if value.len() > 64
        || value.len() % 2 != 0
        || !value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(PeerError::Profile);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(|_| PeerError::Profile)?;
            u8::from_str_radix(text, 16).map_err(|_| PeerError::Profile)
        })
        .collect()
}

fn private_directory(path: &Path) -> Result<(), PeerError> {
    if fs::symlink_metadata(path).is_err() {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path).map_err(|_| PeerError::Profile)?;
    }
    check_private(path, true)
}

fn check_private(path: &Path, directory: bool) -> Result<(), PeerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| PeerError::Profile)?;
    if (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
        || metadata.file_type().is_symlink()
    {
        return Err(PeerError::Profile);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != rustix::process::getuid().as_raw() || metadata.mode() & 0o077 != 0 {
            return Err(PeerError::Profile);
        }
    }
    Ok(())
}

/// Accepts the forgotten-authors directory during a profile reset: private,
/// owned, real. A reset keeps it, because the vault history still needs it.
pub(crate) fn validate_directory(path: &Path) -> Result<(), PeerError> {
    check_private(path, true)
}
