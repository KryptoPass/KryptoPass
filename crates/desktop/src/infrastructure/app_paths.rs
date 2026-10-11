//! Application-owned paths for persistent data.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const DATABASE_FILE: &str = "kryptopass.sqlite3";
const ACTIVE_VAULT_FILE: &str = "active-vault.json";
const MAX_ACTIVE_VAULT_FILE_BYTES: u64 = 4096;
static ACTIVE_VAULT_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActiveVaultConfig {
    path: String,
}

/// Returns the database path under the platform data directory.
///
/// `app_id` is supplied by the application identity owner so this module does
/// not duplicate the desktop ID. Only the app-specific directory is made
/// private; shared ancestors such as `~/.local/share` are never chmodded.
pub fn database_path(app_id: &str) -> Result<PathBuf> {
    validate_app_id(app_id)?;

    if let Some(path) = active_vault_path(app_id)? {
        return Ok(path);
    }

    default_database_path(app_id)
}

/// Allocates a new vault name in the application's private data directory.
pub fn restored_vault_path(app_id: &str) -> Result<PathBuf> {
    let database = default_database_path(app_id)?;
    let directory = database
        .parent()
        .context("vault directory is unavailable")?;
    Ok(directory.join(format!("restored-{}.sqlite3", uuid::Uuid::new_v4())))
}

/// Returns the explicitly selected database file, if the user has selected one.
pub fn active_vault_path(app_id: &str) -> Result<Option<PathBuf>> {
    validate_app_id(app_id)?;

    let Some(config_path) = active_vault_config_path(app_id, false)? else {
        return Ok(None);
    };
    let metadata = match fs::symlink_metadata(&config_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("could not inspect the active store setting"),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("active store setting is not a regular file");
    }

    let file =
        open_config_file(&config_path).context("could not open the active store setting safely")?;
    let metadata = file
        .metadata()
        .context("could not inspect the active store setting")?;
    ensure_owned_by_process(&metadata)?;
    ensure_private_config_file(&metadata)?;
    if metadata.len() > MAX_ACTIVE_VAULT_FILE_BYTES {
        bail!("active store setting exceeds the maximum size");
    }

    let mut contents = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_ACTIVE_VAULT_FILE_BYTES + 1)
        .read_to_end(&mut contents)
        .context("could not read the active store setting")?;
    if contents.len() as u64 > MAX_ACTIVE_VAULT_FILE_BYTES {
        bail!("active store setting exceeds the maximum size");
    }

    let config: ActiveVaultConfig =
        serde_json::from_slice(&contents).context("active store setting is invalid")?;
    let path = PathBuf::from(config.path);
    if !path.is_absolute() {
        bail!("active store path must be absolute");
    }
    validate_existing_database_path(&path)?;
    Ok(Some(path))
}

/// Atomically records a validated database path in the per-user config directory.
/// Returns `false` only when the rename succeeded but syncing the directory failed.
pub fn set_active_vault_path(app_id: &str, path: &Path) -> Result<bool> {
    validate_app_id(app_id)?;
    if !path.is_absolute() {
        bail!("active store path must be absolute");
    }
    validate_existing_database_path(path)?;

    let path = path
        .to_str()
        .context("active store path is not valid UTF-8")?;
    let contents = serde_json::to_vec(&ActiveVaultConfig {
        path: path.to_owned(),
    })
    .context("could not encode the active store setting")?;
    let config_path = active_vault_config_path(app_id, true)?
        .context("could not prepare the active store setting directory")?;
    let directory = config_path
        .parent()
        .context("active store setting path has no parent")?;
    write_config_atomically(&config_path, directory, &contents)
}

fn default_database_path(app_id: &str) -> Result<PathBuf> {
    validate_app_id(app_id)?;

    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|home| home.is_absolute())
                .map(|home| home.join(".local").join("share"))
        })
        .context("no absolute application data directory is available")?;

    fs::create_dir_all(&data_home).context("could not prepare the application data root")?;

    let app_directory = data_home.join(app_id);
    ensure_private_app_directory(&data_home, &app_directory)?;

    Ok(app_directory.join(DATABASE_FILE))
}

fn active_vault_config_path(app_id: &str, create: bool) -> Result<Option<PathBuf>> {
    let Some(config_home) = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|home| home.is_absolute())
                .map(|home| home.join(".config"))
        })
    else {
        if create {
            bail!("no absolute application config directory is available");
        }
        return Ok(None);
    };

    if create {
        fs::create_dir_all(&config_home)
            .context("could not prepare the application config root")?;
    } else {
        match fs::symlink_metadata(&config_home) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => bail!("could not inspect the application config root"),
        }
    }

    let app_directory = config_home.join(app_id);
    if !create {
        match fs::symlink_metadata(&app_directory) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => bail!("could not inspect the application config directory"),
        }
    }
    ensure_private_app_directory(&config_home, &app_directory)?;
    Ok(Some(app_directory.join(ACTIVE_VAULT_FILE)))
}

fn validate_existing_database_path(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .context("selected store file does not exist or cannot be inspected")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("selected store path is not a regular file");
    }
    Ok(())
}

fn open_config_file(path: &Path) -> Result<fs::File> {
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?
    };

    #[cfg(not(unix))]
    let file = OpenOptions::new().read(true).open(path)?;

    Ok(file)
}

fn write_config_atomically(path: &Path, directory: &Path, contents: &[u8]) -> Result<bool> {
    let mut temporary_path = None;
    let mut temporary_file = None;
    for _ in 0..32 {
        let sequence = ACTIVE_VAULT_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = directory.join(format!(
            ".active-vault-{}-{sequence}.tmp",
            std::process::id()
        ));
        match create_private_config_file(&candidate) {
            Ok(file) => {
                temporary_path = Some(candidate);
                temporary_file = Some(file);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => bail!("could not create the active store setting safely"),
        }
    }
    let (temporary_path, mut temporary_file) = match (temporary_path, temporary_file) {
        (Some(path), Some(file)) => (path, file),
        _ => bail!("could not allocate an active store setting file"),
    };
    let write_result = (|| -> Result<()> {
        temporary_file
            .write_all(contents)
            .context("could not write the active store setting")?;
        temporary_file
            .sync_all()
            .context("could not flush the active store setting")?;
        Ok(())
    })();
    drop(temporary_file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    let result = (|| -> Result<bool> {
        fs::rename(&temporary_path, path).context("could not activate the selected store")?;
        let directory = match open_directory(directory) {
            Ok(directory) => directory,
            Err(_) => return Ok(false),
        };
        Ok(directory.sync_all().is_ok())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn create_private_config_file(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    options.open(path)
}

#[cfg(unix)]
fn ensure_private_config_file(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o7777 != 0o600 {
        bail!("active store setting permissions are not private");
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_config_file(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

fn validate_app_id(app_id: &str) -> Result<()> {
    if app_id.is_empty()
        || app_id == "."
        || app_id == ".."
        || app_id
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || ".-_".contains(character)))
        || Path::new(app_id)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("application identifier is invalid");
    }

    Ok(())
}

fn ensure_private_app_directory(data_root: &Path, path: &Path) -> Result<()> {
    let root = open_directory(data_root).context("could not inspect the application data root")?;
    let root_metadata = root
        .metadata()
        .context("could not inspect the application data root")?;
    ensure_owned_by_process(&root_metadata)?;
    ensure_data_root_not_writable_by_others(&root_metadata)?;

    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_private_directory(path)?;
        }
        Err(_) => bail!("could not inspect the application data directory"),
    }

    let app_directory =
        open_directory(path).context("application data path is not a real directory")?;
    let metadata = app_directory
        .metadata()
        .context("could not inspect the application data directory")?;
    ensure_owned_by_process(&metadata)?;
    set_private_directory_permissions(&app_directory, &metadata)?;

    Ok(())
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let result = fs::DirBuilder::new().mode(0o700).create(path);
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(_) => bail!("could not create the application data directory"),
    }
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(_) => bail!("could not create the application data directory"),
    }
}

#[cfg(unix)]
fn set_private_directory_permissions(directory: &fs::File, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o7777 != 0o700 {
        directory
            .set_permissions(fs::Permissions::from_mode(0o700))
            .context("could not secure the application data directory")?;
    }

    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(
    _directory: &fs::File,
    _metadata: &fs::Metadata,
) -> Result<()> {
    Ok(())
}

fn open_directory(path: &Path) -> Result<fs::File> {
    #[cfg(unix)]
    let directory = {
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .context("could not open directory without following links")?
    };

    #[cfg(not(unix))]
    let directory = OpenOptions::new()
        .read(true)
        .open(path)
        .context("could not open directory")?;

    let metadata = directory
        .metadata()
        .context("could not inspect the opened directory")?;
    if !metadata.is_dir() {
        bail!("path is not a directory");
    }
    Ok(directory)
}

#[cfg(unix)]
fn ensure_owned_by_process(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let effective_uid = unsafe { libc::geteuid() };
    if metadata.uid() != effective_uid {
        bail!("application data directory is not owned by the current user");
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_owned_by_process(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn ensure_data_root_not_writable_by_others(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o022 != 0 {
        bail!("application data root is writable by other users");
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_data_root_not_writable_by_others(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}
