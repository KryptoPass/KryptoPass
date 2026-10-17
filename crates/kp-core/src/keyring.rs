//! A small, domain-friendly facade over the local OpenPGP keyring.
//!
//! The rest of KryptoPass should depend on this module instead of depending on
//! GPGME handles, iterators, or error types. This first slice is deliberately
//! read-only: it discovers public metadata and never exposes secret material.

use std::{error::Error, fmt};

/// A local OpenPGP keyring backed by GPGME.
pub struct Keyring {
    context: gpgme::Context,
}

impl fmt::Debug for Keyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Keyring").finish_non_exhaustive()
    }
}

impl Keyring {
    /// Opens the user's default OpenPGP keyring.
    pub fn open() -> Result<Self, KeyringError> {
        let mut context = gpgme::Context::from_protocol(gpgme::Protocol::OpenPgp)
            .map_err(KeyringError::backend)?;

        context
            .add_key_list_mode(gpgme::KeyListMode::WITH_SECRET | gpgme::KeyListMode::SIGS)
            .map_err(KeyringError::backend)?;

        Ok(Self { context })
    }

    /// Lists public keys available in the user's keyring.
    ///
    /// A returned key can still have a secret counterpart; check
    /// [`KeyInfo::has_secret`] for that distinction.
    pub fn list_keys(&mut self) -> Result<Vec<KeyInfo>, KeyringError> {
        let mut keys = self.context.keys().map_err(KeyringError::backend)?;
        let mut result = Vec::new();

        for key in keys.by_ref() {
            let key = key.map_err(KeyringError::backend)?;
            result.push(KeyInfo::from_gpgme(&key)?);
        }

        keys.finish().map_err(KeyringError::backend)?;
        Ok(result)
    }

    /// Lists only keys that have secret material in the user's keyring.
    ///
    /// No secret material is returned by this method.
    pub fn list_secret_keys(&mut self) -> Result<Vec<KeyInfo>, KeyringError> {
        let mut keys = self.context.secret_keys().map_err(KeyringError::backend)?;
        let mut result = Vec::new();

        for key in keys.by_ref() {
            let key = key.map_err(KeyringError::backend)?;
            result.push(KeyInfo::from_gpgme(&key)?);
        }

        keys.finish().map_err(KeyringError::backend)?;
        Ok(result)
    }

    /// Signs a fixed test message to exercise the system pinentry.
    ///
    /// This is intentionally a small integration probe for the desktop demo.
    /// The generated signature is discarded; no application secret is returned.
    pub fn sign_test_message(&mut self) -> Result<(), KeyringError> {
        self.context
            .set_pinentry_mode(gpgme::PinentryMode::Ask)
            .map_err(KeyringError::backend)?;

        let mut keys = self.context.secret_keys().map_err(KeyringError::backend)?;
        let key = keys
            .next()
            .ok_or(KeyringError::NoSecretKey)?
            .map_err(KeyringError::backend)?;
        keys.finish().map_err(KeyringError::backend)?;

        self.context
            .add_signer(&key)
            .map_err(KeyringError::backend)?;

        let mut signature = Vec::new();
        self.context
            .sign_detached("KryptoPass agent test", &mut signature)
            .map_err(KeyringError::backend)?;

        Ok(())
    }
}

/// Public metadata for one OpenPGP key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyInfo {
    pub fingerprint: String,
    pub user_ids: Vec<UserIdInfo>,
    pub subkeys: Vec<SubkeyInfo>,
    pub has_secret: bool,
    pub can_encrypt: bool,
    pub can_sign: bool,
    pub can_certify: bool,
    pub can_authenticate: bool,
    pub revoked: bool,
    pub expired: bool,
    pub disabled: bool,
    pub invalid: bool,
}

impl KeyInfo {
    fn from_gpgme(key: &gpgme::Key) -> Result<Self, KeyringError> {
        let fingerprint = key
            .fingerprint_raw()
            .ok_or(KeyringError::MissingFingerprint)?
            .to_string_lossy()
            .into_owned();

        let user_ids = key
            .user_ids()
            .map(UserIdInfo::from_gpgme)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            fingerprint,
            user_ids,
            subkeys: key.subkeys().map(SubkeyInfo::from_gpgme).collect(),
            has_secret: key.has_secret(),
            can_encrypt: key.can_encrypt(),
            can_sign: key.can_sign(),
            can_certify: key.can_certify(),
            can_authenticate: key.can_authenticate(),
            revoked: key.is_revoked(),
            expired: key.is_expired(),
            disabled: key.is_disabled(),
            invalid: key.is_invalid(),
        })
    }
}

/// Public metadata for one identity attached to an OpenPGP key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserIdInfo {
    pub signatures: Vec<CertificationInfo>,
    /// The complete user ID as stored by GnuPG, for example `Ada <ada@example.org>`.
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub comment: Option<String>,
    pub validity: KeyValidity,
    pub revoked: bool,
    pub invalid: bool,
}

impl UserIdInfo {
    fn from_gpgme(user_id: gpgme::UserId<'_>) -> Result<Self, KeyringError> {
        let id = user_id
            .id_raw()
            .map(|value| value.to_string_lossy().into_owned())
            .ok_or(KeyringError::MissingUserId)?;

        Ok(Self {
            signatures: user_id
                .signatures()
                .map(CertificationInfo::from_gpgme)
                .collect(),
            id,
            name: user_id
                .name_raw()
                .map(|value| value.to_string_lossy().into_owned()),
            email: user_id
                .email_raw()
                .map(|value| value.to_string_lossy().into_owned()),
            comment: user_id
                .comment_raw()
                .map(|value| value.to_string_lossy().into_owned()),
            validity: KeyValidity::from_gpgme(user_id.validity()),
            revoked: user_id.is_revoked(),
            invalid: user_id.is_invalid(),
        })
    }
}

/// Public parameters for a primary key or subkey; never includes private bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubkeyInfo {
    pub fingerprint: String,
    pub algorithm: String,
    pub expires_at: Option<u64>,
    pub can_encrypt: bool,
    pub can_sign: bool,
    pub can_certify: bool,
    pub can_authenticate: bool,
    pub revoked: bool,
    pub expired: bool,
    pub disabled: bool,
    pub invalid: bool,
}

impl SubkeyInfo {
    fn from_gpgme(key: gpgme::Subkey<'_>) -> Self {
        Self {
            fingerprint: key
                .fingerprint_raw()
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            algorithm: key
                .algorithm_name()
                .unwrap_or_else(|_| format!("{:?}", key.algorithm())),
            expires_at: key
                .expiration_time()
                .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|v| v.as_secs()),
            can_encrypt: key.can_encrypt(),
            can_sign: key.can_sign(),
            can_certify: key.can_certify(),
            can_authenticate: key.can_authenticate(),
            revoked: key.is_revoked(),
            expired: key.is_expired(),
            disabled: key.is_disabled(),
            invalid: key.is_invalid(),
        }
    }
}

/// A recorded certification on a user ID, without implying identity verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificationInfo {
    pub signer_key_id: String,
    pub signer: String,
    pub revoked: bool,
    pub expired: bool,
    pub invalid: bool,
}

impl CertificationInfo {
    fn from_gpgme(signature: gpgme::UserIdSignature<'_>) -> Self {
        Self {
            signer_key_id: signature
                .signer_key_id_raw()
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            signer: signature
                .signer_user_id_raw()
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            revoked: signature.is_revocation(),
            expired: signature.is_expired(),
            invalid: signature.is_invalid(),
        }
    }
}

/// GPG's trust classification for a user ID, without exposing GPGME's enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyValidity {
    Unknown,
    Undefined,
    Never,
    Marginal,
    Full,
    Ultimate,
}

impl KeyValidity {
    fn from_gpgme(validity: gpgme::Validity) -> Self {
        match validity {
            gpgme::Validity::Unknown => Self::Unknown,
            gpgme::Validity::Undefined => Self::Undefined,
            gpgme::Validity::Never => Self::Never,
            gpgme::Validity::Marginal => Self::Marginal,
            gpgme::Validity::Full => Self::Full,
            gpgme::Validity::Ultimate => Self::Ultimate,
        }
    }
}

/// Errors returned by the keyring facade.
#[derive(Debug)]
pub enum KeyringError {
    Backend(String),
    MissingFingerprint,
    MissingUserId,
    NoSecretKey,
}

impl KeyringError {
    fn backend(error: gpgme::Error) -> Self {
        Self::Backend(error.to_string())
    }
}

impl fmt::Display for KeyringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(message) => write!(formatter, "GPG backend error: {message}"),
            Self::MissingFingerprint => formatter.write_str("GPG key has no fingerprint"),
            Self::MissingUserId => formatter.write_str("GPG key has a user ID without a value"),
            Self::NoSecretKey => formatter.write_str("No secret OpenPGP key is available"),
        }
    }
}

impl Error for KeyringError {}
