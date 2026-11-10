//! The real pinentry that shows the dialog when nobody else answers.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Tried in order; the first executable one wins. Desktop-friendly first.
const CANDIDATES: &[&str] = &[
    "pinentry-qt",
    "pinentry-qt5",
    "pinentry-gnome3",
    "pinentry-gtk",
    "pinentry-gtk-2",
    "pinentry-curses",
    "pinentry-tty",
    "pinentry",
];

/// Overrides the search, for example to test with `pinentry-tty`.
const OVERRIDE: &str = "KRYPTOPASS_PINENTRY_FALLBACK";

/// Finds the fallback, never this program itself (that would loop forever).
pub fn find() -> Option<PathBuf> {
    let me = std::env::current_exe().and_then(std::fs::canonicalize).ok();
    let is_me = |path: &Path| {
        let real = std::fs::canonicalize(path).ok();
        real.is_some() && real == me
    };
    if let Some(chosen) = std::env::var_os(OVERRIDE).map(PathBuf::from) {
        if chosen.is_absolute() && is_executable(&chosen) && !is_me(&chosen) {
            return Some(chosen);
        }
        crate::log::write(format_args!(
            "{OVERRIDE}={} is not an absolute executable path other than this pinentry; searching instead",
            chosen.display()
        ));
    }
    let path = std::env::var_os("PATH").unwrap_or_else(|| "/usr/local/bin:/usr/bin".into());
    CANDIDATES.iter().find_map(|name| {
        std::env::split_paths(&path)
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(name))
            .find(|candidate| is_executable(candidate) && !is_me(candidate))
    })
}

/// Starts the fallback with the same arguments gpg-agent gave us. It dies with
/// this process, so a cancelled request never leaves a dialog behind.
pub fn spawn(program: &Path, arguments: &[std::ffi::OsString]) -> std::io::Result<Child> {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    // SAFETY: prctl is async-signal-safe and touches no memory of the parent.
    unsafe {
        command.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn()
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}
