#![cfg(unix)]

use super::GnuPgOpenPgp;
#[cfg(debug_assertions)]
use super::{IsolatedGnuPgOpenPgp, SyntheticFixture};
use anyhow::{Context, Result, bail, ensure};
use kryptopass_core::{KeyFingerprint, OpenPgp, RecipientSet};
use kryptopass_secure_memory::SensitiveText;
#[cfg(debug_assertions)]
use sha2::{Digest, Sha256};
#[cfg(debug_assertions)]
use std::fs;
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

struct TemporaryFixtureHome {
    root: PathBuf,
    home: PathBuf,
}

impl TemporaryFixtureHome {
    fn new() -> Result<Self> {
        let root =
            std::env::temp_dir().join(format!("kp-synthetic-gpg-fixture-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new().mode(0o700).create(&root)?;
        let home = root.join("home");
        std::fs::DirBuilder::new().mode(0o700).create(&home)?;
        Ok(Self { root, home })
    }

    fn cleanup(&self) -> Result<()> {
        let agent_result = Command::new("gpgconf")
            .arg("--homedir")
            .arg(&self.home)
            .args(["--kill", "gpg-agent"])
            .env("GNUPGHOME", &self.home)
            .output();
        let directory_result = std::fs::remove_dir_all(&self.root);
        let output = agent_result?;
        ensure!(
            output.status.success(),
            "temporary fixture agent cleanup failed"
        );
        directory_result?;
        Ok(())
    }
}

impl Drop for TemporaryFixtureHome {
    fn drop(&mut self) {
        if self.root.exists()
            && let Err(error) = self.cleanup()
        {
            eprintln!("temporary fixture keyring cleanup failed: {error}");
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

#[test]
#[cfg(debug_assertions)]
fn synthetic_fixture_roundtrips_without_preconfigured_ownertrust() -> Result<()> {
    for tool in ["gpg", "gpgconf"] {
        if !tool_available(tool)? {
            eprintln!("skipping synthetic OpenPGP fixture test: {tool} is not installed");
            return Ok(());
        }
    }

    let (synthetic_fingerprint, _fixture_sha256) = synthetic_fixture_metadata()?;
    let keyring = TemporaryFixtureHome::new()?;
    let fixture = SyntheticFixture::load()?;
    let adapter = IsolatedGnuPgOpenPgp::open_synthetic_fixture(keyring.home.clone(), fixture)?;
    let recipients = RecipientSet::new(vec![KeyFingerprint::parse(&synthetic_fingerprint)?])?;
    let plaintext = b"synthetic ownertrust-independent payload";

    adapter.validate_recipients(&recipients)?;
    let ciphertext = adapter.encrypt(plaintext, &recipients)?;
    ensure!(
        !ciphertext.is_empty(),
        "synthetic fixture encryption returned no data"
    );
    let decrypted = adapter.decrypt(&ciphertext)?;
    ensure!(
        decrypted.as_slice() == plaintext,
        "synthetic fixture OpenPGP roundtrip mismatch"
    );

    keyring.cleanup()?;
    Ok(())
}

#[test]
#[cfg(debug_assertions)]
fn synthetic_transfer_preparation_uses_manifest_key_and_fresh_decryptable_challenges() -> Result<()>
{
    for tool in ["gpg", "gpgconf"] {
        if !tool_available(tool)? {
            eprintln!("skipping synthetic transfer preparation test: {tool} is not installed");
            return Ok(());
        }
    }

    let (fingerprint, fixture_sha256) = synthetic_fixture_metadata()?;
    let expected_fingerprint = super::decode_fingerprint(&fingerprint)?;
    let first = super::prepare_synthetic_transfer(&fingerprint)?;
    let second = super::prepare_synthetic_transfer(&fingerprint)?;
    ensure!(
        first.fingerprint.as_slice() == expected_fingerprint.as_slice()
            && second.fingerprint.as_slice() == expected_fingerprint.as_slice(),
        "prepared transfer fingerprint does not match the local fixture manifest"
    );
    ensure!(
        format!(
            "{:x}",
            Sha256::digest(first.protected_secret_key.as_slice())
        ) == fixture_sha256
            && format!(
                "{:x}",
                Sha256::digest(second.protected_secret_key.as_slice())
            ) == fixture_sha256,
        "prepared protected key does not match the local fixture manifest"
    );
    ensure!(
        first.challenge_sha256 != second.challenge_sha256,
        "separate transfer preparations must use fresh challenge plaintexts"
    );

    let keyring = TemporaryFixtureHome::new()?;
    let crypto = IsolatedGnuPgOpenPgp::open_synthetic_fixture(
        keyring.home.clone(),
        SyntheticFixture::load()?,
    )?;
    let first_plaintext = crypto.decrypt(&first.encrypted_challenge)?;
    let second_plaintext = crypto.decrypt(&second.encrypted_challenge)?;
    ensure!(
        first_plaintext.len() == 32 && second_plaintext.len() == 32,
        "prepared challenges must decrypt to exactly 32 bytes"
    );
    ensure!(
        first_plaintext.as_slice() != second_plaintext.as_slice(),
        "separate transfer preparations must decrypt to different challenges"
    );
    let first_challenge_digest: [u8; 32] = Sha256::digest(first_plaintext.as_slice()).into();
    let second_challenge_digest: [u8; 32] = Sha256::digest(second_plaintext.as_slice()).into();
    ensure!(
        first_challenge_digest == first.challenge_sha256
            && second_challenge_digest == second.challenge_sha256,
        "prepared challenge digest must match the decrypted plaintext"
    );

    drop(first_plaintext);
    drop(second_plaintext);
    drop(crypto);
    keyring.cleanup()?;
    Ok(())
}

#[cfg(debug_assertions)]
fn synthetic_fixture_metadata() -> Result<(String, String)> {
    let manifest_path = PathBuf::from(env!("KP_SYNTHETIC_FIXTURE_MANIFEST"));
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let fingerprint = manifest
        .get("fingerprint")
        .and_then(serde_json::Value::as_str)
        .context("synthetic fixture manifest has no fingerprint")?;
    let sha256 = manifest
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .context("synthetic fixture manifest has no checksum")?;
    let size = manifest
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .context("synthetic fixture manifest has no size")?;
    ensure!(
        matches!(fingerprint.len(), 40 | 64)
            && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "synthetic fixture fingerprint is invalid"
    );
    ensure!(
        sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "synthetic fixture checksum is invalid"
    );
    let secret_key_path = manifest_path
        .parent()
        .context("synthetic fixture manifest has no parent directory")?
        .join("secret-key.pgp");
    ensure!(
        fs::metadata(secret_key_path)?.len() == size,
        "synthetic fixture size does not match its manifest"
    );
    Ok((
        fingerprint.to_ascii_uppercase(),
        sha256.to_ascii_lowercase(),
    ))
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
