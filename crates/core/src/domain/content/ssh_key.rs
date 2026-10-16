//! OpenSSH private-key payload and derived public metadata.

use crate::SensitiveText;
use std::fmt::Write;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use ssh_key::{HashAlg, PrivateKey};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::ensure_field_size;

const MAX_PRIVATE_KEY_BYTES: usize = 512 * 1024;
const MAX_PASSPHRASE_BYTES: usize = 64 * 1024;
const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_HOSTS: usize = 100;
const MAX_NOTES_BYTES: usize = 256 * 1024;

/// OpenSSH private key and its derived public metadata.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SshKeyContent {
    pub private_key: SensitiveText,
    pub passphrase: SensitiveText,
    pub public_key: SensitiveText,
    pub fingerprint: SensitiveText,
    pub algorithm: SensitiveText,
    pub user: SensitiveText,
    pub hosts: Vec<SensitiveText>,
    pub notes: SensitiveText,
}

impl SshKeyContent {
    /// Imports an OpenSSH private key, decrypting it in Rust when a passphrase is supplied.
    pub fn import(private_key: SensitiveText, passphrase: SensitiveText) -> Result<Self> {
        ensure!(
            !private_key.is_empty() && private_key.len() <= MAX_PRIVATE_KEY_BYTES,
            "OpenSSH private key is empty or exceeds the maximum size"
        );
        ensure_field_size("SSH key passphrase", &passphrase, MAX_PASSPHRASE_BYTES)?;
        let parsed = parse_private_key(&private_key, &passphrase)?;
        let public_key: SensitiveText = parsed
            .public_key()
            .to_openssh()
            .context("could not encode SSH public key")?
            .into();
        let mut fingerprint = SensitiveText::new();
        write!(
            &mut fingerprint,
            "{}",
            parsed.fingerprint(HashAlg::default())
        )?;
        let mut algorithm = SensitiveText::new();
        write!(&mut algorithm, "{}", parsed.algorithm())?;
        let user = SensitiveText::from(parsed.comment());

        Ok(Self {
            private_key,
            passphrase,
            public_key,
            fingerprint,
            algorithm,
            user,
            hosts: Vec::new(),
            notes: SensitiveText::new(),
        })
    }

    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            !self.private_key.is_empty() && self.private_key.len() <= MAX_PRIVATE_KEY_BYTES,
            "OpenSSH private key is empty or exceeds the maximum size"
        );
        ensure_field_size("SSH key passphrase", &self.passphrase, MAX_PASSPHRASE_BYTES)?;
        ensure_field_size("SSH key user", &self.user, MAX_TEXT_BYTES)?;
        ensure!(self.hosts.len() <= MAX_HOSTS, "too many SSH hosts");
        for host in &self.hosts {
            ensure!(
                !host.is_empty() && host.len() <= MAX_TEXT_BYTES,
                "SSH host is empty or exceeds the maximum size"
            );
        }
        ensure_field_size("SSH key notes", &self.notes, MAX_NOTES_BYTES)?;

        let parsed = parse_private_key(&self.private_key, &self.passphrase)?;
        let public_key: SensitiveText = parsed
            .public_key()
            .to_openssh()
            .context("could not derive SSH public key")?
            .into();
        let mut fingerprint = SensitiveText::new();
        write!(
            &mut fingerprint,
            "{}",
            parsed.fingerprint(HashAlg::default())
        )?;
        let mut algorithm = SensitiveText::new();
        write!(&mut algorithm, "{}", parsed.algorithm())?;
        ensure!(
            self.public_key == public_key,
            "SSH public key does not match private key"
        );
        ensure!(
            self.fingerprint == fingerprint,
            "SSH fingerprint does not match private key"
        );
        ensure!(
            self.algorithm == algorithm,
            "SSH algorithm does not match private key"
        );
        Ok(())
    }
}

fn parse_private_key(private_key: &str, passphrase: &str) -> Result<PrivateKey> {
    let parsed =
        PrivateKey::from_openssh(private_key.as_bytes()).context("invalid OpenSSH private key")?;
    match parsed.is_encrypted() {
        true => {
            ensure!(
                !passphrase.is_empty(),
                "SSH private key requires a passphrase"
            );
            parsed
                .decrypt(passphrase.as_bytes())
                .context("could not decrypt OpenSSH private key")
        }
        false => {
            ensure!(
                passphrase.is_empty(),
                "passphrase supplied for an unencrypted SSH key"
            );
            Ok(parsed)
        }
    }
}
