//! Validated, private persistence for browser associations.
//!
//! Callers serialize state updates around `load` and `save`; an individual
//! save is atomic, but concurrent snapshots could otherwise overwrite one
//! another's association changes.

use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};

use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_SIZE: usize = 64 * 1024;
const MAX_ASSOCIATIONS: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Store {
    version: u32,
    pub(crate) hash: String,
    pub(crate) associations: BTreeMap<String, String>,
}

impl Store {
    pub(crate) fn new() -> Self {
        Self::from_parts(Uuid::new_v4().to_string(), BTreeMap::new())
    }

    pub(crate) fn from_parts(hash: String, associations: BTreeMap<String, String>) -> Self {
        Self {
            version: FORMAT_VERSION,
            hash,
            associations,
        }
    }
}

/// Load the store, creating and durably writing a fresh identity when absent.
///
/// The parent directory must already exist, be owned by the effective user,
/// and have mode `0700`. Every parent path component is opened without
/// following symlinks.
pub(crate) fn load(path: &Path) -> Result<Store> {
    let (directory, file_name) = open_store_directory(path)?;
    let directory_metadata = directory
        .metadata()
        .context("inspect browser store directory")?;
    validate_directory(&directory_metadata)?;

    if let Some(store) = read_existing_store(&directory, &file_name)? {
        return Ok(store);
    }

    let store = Store::new();
    validate_store(&store)?;
    write_atomically(&directory, &file_name, &store, false)?;

    // Another cooperating process may have created the file after the first
    // check. The app serializes these calls, but re-read if that happened.
    match read_existing_store(&directory, &file_name)? {
        Some(persisted) => Ok(persisted),
        None => Err(anyhow!("browser store disappeared after writing")),
    }
}

/// Validate and atomically replace the persisted store.
///
/// The parent directory must already exist, be owned by the effective user,
/// and have mode `0700`. The caller serializes store updates so snapshots
/// cannot overwrite newer association changes.
pub(crate) fn save(path: &Path, store: &Store) -> Result<()> {
    validate_store(store)?;
    let (directory, file_name) = open_store_directory(path)?;
    let directory_metadata = directory
        .metadata()
        .context("inspect browser store directory")?;
    validate_directory(&directory_metadata)?;

    // Refuse to replace a file whose metadata or contents are untrusted or
    // malformed. The temp-file rename below also avoids truncating the live
    // file if writing or syncing fails.
    let _ = read_existing_store(&directory, &file_name)?;
    let _ = write_atomically(&directory, &file_name, store, true)?;
    Ok(())
}

fn validate_store(store: &Store) -> Result<()> {
    ensure!(
        store.version == FORMAT_VERSION,
        "unsupported browser store version {}",
        store.version
    );
    validate_uuid("store hash", &store.hash)?;
    ensure!(
        store.associations.len() <= MAX_ASSOCIATIONS,
        "browser store has more than {MAX_ASSOCIATIONS} associations"
    );

    for (id, encoded_key) in &store.associations {
        validate_uuid("association id", id)?;
        let decoded_key = STANDARD
            .decode(encoded_key)
            .with_context(|| format!("association {id} has invalid base64 key data"))?;
        ensure!(
            decoded_key.len() == 32,
            "association {id} key must decode to exactly 32 bytes"
        );
        ensure!(
            STANDARD.encode(&decoded_key) == *encoded_key,
            "association {id} key is not canonical base64"
        );
    }
    Ok(())
}

fn validate_uuid(label: &str, value: &str) -> Result<()> {
    let parsed = Uuid::parse_str(value).with_context(|| format!("invalid {label}"))?;
    ensure!(
        parsed.to_string() == value,
        "{label} must use canonical lowercase UUID format"
    );
    Ok(())
}

fn validate_directory(metadata: &std::fs::Metadata) -> Result<()> {
    ensure!(metadata.is_dir(), "browser store parent is not a directory");
    ensure!(
        metadata.uid() == unsafe { libc::geteuid() },
        "browser store parent is not owned by the effective user"
    );
    ensure!(
        metadata.mode() & 0o7777 == 0o700,
        "browser store parent must have mode 0700"
    );
    Ok(())
}

fn validate_file(file: &File) -> Result<()> {
    let metadata = file.metadata().context("inspect browser store file")?;
    ensure!(
        metadata.is_file(),
        "browser store path is not a regular file"
    );
    ensure!(
        metadata.uid() == unsafe { libc::geteuid() },
        "browser store file is not owned by the effective user"
    );
    ensure!(
        metadata.mode() & 0o7777 == 0o600,
        "browser store file must have mode 0600"
    );
    Ok(())
}

fn open_store_directory(path: &Path) -> Result<(File, CString)> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("browser store path has no file name"))?;
    let file_name = CString::new(file_name.as_bytes())
        .context("browser store file name contains a NUL byte")?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };

    let start = if parent.is_absolute() { c"/" } else { c"." };
    let start_fd = unsafe {
        libc::open(
            start.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if start_fd < 0 {
        return Err(std::io::Error::last_os_error()).context("open browser store path root");
    }
    let mut directory = unsafe { File::from_raw_fd(start_fd) };

    for component in parent.components() {
        let component = match component {
            Component::RootDir | Component::CurDir => continue,
            Component::ParentDir => {
                CString::new("..").context("encode browser store parent path component")?
            }
            Component::Normal(value) => CString::new(value.as_bytes())
                .context("browser store parent path contains a NUL byte")?,
            Component::Prefix(_) => bail!("unsupported browser store path prefix"),
        };
        let next_fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                component.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if next_fd < 0 {
            return Err(std::io::Error::last_os_error())
                .context("open browser store parent without following symlinks");
        }
        directory = unsafe { File::from_raw_fd(next_fd) };
    }

    Ok((directory, file_name))
}

fn read_existing_store(directory: &File, file_name: &CString) -> Result<Option<Store>> {
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            file_name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::NotFound {
            return Ok(None);
        }
        return Err(error).context("open browser store without following symlinks");
    }

    let file = unsafe { File::from_raw_fd(fd) };
    validate_file(&file)?;
    let bytes = read_bounded(file)?;
    let store: Store = serde_json::from_slice(&bytes).context("parse browser store JSON")?;
    validate_store(&store)?;
    Ok(Some(store))
}

fn read_bounded(file: File) -> Result<Vec<u8>> {
    let metadata = file.metadata().context("inspect browser store size")?;
    ensure!(
        metadata.len() <= MAX_FILE_SIZE as u64,
        "browser store exceeds the {MAX_FILE_SIZE}-byte limit"
    );

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_FILE_SIZE + 1) as u64)
        .read_to_end(&mut bytes)
        .context("read browser store")?;
    ensure!(
        bytes.len() <= MAX_FILE_SIZE,
        "browser store exceeds the {MAX_FILE_SIZE}-byte limit"
    );
    Ok(bytes)
}

fn write_atomically(
    directory: &File,
    file_name: &CString,
    store: &Store,
    replace_existing: bool,
) -> Result<bool> {
    let bytes = serde_json::to_vec(store).context("serialize browser store")?;
    ensure!(
        bytes.len() <= MAX_FILE_SIZE,
        "browser store exceeds the {MAX_FILE_SIZE}-byte limit"
    );

    let (temp_name, mut temp_file) = create_temp_file(directory)?;
    let mut temp_guard = TempFileGuard {
        directory,
        name: temp_name.clone(),
        renamed: false,
    };
    temp_file
        .write_all(&bytes)
        .context("write temporary browser store")?;
    temp_file
        .sync_all()
        .context("sync temporary browser store")?;

    match read_existing_store(directory, file_name)? {
        Some(_) if !replace_existing => return Ok(false),
        _ => {}
    }

    let rename_result = unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            temp_name.as_ptr(),
            directory.as_raw_fd(),
            file_name.as_ptr(),
        )
    };
    if rename_result < 0 {
        return Err(std::io::Error::last_os_error()).context("replace browser store atomically");
    }
    temp_guard.renamed = true;
    directory
        .sync_all()
        .context("sync browser store directory")?;
    Ok(true)
}

fn create_temp_file(directory: &File) -> Result<(CString, File)> {
    for _ in 0..8 {
        let temp_name = CString::new(format!(".kryptopass-browser-{}.tmp", Uuid::new_v4()))
            .context("encode temporary browser store file name")?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                temp_name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                0o600 as libc::mode_t,
            )
        };
        if fd < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                continue;
            }
            return Err(error).context("create temporary browser store");
        }

        let file = unsafe { File::from_raw_fd(fd) };
        let validation = (|| {
            if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } < 0 {
                return Err(std::io::Error::last_os_error())
                    .context("set temporary browser store mode");
            }
            validate_file(&file).context("validate temporary browser store")
        })();
        if let Err(error) = validation {
            unsafe {
                libc::unlinkat(directory.as_raw_fd(), temp_name.as_ptr(), 0);
            }
            return Err(error);
        }
        return Ok((temp_name, file));
    }
    bail!("could not allocate a unique temporary browser store file")
}

struct TempFileGuard<'a> {
    directory: &'a File,
    name: CString,
    renamed: bool,
}

impl Drop for TempFileGuard<'_> {
    fn drop(&mut self) {
        if !self.renamed {
            unsafe {
                libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0);
            }
        }
    }
}
