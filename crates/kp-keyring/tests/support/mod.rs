//! A throwaway GnuPG home driven by the real gpg, gpg-agent and a scripted
//! pinentry. Nothing in the backend is mocked; only the human typing the
//! passphrase is replaced.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use kryptopass_keyring::{Expiry, Fingerprint, KeyInfo, Keyring, UserId};
use tempfile::TempDir;

pub const PASSPHRASE: &str = "correct horse battery staple";

pub enum Pinentry {
    /// Answers every prompt with [`PASSPHRASE`].
    Answers,
    /// Dismisses every prompt, like a user pressing Cancel.
    Cancels,
}

pub struct GnupgHome {
    dir: TempDir,
}

impl GnupgHome {
    pub fn new() -> Self {
        Self::with(Pinentry::Answers)
    }

    pub fn with(pinentry: Pinentry) -> Self {
        // Under /tmp so the agent socket path stays short.
        let dir = tempfile::Builder::new()
            .prefix("kp-gnupg-")
            .tempdir_in("/tmp")
            .unwrap();
        let script = dir.path().join("pinentry.sh");
        let getpin = match pinentry {
            Pinentry::Answers => format!(
                r#"[ -n "$repeat" ] && echo "S PIN_REPEATED"; echo "D {PASSPHRASE}"; echo OK"#
            ),
            Pinentry::Cancels => "echo 'ERR 83886179 Operation cancelled'".to_owned(),
        };
        fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 echo 'OK scripted pinentry'\n\
                 repeat=\n\
                 while IFS= read -r line; do\n\
                   case \"$line\" in\n\
                     SETREPEAT*) repeat=1; echo OK ;;\n\
                     GETPIN*) {getpin} ;;\n\
                     BYE*) echo OK; exit 0 ;;\n\
                     *) echo OK ;;\n\
                   esac\n\
                 done\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            dir.path().join("gpg-agent.conf"),
            format!("pinentry-program {}\n", script.display()),
        )
        .unwrap();
        // Keep tests offline and deterministic.
        fs::write(dir.path().join("gpg.conf"), "no-auto-key-retrieve\n").unwrap();
        Self { dir }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn keyring(&self) -> Keyring {
        Keyring::with_home(self.path())
    }

    /// A scratch folder outside the GnuPG home for exported files.
    pub fn outbox(&self) -> PathBuf {
        let outbox = self.path().with_extension("out");
        fs::create_dir_all(&outbox).unwrap();
        outbox
    }

    pub fn create(&self, user_id: &str) -> Fingerprint {
        self.keyring()
            .create_key(&UserId::parse(user_id).unwrap(), Expiry::Never)
            .unwrap()
    }

    pub fn key(&self, fingerprint: &Fingerprint) -> KeyInfo {
        self.keyring()
            .key(fingerprint)
            .unwrap()
            .expect("key is in the keyring")
    }
}

impl Drop for GnupgHome {
    fn drop(&mut self) {
        let _ = Command::new("gpgconf")
            .arg("--homedir")
            .arg(self.path())
            .args(["--kill", "all"])
            .status();
        let _ = fs::remove_dir_all(self.path().with_extension("out"));
    }
}

pub fn user_id(value: &str) -> UserId {
    UserId::parse(value).unwrap()
}

pub fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}
