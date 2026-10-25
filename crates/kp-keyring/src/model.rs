//! Public metadata read from the keyring. Nothing here holds secret bytes.

use std::time::SystemTime;

use crate::{error::Unconfirmed, types::Fingerprint};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyInfo {
    pub fingerprint: Fingerprint,
    pub user_ids: Vec<UserIdInfo>,
    /// The primary key first, then its subkeys, in GnuPG's order.
    pub subkeys: Vec<SubkeyInfo>,
    pub has_secret: bool,
    /// GnuPG's ownertrust for the key. A key whose private part is here but whose
    /// ownertrust is not `Ultimate` is refused as an encryption recipient.
    pub owner_trust: Validity,
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
    pub(crate) fn from_gpgme(key: &gpgme::Key) -> Result<Self, Unconfirmed> {
        let fingerprint = key
            .fingerprint_raw()
            .and_then(|raw| Fingerprint::parse(&raw.to_string_lossy()).ok())
            .ok_or(Unconfirmed::MissingFingerprint)?;
        Ok(Self {
            fingerprint,
            user_ids: key.user_ids().map(UserIdInfo::from_gpgme).collect(),
            subkeys: key.subkeys().map(SubkeyInfo::from_gpgme).collect(),
            has_secret: key.has_secret(),
            owner_trust: Validity::from_gpgme(key.owner_trust()),
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

    /// Revoked, disabled, invalid and expired keys cannot be used.
    pub fn is_usable(&self) -> bool {
        !(self.revoked || self.disabled || self.invalid || self.expired)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserIdInfo {
    /// The complete user ID as GnuPG stores it.
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub comment: Option<String>,
    pub validity: Validity,
    pub revoked: bool,
    pub invalid: bool,
    pub certifications: Vec<CertificationInfo>,
}

impl UserIdInfo {
    fn from_gpgme(user_id: gpgme::UserId<'_>) -> Self {
        let text = |raw: Option<&std::ffi::CStr>| raw.map(|v| v.to_string_lossy().into_owned());
        Self {
            id: text(user_id.id_raw()).unwrap_or_default(),
            name: text(user_id.name_raw()),
            email: text(user_id.email_raw()),
            comment: text(user_id.comment_raw()),
            validity: Validity::from_gpgme(user_id.validity()),
            revoked: user_id.is_revoked(),
            invalid: user_id.is_invalid(),
            certifications: user_id
                .signatures()
                .map(CertificationInfo::from_gpgme)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubkeyInfo {
    /// Missing only for malformed keys GnuPG still lists.
    pub fingerprint: Option<Fingerprint>,
    pub algorithm: String,
    pub expires_at: Option<SystemTime>,
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
    fn from_gpgme(subkey: gpgme::Subkey<'_>) -> Self {
        Self {
            fingerprint: subkey
                .fingerprint_raw()
                .and_then(|raw| Fingerprint::parse(&raw.to_string_lossy()).ok()),
            algorithm: subkey
                .algorithm_name()
                .unwrap_or_else(|_| format!("{:?}", subkey.algorithm())),
            expires_at: subkey.expiration_time(),
            can_encrypt: subkey.can_encrypt(),
            can_sign: subkey.can_sign(),
            can_certify: subkey.can_certify(),
            can_authenticate: subkey.can_authenticate(),
            revoked: subkey.is_revoked(),
            expired: subkey.is_expired(),
            disabled: subkey.is_disabled(),
            invalid: subkey.is_invalid(),
        }
    }
}

/// A certification recorded on a user ID. Its presence does not mean anyone
/// verified the owner's identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificationInfo {
    pub signer_key_id: String,
    pub signer: String,
    pub revocation: bool,
    pub expired: bool,
    pub invalid: bool,
}

impl CertificationInfo {
    fn from_gpgme(signature: gpgme::UserIdSignature<'_>) -> Self {
        let text = |raw: Option<&std::ffi::CStr>| {
            raw.map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        Self {
            signer_key_id: text(signature.signer_key_id_raw()),
            signer: text(signature.signer_user_id_raw()),
            revocation: signature.is_revocation(),
            expired: signature.is_expired(),
            invalid: signature.is_invalid(),
        }
    }
}

/// GnuPG's validity classification for a user ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Validity {
    Unknown,
    Undefined,
    Never,
    Marginal,
    Full,
    Ultimate,
}

impl Validity {
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
