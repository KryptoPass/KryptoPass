//! Values derived from an OpenSSH private key.
//!
//! The public half and the fingerprint are readable from the key file even
//! when the private half is passphrase-protected, so deriving them never
//! needs the passphrase or decrypts anything.

use kp_template::{FieldType, Template};
use ssh_key::{HashAlg, PrivateKey};

/// The public values of a private key.
pub(super) struct DerivedKey {
    pub(super) public_key: String,
    pub(super) fingerprint: String,
}

impl DerivedKey {
    pub(super) fn from_private(private_key: &str) -> Option<Self> {
        let key = PrivateKey::from_openssh(private_key).ok()?;
        let public_key = key.public_key().to_openssh().ok()?;
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        Some(Self {
            public_key,
            fingerprint,
        })
    }

    /// The derived value a field of `kind` should hold.
    pub(super) fn value_for(&self, kind: FieldType) -> Option<&str> {
        match kind {
            FieldType::SshPublicKey => Some(&self.public_key),
            FieldType::SshFingerprint => Some(&self.fingerprint),
            _ => None,
        }
    }
}

/// The public values of an OpenSSH private key, for the editor to show before
/// saving. `None` when the text is not a valid key. Never needs the passphrase.
pub fn inspect_ssh_key(private_key: &str) -> Option<SshKeyInfo> {
    let key = PrivateKey::from_openssh(private_key).ok()?;
    Some(SshKeyInfo {
        public_key: key.public_key().to_openssh().ok()?,
        fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        algorithm: key.algorithm().to_string(),
    })
}

/// See [`inspect_ssh_key`].
pub struct SshKeyInfo {
    pub public_key: String,
    pub fingerprint: String,
    pub algorithm: String,
}

/// The template's private-key field id, if it has one.
pub(super) fn private_key_field(template: &Template) -> Option<&str> {
    template
        .fields()
        .find(|field| field.field_type == FieldType::SshPrivateKey)
        .map(|field| field.id.as_str())
}
