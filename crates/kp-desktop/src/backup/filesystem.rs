//! Bounded file access, SQLite target protection, and private backup publication.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use uuid::Uuid;

use super::BackupPublishOutcome;

pub(super) fn prepare_destination(path: &Path, allow_existing_file: bool) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "backup path must be absolute");
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .context("backup path has no file name")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("/"));
    let parent = fs::canonicalize(parent).context("backup directory is unavailable")?;
    ensure!(
        open_real_directory(&parent)
            .context("backup parent is not a real directory")?
            .metadata()
            .context("could not inspect backup parent")?
            .is_dir(),
        "backup parent is not a directory"
    );
    let destination = parent.join(file_name);
    match fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            ensure!(
                allow_existing_file && metadata.is_file() && !metadata.file_type().is_symlink(),
                "backup destination already exists or is not a regular file"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => bail!("could not inspect backup destination"),
    }
    Ok(destination)
}

pub(super) fn prepare_existing_file(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "backup path must be absolute");
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .context("backup path has no file name")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("/"));
    let parent = fs::canonicalize(parent).context("backup directory is unavailable")?;
    let file = parent.join(file_name);
    let metadata = fs::symlink_metadata(&file).context("backup file is unavailable")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "backup input is not a regular file"
    );
    Ok(file)
}

pub(super) fn protect_store_target(database: &Path, destination: &Path) -> Result<()> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut candidate = database.as_os_str().to_os_string();
        candidate.push(suffix);
        ensure!(
            !same_path(Path::new(&candidate), destination),
            "backup path cannot replace the active store or its SQLite sidecars"
        );
    }

    if let Some(database_base) = sqlite_sidecar_base(destination) {
        ensure!(
            !file_has_sqlite_database_header(&database_base)?,
            "backup path cannot replace a SQLite database sidecar"
        );
    }

    if let Some(prefix) = read_file_prefix_if_present(destination)? {
        let sqlite_database = prefix.starts_with(b"SQLite format 3\0");
        let sqlite_wal = prefix
            .get(..4)
            .map(|magic| {
                magic == 0x377f_0682u32.to_be_bytes() || magic == 0x377f_0683u32.to_be_bytes()
            })
            .unwrap_or(false);
        let sqlite_journal = prefix.starts_with(&[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]);
        ensure!(
            !sqlite_database && !sqlite_wal && !sqlite_journal,
            "backup path cannot replace a SQLite database or journal"
        );
    }
    Ok(())
}

fn file_has_sqlite_database_header(path: &Path) -> Result<bool> {
    Ok(read_file_prefix_if_present(path)?
        .map(|prefix| prefix.starts_with(b"SQLite format 3\0"))
        .unwrap_or(false))
}

fn sqlite_sidecar_base(path: &Path) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let bytes = path.as_os_str().as_bytes();
        for suffix in [b"-wal".as_slice(), b"-shm", b"-journal"] {
            if let Some(base) = bytes.strip_suffix(suffix).filter(|base| !base.is_empty()) {
                return Some(PathBuf::from(OsString::from_vec(base.to_vec())));
            }
        }
        None
    }
    #[cfg(not(unix))]
    {
        let name = path.file_name()?.to_str()?;
        for suffix in ["-wal", "-shm", "-journal"] {
            if let Some(base) = name.strip_suffix(suffix).filter(|base| !base.is_empty()) {
                return Some(path.with_file_name(base));
            }
        }
        None
    }
}

fn read_file_prefix_if_present(path: &Path) -> Result<Option<Vec<u8>>> {
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("could not inspect backup destination"),
    };
    ensure!(
        before.is_file() && !before.file_type().is_symlink(),
        "backup destination is not a regular file"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .context("could not inspect backup destination")?;
    let opened = file
        .metadata()
        .context("could not inspect backup destination")?;
    ensure!(
        opened.is_file() && same_inode(&before, &opened),
        "backup destination changed while opening"
    );
    let mut prefix = Vec::with_capacity(16);
    file.take(16)
        .read_to_end(&mut prefix)
        .context("could not inspect backup destination")?;
    Ok(Some(prefix))
}

pub(super) fn read_bounded_file(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let before = fs::symlink_metadata(path).context("backup file is unavailable")?;
    ensure!(
        before.is_file() && !before.file_type().is_symlink() && before.len() <= maximum as u64,
        "backup file exceeds the maximum size or is not regular"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    let file = options.open(path).context("could not open backup file")?;
    let opened = file.metadata().context("could not inspect backup file")?;
    ensure!(
        opened.is_file() && opened.len() <= maximum as u64 && same_inode(&before, &opened),
        "backup file changed while opening"
    );
    let mut bounded = file.take(maximum as u64 + 1);
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    bounded
        .read_to_end(&mut bytes)
        .context("could not read backup file")?;
    ensure!(
        bytes.len() <= maximum,
        "backup file exceeds the maximum size"
    );
    Ok(bytes)
}

pub(super) fn write_backup_atomically(
    destination: &Path,
    bytes: &[u8],
) -> Result<BackupPublishOutcome> {
    let parent = destination
        .parent()
        .context("backup destination has no parent")?;
    let temporary = parent.join(format!(".kryptopass-backup-{}.tmp", Uuid::new_v4()));
    let mut guard = TemporaryFile(temporary.clone());
    let mut file = create_private_file(&temporary)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .context("could not write encrypted backup")?;
    drop(file);
    fs::rename(&temporary, destination).context("could not publish encrypted backup")?;
    guard.disarm();
    Ok(if sync_directory(parent).is_ok() {
        BackupPublishOutcome::Durable
    } else {
        BackupPublishOutcome::PublishedButDurabilityUnconfirmed
    })
}

fn create_private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options
        .open(path)
        .context("could not create a private backup file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .context("could not secure backup file permissions")?;
    }
    Ok(file)
}

pub(super) fn create_private_staging_directory(parent: &Path) -> Result<PathBuf> {
    for _ in 0..3 {
        let path = parent.join(format!(".kryptopass-restore-{}", Uuid::new_v4()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => bail!("could not create private restore staging directory"),
            }
        }
        #[cfg(not(unix))]
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => bail!("could not create private restore staging directory"),
        }
    }
    bail!("could not reserve a private restore staging directory")
}

pub(super) fn ensure_private_directory(path: &Path) -> Result<()> {
    let directory =
        open_real_directory(path).context("restore destination must be a private directory")?;
    let metadata = directory
        .metadata()
        .context("could not inspect restore destination")?;
    ensure!(metadata.is_dir(), "restore destination is not a directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        ensure!(
            metadata.uid() == unsafe { libc::geteuid() }
                && metadata.permissions().mode() & 0o7777 == 0o700,
            "restore destination must be owned by the current user with mode 0700"
        );
    }
    Ok(())
}

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    open_real_directory(path)
        .and_then(|directory| directory.sync_all())
        .context("could not sync backup directory")
}

fn open_real_directory(path: &Path) -> std::io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        File::open(path)
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
}

fn normalize_path(path: &Path) -> PathBuf {
    if let Ok(path) = fs::canonicalize(path) {
        return path;
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("/"));
    match (fs::canonicalize(parent), path.file_name()) {
        (Ok(parent), Some(file_name)) => parent.join(file_name),
        _ => path.to_path_buf(),
    }
}

fn same_inode(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        left.dev() == right.dev() && left.ino() == right.ino()
    }
    #[cfg(not(unix))]
    {
        left.file_type() == right.file_type() && left.len() == right.len()
    }
}

struct TemporaryFile(PathBuf);

impl TemporaryFile {
    fn disarm(&mut self) {
        self.0.clear();
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.0);
        }
    }
}
