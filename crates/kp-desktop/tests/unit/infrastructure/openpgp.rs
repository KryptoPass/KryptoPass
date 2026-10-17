#![cfg(unix)]

use super::GnuPgOpenPgp;
use anyhow::{Context, Result, bail, ensure};
use kryptopass_core::{KeyFingerprint, OpenPgp, RecipientSet};
use kryptopass_secure_memory::SensitiveText;
use std::{
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct TemporaryKeyring {
    path: PathBuf,
}

impl TemporaryKeyring {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("kp-gpg-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self { path })
    }

    fn cleanup(&self) -> Result<()> {
        let agent_result = Command::new("gpgconf")
            .arg("--homedir")
            .arg(&self.path)
            .args(["--kill", "gpg-agent"])
            .env("GNUPGHOME", &self.path)
            .output();
        // Directory cleanup is attempted even if agent shutdown fails.
        let directory_result = std::fs::remove_dir_all(&self.path);
        let output = agent_result?;
        ensure!(output.status.success(), "temporary agent cleanup failed");
        directory_result?;
        Ok(())
    }
}

impl Drop for TemporaryKeyring {
    fn drop(&mut self) {
        if self.path.exists()
            && let Err(error) = self.cleanup()
        {
            eprintln!("temporary test keyring cleanup failed: {error}");
        }
    }
}

fn tool_available(tool: &str) -> Result<bool> {
    match Command::new(tool).arg("--version").output() {
        Ok(output) => {
            ensure!(output.status.success(), "{tool} --version failed");
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[test]
fn protected_process_roundtrips_with_isolated_gpg_agent() -> Result<()> {
    for tool in ["gpg", "gpgconf"] {
        if !tool_available(tool)? {
            eprintln!("skipping isolated OpenPGP integration: {tool} is not installed");
            return Ok(());
        }
    }
    let keyring = TemporaryKeyring::new()?;
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "infrastructure::openpgp::tests::isolated_gpg_roundtrip",
            "--nocapture",
        ])
        .env("GNUPGHOME", &keyring.path)
        .env("KRYPTO_ISOLATED_GPG_TEST", &keyring.path)
        .output()?;
    // Cleanup precedes reporting child failure; Drop retries cleanup on every error path.
    keyring.cleanup()?;
    ensure!(
        output.status.success(),
        "isolated OpenPGP integration failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn gpg(home: &Path, args: &[&str]) -> Result<Output> {
    let output = Command::new("gpg")
        .arg("--homedir")
        .arg(home)
        .args(args)
        .env("GNUPGHOME", home)
        .output()?;
    ensure!(
        output.status.success(),
        "temporary gpg operation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

#[test]
fn isolated_gpg_roundtrip() -> Result<()> {
    let Some(home) = std::env::var_os("KRYPTO_ISOLATED_GPG_TEST") else {
        return Ok(());
    };
    ensure!(std::env::var_os("GNUPGHOME").as_ref() == Some(&home));
    let home = PathBuf::from(home);
    ensure!(
        home.file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("kp-gpg-"))
    );
    let limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: initialized limit is valid for this call; only this child is affected.
    ensure!(unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } == 0);
    crate::process_security::initialize();
    gpg(
        &home,
        &[
            "--batch",
            "--pinentry-mode",
            "loopback",
            "--passphrase",
            "",
            "--quick-generate-key",
            "memory-test@example.invalid",
            "default",
            "default",
            "0",
        ],
    )?;
    let listing = gpg(&home, &["--batch", "--with-colons", "--list-keys"])?;
    let listing = std::str::from_utf8(&listing.stdout)?;
    let fingerprint = listing
        .lines()
        .find_map(|line| {
            let mut fields = line.split(':');
            (fields.next() == Some("fpr"))
                .then(|| fields.nth(8))
                .flatten()
        })
        .context("generated key has no fingerprint")?;
    let recipients = RecipientSet::new(vec![KeyFingerprint::parse(fingerprint)?])?;
    let plaintext = SensitiveText::from(
        "{\"password\":\"secreto-ñ-🦀\\n\\\"\\\\\",\"notes\":\"memory-marker\"}",
    );
    let adapter = GnuPgOpenPgp;
    adapter.validate_recipients(&recipients)?;
    let ciphertext = adapter.encrypt(plaintext.as_bytes(), &recipients)?;
    ensure!(!ciphertext.is_empty());
    let decrypted = adapter.decrypt(&ciphertext)?;
    ensure!(
        decrypted.as_slice() == plaintext.as_bytes(),
        "OpenPGP roundtrip mismatch"
    );
    match adapter.decrypt_bounded(&ciphertext, ciphertext.len(), 8) {
        Ok(_) => bail!("bounded decryption accepted oversized plaintext"),
        Err(error) => ensure!(!format!("{error:?}").contains("memory-marker")),
    }
    // The failed bounded operation must not corrupt the keyring or a later decryption.
    ensure!(adapter.decrypt(&ciphertext)?.as_slice() == plaintext.as_bytes());
    Ok(())
}
