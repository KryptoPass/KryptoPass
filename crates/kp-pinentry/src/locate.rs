//! Finds the real pinentry of this system.
//!
//! Whatever is chosen here shows the dialog and sees the passphrase, so the
//! search may look in many places but trusts only one kind of file: a program
//! owned by root, in a directory owned by root, neither writable by anyone
//! else. A user-writable `PATH` entry such as `~/.local/bin` can therefore be
//! searched without being trusted.
//!
//! Places, in order: where GnuPG says its default pinentry lives (`gpgconf`),
//! next to the gpg-agent that started us, the usual system directories and
//! `PATH`. In each, a graphical pinentry is preferred over a terminal one; the
//! distribution's default pinentry is the last resort.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::log;

/// Preferred flavors, graphical first.
const FLAVORS: &[&str] = &[
    "pinentry-qt",
    "pinentry-qt5",
    "pinentry-gnome3",
    "pinentry-gtk",
    "pinentry-gtk-2",
    "pinentry-curses",
    "pinentry-tty",
    "pinentry",
];

const SYSTEM_DIRECTORIES: &[&str] = &[
    "/usr/bin",
    "/usr/local/bin",
    "/bin",
    "/run/current-system/sw/bin",
    "/opt/homebrew/bin",
];

/// Debug builds only: forces a fallback, for example a test script.
const OVERRIDE: &str = "KRYPTOPASS_PINENTRY_FALLBACK";

/// The pinentry to run as fallback, never this program itself.
pub fn fallback() -> Option<PathBuf> {
    let me = std::env::current_exe().and_then(std::fs::canonicalize).ok();
    let is_me = |path: &Path| std::fs::canonicalize(path).ok() == me && me.is_some();

    // An environment variable must not choose who sees the passphrase in a
    // release build; gpg-agent's environment is not a boundary we control.
    if cfg!(debug_assertions)
        && let Some(forced) = std::env::var_os(OVERRIDE).map(PathBuf::from)
    {
        if forced.is_absolute() && is_executable(&forced) && !is_me(&forced) {
            log::write(format_args!(
                "debug build: fallback forced to {}",
                forced.display()
            ));
            return Some(forced);
        }
        log::write(format_args!(
            "{OVERRIDE} ignored: not another absolute executable"
        ));
    }

    let default = gnupg_default_pinentry();
    let mut directories: Vec<PathBuf> = Vec::new();
    let mut add = |directory: Option<PathBuf>| {
        if let Some(directory) = directory.filter(|path| path.is_absolute())
            && !directories.contains(&directory)
        {
            directories.push(directory);
        }
    };
    add(default
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf));
    add(parent_program_directory());
    for directory in SYSTEM_DIRECTORIES {
        add(Some(PathBuf::from(directory)));
    }
    for directory in std::env::var_os("PATH")
        .iter()
        .flat_map(std::env::split_paths)
    {
        add(Some(directory));
    }

    let found = FLAVORS
        .iter()
        .find_map(|flavor| {
            directories
                .iter()
                .map(|directory| directory.join(flavor))
                .find(|candidate| is_system_program(candidate) && !is_me(candidate))
        })
        .or_else(|| default.filter(|path| is_system_program(path) && !is_me(path)));
    match &found {
        Some(program) => log::write(format_args!("fallback found: {}", program.display())),
        None => log::write(format_args!(
            "no trusted pinentry in {} directories",
            directories.len()
        )),
    }
    found
}

/// The pinentry GnuPG was built to use, whatever `pinentry-program` says now.
fn gnupg_default_pinentry() -> Option<PathBuf> {
    let gpgconf = parent_program_directory()
        .into_iter()
        .chain(SYSTEM_DIRECTORIES.iter().map(PathBuf::from))
        .map(|directory| directory.join("gpgconf"))
        .find(|candidate| is_system_program(candidate))?;
    let output = Command::new(gpgconf)
        .arg("--list-components")
        .env_clear()
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    // `pinentry:Passphrase Entry:/usr/bin/pinentry`
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("pinentry:"))
        .and_then(|rest| rest.split_once(':'))
        .map(|(_, path)| PathBuf::from(path.trim()))
        .filter(|path| path.is_absolute())
}

/// The directory of the program that started us: gpg-agent, whose package
/// often ships next to the pinentry on systems without a common `bin`.
fn parent_program_directory() -> Option<PathBuf> {
    let parent = std::os::unix::process::parent_id();
    let program = std::fs::read_link(format!("/proc/{parent}/exe")).ok()?;
    program.parent().map(Path::to_path_buf)
}

/// Executable, owned by root and not writable by group or others, in a
/// directory with the same guarantees: a packaged program, not something a
/// user (this one included) could have dropped or swapped.
fn is_system_program(path: &Path) -> bool {
    let root_only = |metadata: &std::fs::Metadata| {
        metadata.uid() == 0 && metadata.permissions().mode() & 0o022 == 0
    };
    let Ok(real) = std::fs::canonicalize(path) else {
        return false;
    };
    let program_ok = std::fs::metadata(&real).is_ok_and(|metadata| {
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 && root_only(&metadata)
    });
    let directory_ok = real
        .parent()
        .and_then(|directory| std::fs::metadata(directory).ok())
        .is_some_and(|metadata| metadata.is_dir() && root_only(&metadata));
    program_ok && directory_ok
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}
