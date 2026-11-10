//! A private, append-only log of what gpg-agent asks, never of what is typed.
//!
//! stdout is the protocol and gpg-agent usually discards stderr, so the log goes
//! to `$XDG_STATE_HOME/kryptopass/pinentry.log` (or `~/.local/state/…`), mode
//! 0600. Logging failures never stop the pinentry.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Past this size the log starts over instead of growing forever.
const MAX_BYTES: u64 = 1024 * 1024;

static FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn open() {
    let Some(directory) = state_directory() else {
        return;
    };
    if std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .is_err()
    {
        return;
    }
    let path = directory.join("pinentry.log");
    let too_big = std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() > MAX_BYTES);
    let file = OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .mode(0o600)
        .open(path);
    if let (Ok(file), Ok(mut slot)) = (file, FILE.lock()) {
        *slot = Some(file);
    }
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

fn state_directory() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".local/state"))
        })?;
    Some(base.join("kryptopass"))
}
