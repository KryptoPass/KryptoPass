//! Bounded reads and atomic, owner-only file publication.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use kryptopass_secure_memory::SensitiveBytes;

use crate::{backend::MAX_KEY_BLOB_BYTES, error::FileError, types::Published};

pub(crate) const MAX_INPUT_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;

pub(crate) fn check_path(path: &Path) -> Result<(), crate::error::InvalidInput> {
    let length = path.as_os_str().len();
    if length == 0 || length > MAX_PATH_BYTES {
        return Err(crate::error::InvalidInput::Path);
    }
    Ok(())
}

/// Reads a regular file without following a final symlink, into protected
/// memory, and refuses anything larger than `limit`.
pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<SensitiveBytes, FileError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path).map_err(|source| FileError::Open {
        path: path.to_owned(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| FileError::Read {
        path: path.to_owned(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(FileError::NotRegular(path.to_owned()));
    }
    if metadata.len() > limit {
        return Err(FileError::TooLarge(path.to_owned()));
    }
    let mut bytes = SensitiveBytes::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    // Read one byte past the limit to catch a file that grew after `metadata`.
    std::io::copy(
        &mut Read::by_ref(&mut file).take(limit.saturating_add(1)),
        &mut bytes,
    )
    .map_err(|source| FileError::Read {
        path: path.to_owned(),
        source,
    })?;
    if bytes.len() as u64 > limit {
        return Err(FileError::TooLarge(path.to_owned()));
    }
    Ok(bytes)
}

/// Writes `bytes` to a new file at `path` and never replaces an existing one.
///
/// The content is fully written and synced to a mode-0600 temporary in the
/// same folder before it appears under its final name.
pub(crate) fn publish_new(
    path: &Path,
    bytes: &[u8],
    gnupg_home: Option<&Path>,
) -> Result<Published, FileError> {
    if bytes.len() > MAX_KEY_BLOB_BYTES {
        return Err(FileError::TooLarge(path.to_owned()));
    }
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| FileError::NotAFile(path.to_owned()))?;
    let requested_parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(requested_parent).map_err(|source| FileError::Destination {
        path: requested_parent.to_owned(),
        source,
    })?;
    let destination = parent.join(file_name);
    reject_protected(&parent, &destination, gnupg_home)?;
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(FileError::DestinationExists(destination));
    }

    let mut temporary = tempfile::Builder::new()
        .prefix(".kryptopass-export-")
        .suffix(".tmp")
        .tempfile_in(&parent)
        .map_err(FileError::Temporary)?;
    let write_error = |source| FileError::Write {
        path: destination.clone(),
        source,
    };
    temporary.write_all(bytes).map_err(write_error)?;
    temporary.as_file().sync_all().map_err(write_error)?;
    temporary.persist_noclobber(&destination).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            FileError::DestinationExists(destination.clone())
        } else {
            write_error(error.error)
        }
    })?;

    // The file is already visible. A folder-sync failure only affects crash
    // durability and must not look like a failure the caller would retry.
    let durable = File::open(&parent)
        .and_then(|folder| folder.sync_all())
        .is_ok();
    Ok(Published {
        path: destination,
        durable,
    })
}

fn reject_protected(
    parent: &Path,
    destination: &Path,
    gnupg_home: Option<&Path>,
) -> Result<(), FileError> {
    let database = destination
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "db" | "db3" | "sqlite" | "sqlite3"
            )
        });
    if database {
        return Err(FileError::DatabaseExtension(destination.to_owned()));
    }
    let inside_home = gnupg_home
        .and_then(|home| fs::canonicalize(home).ok())
        .is_some_and(|home| parent.starts_with(home));
    if inside_home {
        return Err(FileError::InsideGnupgHome(destination.to_owned()));
    }
    Ok(())
}

/// A private, mode-0700 folder that is removed when dropped.
pub(crate) fn private_temp_dir(prefix: &str) -> Result<tempfile::TempDir, FileError> {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .map_err(FileError::Temporary)
}
