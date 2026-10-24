//! Validated inputs and plain results of keyring operations.

use std::{
    collections::HashSet,
    fmt,
    num::NonZeroU32,
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::error::InvalidInput;

const MAX_USER_ID_BYTES: usize = 255;
const MAX_URL_BYTES: usize = 4096;
pub(crate) const MAX_LISTED_ITEMS: usize = 64;

/// A full OpenPGP fingerprint (v4 or v5/v6), stored in upper case.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fingerprint(String);

impl Fingerprint {
    pub fn parse(input: &str) -> Result<Self, InvalidInput> {
        let bytes = input.as_bytes();
        if !matches!(bytes.len(), 40 | 64) || !bytes.iter().all(u8::is_ascii_hexdigit) {
            return Err(InvalidInput::Fingerprint);
        }
        Ok(Self(input.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The trailing 16 digits GnuPG shows as the long key ID.
    pub fn key_id(&self) -> &str {
        &self.0[self.0.len() - 16..]
    }
}

impl FromStr for Fingerprint {
    type Err = InvalidInput;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

impl AsRef<str> for Fingerprint {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Fingerprint({})", self.0)
    }
}

/// A complete user ID such as `Ada <ada@example.org>`, kept byte for byte.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UserId(String);

impl UserId {
    pub fn parse(input: &str) -> Result<Self, InvalidInput> {
        if input.trim().is_empty()
            || input.len() > MAX_USER_ID_BYTES
            || input.chars().any(char::is_control)
        {
            return Err(InvalidInput::UserId);
        }
        Ok(Self(input.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Expiry {
    Never,
    InDays(NonZeroU32),
}

impl Expiry {
    /// Zero days means no expiry, matching the convention of the UI.
    pub fn from_days(days: u32) -> Self {
        NonZeroU32::new(days).map_or(Self::Never, Self::InDays)
    }

    pub(crate) fn is_never(self) -> bool {
        self == Self::Never
    }

    /// The relative duration GPGME expects, refused when the absolute date
    /// would overflow GnuPG's 32-bit timestamp.
    pub(crate) fn duration(self) -> Result<Duration, InvalidInput> {
        let Self::InDays(days) = self else {
            return Ok(Duration::ZERO);
        };
        let seconds = u64::from(days.get()) * 24 * 60 * 60;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| InvalidInput::ExpiryTooFar)?
            .as_secs();
        if now.saturating_add(seconds) > u64::from(u32::MAX) {
            return Err(InvalidInput::ExpiryTooFar);
        }
        Ok(Duration::from_secs(seconds))
    }
}

/// Which part of a key an expiry change applies to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpiryTarget {
    Primary,
    Subkeys(Vec<Fingerprint>),
}

impl ExpiryTarget {
    pub(crate) fn validated_subkeys(&self) -> Result<&[Fingerprint], InvalidInput> {
        let Self::Subkeys(subkeys) = self else {
            return Ok(&[]);
        };
        if subkeys.len() > MAX_LISTED_ITEMS {
            return Err(InvalidInput::TooManySubkeys);
        }
        let mut seen = HashSet::with_capacity(subkeys.len());
        if !subkeys.iter().all(|subkey| seen.insert(subkey)) {
            return Err(InvalidInput::DuplicateSubkey);
        }
        Ok(subkeys)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubkeyUsage {
    Encrypt,
    Sign,
    Authenticate,
}

/// What [`crate::Keyring::delete`] removes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteScope {
    /// The whole key; only allowed when no secret material is present.
    PublicOnly,
    /// The secret material, keeping the public key.
    SecretOnly,
    /// Public key and secret material.
    Everything,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CertificationKind {
    /// Stays in this keyring and is never exported.
    Local,
    Exportable,
}

/// A keyserver URL accepted by `gpg --keyserver`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Keyserver(String);

impl Keyserver {
    pub fn parse(input: &str) -> Result<Self, InvalidInput> {
        if input.len() > MAX_URL_BYTES || input.chars().any(char::is_whitespace) {
            return Err(InvalidInput::Keyserver);
        }
        let url = url::Url::parse(input).map_err(|_| InvalidInput::Keyserver)?;
        let has_host = url.host_str().is_some_and(|host| !host.is_empty());
        if !matches!(url.scheme(), "hkp" | "hkps" | "http" | "https")
            || !has_host
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(InvalidInput::Keyserver);
        }
        Ok(Self(input.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Counters from one import, as GnuPG reported them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImportReport {
    pub considered: u32,
    pub imported: u32,
    pub unchanged: u32,
    pub new_user_ids: u32,
    pub new_subkeys: u32,
    pub new_signatures: u32,
    pub new_revocations: u32,
    pub not_imported: u32,
    pub without_user_id: u32,
    pub skipped_v3: u32,
    /// Records GnuPG flagged with an error.
    pub failed: u32,
    /// First successfully imported key that is now in the keyring.
    pub first_key: Option<Fingerprint>,
}

impl ImportReport {
    /// Some input was skipped even though the import changed something.
    pub fn is_partial(&self) -> bool {
        self.not_imported > 0 || self.failed > 0 || self.without_user_id > 0 || self.skipped_v3 > 0
    }

    pub(crate) fn changed_anything(&self) -> bool {
        self.imported > 0
            || self.unchanged > 0
            || self.new_user_ids > 0
            || self.new_subkeys > 0
            || self.new_signatures > 0
            || self.new_revocations > 0
            || self.first_key.is_some()
    }
}

/// A file written atomically with owner-only permissions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Published {
    pub path: PathBuf,
    /// False when the file is visible but its folder could not be synced,
    /// so a power loss might still drop it. Do not retry: it already exists.
    pub durable: bool,
}

/// How [`crate::Keyring::fetch`] changed the keyring.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fetched {
    Imported,
    Refreshed,
}
