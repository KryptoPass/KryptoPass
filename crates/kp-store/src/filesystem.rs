//! Private store files, ownership checks, and filesystem permissions.
//!
//! Copied unchanged from `kp-desktop/src/storage/filesystem.rs`, which is
//! removed when the app switches to this store; until then both must change
//! together.

use anyhow::{Context, Result, bail, ensure};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

pub(crate) fn ensure_database_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let existed = match fs::symlink_metadata(parent) {
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(_) => bail!("could not inspect the store directory"),
    };
    if !existed {
        fs::create_dir_all(parent).context("could not create the store directory")?;
    }

    let directory = open_directory(parent)?;
    let metadata = directory
        .metadata()
        .context("could not inspect the store directory")?;
    ensure_owned_by_process(&metadata)?;
    if existed {
        ensure_private_directory(&metadata)?;
    } else {
        set_private_directory_permissions(&directory, &metadata)?;
    }
    Ok(())
}

pub(crate) fn ensure_existing_database_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let directory =
        open_directory(parent).context("existing store parent must be a real private directory")?;
    let metadata = directory
        .metadata()
        .context("could not inspect the existing store parent")?;
    ensure_owned_by_process(&metadata)?;
    ensure_private_directory(&metadata)
}

pub(crate) fn ensure_database_file_is_safe(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                !metadata.file_type().is_symlink() && metadata.is_file(),
                "store path must be a regular file"
            );
            let file = open_database_file(path)
                .map_err(|_| anyhow::anyhow!("could not inspect the store file"))?;
            let opened_metadata = file
                .metadata()
                .context("could not inspect the store file")?;
            validate_database_file(&opened_metadata)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => bail!("could not inspect the store file"),
    }
}

pub(crate) fn set_private_database_permissions(path: &Path) -> Result<()> {
    let file = open_database_file(path).context("could not inspect the store file")?;
    let metadata = file
        .metadata()
        .context("could not inspect the store file")?;
    validate_database_file(&metadata)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o7777 != 0o600 {
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .context("could not secure the store file")?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn ensure_private_directory(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    ensure!(
        metadata.permissions().mode() & 0o7777 == 0o700,
        "store directory must have private 0700 permissions"
    );
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_directory(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(directory: &File, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    ensure_owned_by_process(metadata)?;
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        directory
            .set_permissions(fs::Permissions::from_mode(0o700))
            .context("could not secure the store directory")?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_directory: &File, _metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

fn open_directory(path: &Path) -> Result<File> {
    #[cfg(unix)]
    let directory = {
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .context("could not open store directory without following links")?
    };

    #[cfg(not(unix))]
    let directory = OpenOptions::new()
        .read(true)
        .open(path)
        .context("could not open store directory")?;

    ensure!(
        directory
            .metadata()
            .context("could not inspect store directory")?
            .is_dir(),
        "store directory must be a real directory"
    );
    Ok(directory)
}

fn open_database_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)
    }

    #[cfg(not(unix))]
    {
        OpenOptions::new().read(true).open(path)
    }
}

fn validate_database_file(metadata: &fs::Metadata) -> Result<()> {
    ensure!(metadata.is_file(), "store path must be a regular file");
    ensure_single_link(metadata)?;
    ensure_owned_by_process(metadata)?;
    Ok(())
}

#[cfg(unix)]
fn ensure_owned_by_process(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let effective_uid = unsafe { libc::geteuid() };
    ensure!(
        metadata.uid() == effective_uid,
        "store path is not owned by the current user"
    );
    Ok(())
}

#[cfg(not(unix))]
fn ensure_owned_by_process(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn ensure_single_link(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    ensure!(metadata.nlink() == 1, "store file must not have hard links");
    Ok(())
}

#[cfg(not(unix))]
fn ensure_single_link(_metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}
