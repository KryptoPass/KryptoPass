//! A private, append-only log of what gpg-agent asks, never of what is typed.
//!
//! stdout is the protocol and gpg-agent usually discards stderr, so the log goes
//! to `$XDG_STATE_HOME/kryptopass/pinentry.log` (or `~/.local/state/…`), mode
//! 0600. Logging failures never stop the pinentry.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Past this size the log starts over instead of growing forever.
const MAX_BYTES: u64 = 1024 * 1024;

static FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn open() {
    let Some(directory) = private_state_directory() else {
        return;
    };
    let path = directory.join("pinentry.log");
    let too_big = std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.len() > MAX_BYTES);
    let file = OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .mode(0o600)
        // Never write through a link someone left where the log goes.
        .custom_flags(libc::O_NOFOLLOW)
        .open(path);
    if let (Ok(file), Ok(mut slot)) = (file, FILE.lock()) {
        *slot = Some(file);
    }
}

/// `<state home>/kryptopass`, created 0700. The state home itself
/// (`~/.local/state`) is created with the usual permissions when missing:
/// other programs share it. An existing directory is only used if it is a
/// real directory of this user that nobody else can enter.
fn private_state_directory() -> Option<PathBuf> {
    let base = state_home()?;
    std::fs::create_dir_all(&base).ok()?;
    let directory = base.join("kryptopass");
    match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return None,
    }
    let metadata = std::fs::symlink_metadata(&directory).ok()?;
    // SAFETY: geteuid has no preconditions.
    let mine = metadata.uid() == unsafe { libc::geteuid() };
    (metadata.is_dir() && mine && metadata.permissions().mode() & 0o077 == 0).then_some(directory)
}

pub fn write(message: impl std::fmt::Display) {
    let Ok(mut slot) = FILE.lock() else {
        return;
    };
    let Some(file) = slot.as_mut() else {
        return;
    };
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    let _ = writeln!(file, "{millis} [{}] {message}", std::process::id());
}

fn state_home() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".local/state"))
        })
}
