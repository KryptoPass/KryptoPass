//! Persisted local network privacy choices for the pairing laboratory.

use std::{
    fs,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
};

use kryptopass_lan::NetworkPrivacyPolicy;

use super::LabError;

const FILE: &str = "pairing-lab-network-privacy-v1.json";
const MAX_BYTES: u64 = 16 * 1024;

/// Durable, local-only network settings. LAN selection is an explicit IP
/// chosen by the user; it does not prove VPN coverage or change Internet routes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkPrivacySettings {
    /// Persisted privacy policy applied before network operations.
    pub policy: NetworkPrivacyPolicy,
    /// Explicit local address selected for LAN sessions and recovery discovery.
    pub lan_bind_ip: Option<IpAddr>,
}

/// Load privacy settings next to the pairing profile. Missing settings use
/// FollowSystem with no VPN exception and no relay-only mode.
pub fn load_network_privacy_settings(
    profile_directory: &Path,
) -> Result<NetworkPrivacySettings, LabError> {
    let path = settings_path(profile_directory)?;
    validate_target(&path)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NetworkPrivacySettings::default());
        }
        Err(_) => return Err(LabError::Profile),
    };
    if bytes.len() as u64 > MAX_BYTES {
        return Err(LabError::Profile);
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| LabError::Profile)?;
    let fields = value.as_object().ok_or(LabError::Profile)?;
    if fields.len() != 3 || value["v"].as_u64() != Some(1) {
        return Err(LabError::Profile);
    }
    let policy: NetworkPrivacyPolicy =
        serde_json::from_value(value["policy"].clone()).map_err(|_| LabError::Profile)?;
    let lan_bind_ip = match &value["lan_bind_ip"] {
        serde_json::Value::Null => None,
        value => Some(serde_json::from_value(value.clone()).map_err(|_| LabError::Profile)?),
    };
    Ok(NetworkPrivacySettings {
        policy,
        lan_bind_ip,
    })
}

/// Atomically save the local privacy selection next to the profile. The
/// profile directory may not exist yet; its app-owned parent must already
/// exist so selecting a policy never initializes an identity or trust store.
pub fn save_network_privacy_settings(
    profile_directory: &Path,
    settings: NetworkPrivacySettings,
) -> Result<(), LabError> {
    let path = settings_path(profile_directory)?;
    let directory = path.parent().ok_or(LabError::Profile)?;
    let value = serde_json::json!({
        "v": 1,
        "policy": settings.policy,
        "lan_bind_ip": settings.lan_bind_ip,
    });
    let bytes = serde_json::to_vec(&value).map_err(|_| LabError::Profile)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(LabError::Profile);
    }
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).map_err(|_| LabError::Profile)?;
    let temporary = directory.join(format!(
        ".pairing-lab-network-privacy-{}.tmp",
        super::hex(&nonce)
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary).map_err(|_| LabError::Profile)?;
        file.write_all(&bytes).map_err(|_| LabError::Profile)?;
        file.sync_all().map_err(|_| LabError::Profile)?;
        validate_target(&path)?;
        fs::rename(&temporary, &path).map_err(|_| LabError::Profile)?;
        fs::File::open(directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| LabError::Profile)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn settings_path(profile_directory: &Path) -> Result<PathBuf, LabError> {
    profile_directory
        .parent()
        .map(|directory| directory.join(FILE))
        .ok_or(LabError::Profile)
}

fn validate_target(path: &Path) -> Result<(), LabError> {
    match fs::symlink_metadata(path) {
        Ok(_) => super::validate_private_artifact(path, MAX_BYTES),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(LabError::Profile),
    }
}
