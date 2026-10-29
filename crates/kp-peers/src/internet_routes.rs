//! Public relay routing, bound to the existing local identity; never authorization.

use std::{collections::BTreeMap, fs, io::Write, path::Path};

use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_lan::RelayRoute;

use super::PeerError;

const FILE: &str = "internet-routes-v1.json";
const MAX_BYTES: u64 = 1024 * 1024;

pub(super) fn read(
    directory: &Path,
    local: DeviceId,
    peers: &[PublicIdentity],
) -> Result<BTreeMap<DeviceId, RelayRoute>, PeerError> {
    let value = read_value(directory, local)?;
    let rows = value["routes"].as_object().ok_or(PeerError::Profile)?;
    let mut result = BTreeMap::new();
    for (id, row) in rows {
        let id_bytes = parse_id(id)?;
        let peer = DeviceId::from_bytes(id_bytes);
        let fields = row.as_object().ok_or(PeerError::Profile)?;
        if fields.len() != 2 {
            return Err(PeerError::Profile);
        }
        let relay = row["relay"].as_str().ok_or(PeerError::Profile)?;
        let room = parse_id(row["room"].as_str().ok_or(PeerError::Profile)?)?;
        let route = RelayRoute::new(relay, room).map_err(|_| PeerError::Profile)?;
        if peers.iter().any(|trusted| trusted.device_id == peer) {
            result.insert(peer, route);
        }
    }
    Ok(result)
}

pub(super) fn write(
    directory: &Path,
    local: DeviceId,
    peer: DeviceId,
    route: Option<&RelayRoute>,
) -> Result<(), PeerError> {
    if route.is_none()
        && !directory
            .join(FILE)
            .try_exists()
            .map_err(|_| PeerError::Profile)?
    {
        return Ok(());
    }
    let mut value = read_value(directory, local)?;
    let rows = value["routes"].as_object_mut().ok_or(PeerError::Profile)?;
    let key = peer.to_string();
    if let Some(route) = route {
        if rows.len() >= 1024 && !rows.contains_key(&key) {
            return Err(PeerError::Profile);
        }
        rows.insert(
            key,
            serde_json::json!({"relay": route.relay_origin(), "room": route.room_hex()}),
        );
    } else {
        rows.remove(&key);
    }
    let bytes = serde_json::to_vec(&value).map_err(|_| PeerError::Profile)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(PeerError::Profile);
    }
    // A private-directory, create-new temporary file prevents a partial write from
    // replacing the last usable routing snapshot. It contains no private keys.
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).map_err(|_| PeerError::Profile)?;
    let temporary = directory.join(format!(".internet-routes-{}.tmp", super::hex(&nonce)));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary).map_err(|_| PeerError::Profile)?;
        file.write_all(&bytes).map_err(|_| PeerError::Profile)?;
        file.sync_all().map_err(|_| PeerError::Profile)?;
        validate_target(directory)?;
        fs::rename(&temporary, directory.join(FILE)).map_err(|_| PeerError::Profile)?;
        super::sync_profile_directory(directory)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn validate_target(directory: &Path) -> Result<(), PeerError> {
    let path = directory.join(FILE);
    match fs::symlink_metadata(&path) {
        Ok(_) => super::validate_private_artifact(&path, MAX_BYTES),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(PeerError::Profile),
    }
}

pub(super) fn reset(directory: &Path) -> Result<(), PeerError> {
    validate_target(directory)?;
    for entry in fs::read_dir(directory).map_err(|_| PeerError::Profile)? {
        let entry = entry.map_err(|_| PeerError::Profile)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let temporary = name
            .strip_prefix(".internet-routes-")
            .and_then(|s| s.strip_suffix(".tmp"))
            .is_some_and(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()));
        if name == FILE || temporary {
            super::validate_private_artifact(&entry.path(), MAX_BYTES)?;
            fs::remove_file(entry.path()).map_err(|_| PeerError::Profile)?;
        }
    }
    super::sync_profile_directory(directory)
}

fn read_value(directory: &Path, local: DeviceId) -> Result<serde_json::Value, PeerError> {
    validate_target(directory)?;
    let path = directory.join(FILE);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::json!({"v": 1, "local": local.to_string(), "routes": {}}));
        }
        Err(_) => return Err(PeerError::Profile),
    };
    if bytes.len() as u64 > MAX_BYTES {
        return Err(PeerError::Profile);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| PeerError::Profile)?;
    let fields = value.as_object().ok_or(PeerError::Profile)?;
    if fields.len() != 3
        || value["v"].as_u64() != Some(1)
        || value["local"].as_str() != Some(local.to_string().as_str())
        || value["routes"]
            .as_object()
            .is_none_or(|rows| rows.len() > 1024)
    {
        return Err(PeerError::Profile);
    }
    Ok(value)
}

fn parse_id(value: &str) -> Result<[u8; 32], PeerError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(PeerError::Profile);
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| PeerError::Profile)?;
    }
    if bytes.iter().all(|b| *b == 0) {
        return Err(PeerError::Profile);
    }
    Ok(bytes)
}
