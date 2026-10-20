//! Export public TURN access records for an existing pairing-lab profile.
//!
//! This administrative helper never creates an identity or exports private
//! key material. The output is intended for the TURN broker's local allowlist.

use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use kryptopass_identity::PublicIdentity;
use kryptopass_lan::RelayRoute;
use kryptopass_pairing_lab::LabHost;

const MAX_DEVICES: usize = 64;

struct Arguments {
    profile: PathBuf,
    output: PathBuf,
    origin: String,
}

fn main() {
    let arguments = parse_arguments().unwrap_or_else(|| {
        eprintln!("Usage: pairing-turn-access --profile <existingdir> --output <newaccessfile> --origin <httpscanonical>");
        std::process::exit(2);
    });
    match export_allowlist(arguments) {
        Ok(device_count) => {
            eprintln!("TURN access allowlist created for {device_count} devices.");
        }
        Err(message) => {
            eprintln!("TURN access allowlist was not created: {message}");
            std::process::exit(1);
        }
    }
}

fn parse_arguments() -> Option<Arguments> {
    let mut profile = None;
    let mut output = None;
    let mut origin = None;
    let mut values = env::args_os().skip(1);
    while let Some(flag) = values.next() {
        let value = values.next()?;
        match flag.to_str()? {
            "--profile" if profile.is_none() => profile = Some(PathBuf::from(value)),
            "--output" if output.is_none() => output = Some(PathBuf::from(value)),
            "--origin" if origin.is_none() => origin = Some(value.into_string().ok()?),
            _ => return None,
        }
    }
    Some(Arguments {
        profile: profile?,
        output: output?,
        origin: origin?,
    })
}

fn export_allowlist(arguments: Arguments) -> Result<usize, &'static str> {
    validate_origin(&arguments.origin)?;
    validate_output_directory(&arguments.output)?;

    // This entry point only loads an existing identity and trust database.
    let host = LabHost::open_existing_system_profile(&arguments.profile)
        .map_err(|_| "the existing pairing profile could not be opened")?;
    let local = host.public_identity();
    local
        .validate()
        .map_err(|_| "the local public identity is invalid")?;
    let peers = host
        .trusted_peers()
        .map_err(|_| "trusted device records could not be read")?;

    let mut identities = BTreeMap::<[u8; 32], [u8; 32]>::new();
    insert_identity(&mut identities, local)?;
    for peer in peers {
        peer.validate()
            .map_err(|_| "a trusted public identity is invalid")?;
        insert_identity(&mut identities, peer)?;
        if identities.len() > MAX_DEVICES {
            return Err("the profile exceeds the 64-device TURN access limit");
        }
    }
    if identities.len() > MAX_DEVICES {
        return Err("the profile exceeds the 64-device TURN access limit");
    }

    let devices: Vec<_> = identities
        .iter()
        .map(|(device, signing_public_key)| {
            serde_json::json!({
                "device": lower_hex(device),
                "signing_public_key": lower_hex(signing_public_key),
            })
        })
        .collect();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "v": 1,
        "origin": arguments.origin,
        "devices": devices,
    }))
    .map_err(|_| "the public allowlist could not be encoded")?;
    write_new_private_file(&arguments.output, &bytes)?;
    Ok(identities.len())
}

fn insert_identity(
    identities: &mut BTreeMap<[u8; 32], [u8; 32]>,
    identity: PublicIdentity,
) -> Result<(), &'static str> {
    let device = *identity.device_id.as_bytes();
    let signing_key = identity.signing_public_key;
    if identities
        .insert(device, signing_key)
        .is_some_and(|previous| previous != signing_key)
    {
        return Err("the profile contains conflicting public identities");
    }
    Ok(())
}

fn validate_origin(origin: &str) -> Result<(), &'static str> {
    if origin.is_empty()
        || origin.len() > 512
        || !origin.is_ascii()
        || origin.trim() != origin
        || origin.chars().any(char::is_control)
    {
        return Err("the broker origin is not canonical HTTPS");
    }
    let Some(authority) = origin.strip_prefix("https://") else {
        return Err("the broker origin is not canonical HTTPS");
    };
    let relay_origin = format!("wss://{authority}");
    let route = RelayRoute::new(&relay_origin, [1; 32])
        .map_err(|_| "the broker origin is not canonical HTTPS")?;
    if route.relay_origin() != relay_origin {
        return Err("the broker origin is not canonical HTTPS");
    }
    Ok(())
}

fn validate_output_directory(output: &Path) -> Result<(), &'static str> {
    if output.file_name().is_none() {
        return Err("the output path is invalid");
    }
    let absolute = if output.is_absolute() {
        output.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|_| "the output directory could not be checked")?
            .join(output)
    };
    let parent = absolute
        .parent()
        .ok_or("the output directory could not be checked")?;
    check_no_symlink_components(parent)?;
    let metadata =
        fs::symlink_metadata(parent).map_err(|_| "the output directory could not be checked")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o777 != 0o700
        {
            return Err("the output directory must be owned by this user with mode 0700");
        }
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        return Err("private output files require a Unix filesystem");
    }
    Ok(())
}

fn check_no_symlink_components(path: &Path) -> Result<(), &'static str> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => return Err("the output directory path is invalid"),
            Component::Normal(part) => {
                current.push(part);
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|_| "the output directory could not be checked")?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("the output directory path contains a symlink or non-directory");
                }
            }
        }
    }
    Ok(())
}

fn write_new_private_file(path: &Path, contents: &[u8]) -> Result<(), &'static str> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|_| "the output file could not be created")?
            .join(path)
    };
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&absolute)
        .map_err(|_| "the output file already exists or could not be created")?;
    let write_result = (|| -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(contents)?;
        file.sync_all()?;
        let parent = absolute
            .parent()
            .ok_or_else(|| io::Error::other("missing parent directory"))?;
        fs::File::open(parent)?.sync_all()
    })();
    if write_result.is_err() {
        drop(file);
        let _ = fs::remove_file(&absolute);
        return Err("the output file could not be written safely");
    }
    Ok(())
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}
