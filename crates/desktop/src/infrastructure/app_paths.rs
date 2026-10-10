//! Application-owned paths for persistent data.

use std::env;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

const DATABASE_FILE: &str = "kryptopass.sqlite3";

/// Returns the database path under the platform data directory.
///
/// `app_id` is supplied by the application identity owner so this module does
/// not duplicate the desktop ID. Only the app-specific directory is made
/// private; shared ancestors such as `~/.local/share` are never chmodded.
pub fn database_path(app_id: &str) -> Result<PathBuf> {
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
