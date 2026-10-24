//! A short-lived GPGME context and the key lookups shared by operations.

use std::{io::Cursor, path::Path};

use gpgme::{Context, Data, ExportMode, Key, KeyListMode, PinentryMode, Protocol};
use kryptopass_secure_memory::SensitiveBytes;

use crate::{
    error::{Error, InvalidInput, Result, Step, Unconfirmed, gpg},
    types::Fingerprint,
};

pub(crate) const MAX_KEY_BLOB_BYTES: usize = 16 * 1024 * 1024;

/// The oldest GnuPG the whole test suite passes on. Earlier engines lack
/// operations this crate relies on or misreport secret-key presence.
pub const MINIMUM_GNUPG: &str = "2.2.45";

/// One GPGME context. Operations open a fresh session per call, and another
/// one to verify the result, so no key listing outlives a change.
pub(crate) struct Session {
    pub context: Context,
}

impl Session {
    pub fn open(home: Option<&Path>) -> Result<Self> {
        let mut context =
            Context::from_protocol(Protocol::OpenPgp).map_err(gpg(Step::OpenContext))?;
        if let Some(home) = home {
            let home = home.to_str().ok_or(InvalidInput::Path)?;
            context
                .set_engine_home_dir(home)
                .map_err(gpg(Step::OpenContext))?;
        }
        let engine = context.engine_info();
        if !engine.check_version(MINIMUM_GNUPG) {
            return Err(Error::UnsupportedEngine {
                found: engine.version().unwrap_or("unknown").to_owned(),
                minimum: MINIMUM_GNUPG,
            });
        }
        context
            .set_pinentry_mode(PinentryMode::Ask)
            .map_err(gpg(Step::OpenContext))?;
        context
            .add_key_list_mode(KeyListMode::WITH_SECRET | KeyListMode::SIGS)
            .map_err(gpg(Step::OpenContext))?;
        Ok(Self { context })
    }

    /// The key whose primary fingerprint is exactly `fingerprint`. A subkey
    /// fingerprint does not match, although GPGME would resolve it.
    pub fn find(&mut self, fingerprint: &Fingerprint) -> Result<Option<Key>> {
        let key = match self.context.get_key(fingerprint.as_str()) {
            Ok(key) => key,
            Err(error) if error.code() == gpgme::Error::EOF.code() => return Ok(None),
            Err(error) => return Err(gpg(Step::ListKeys)(error)),
        };
        Ok((primary_fingerprint(&key).as_ref() == Some(fingerprint)).then_some(key))
    }

    pub fn require(&mut self, fingerprint: &Fingerprint) -> Result<Key> {
        self.find(fingerprint)?
            .ok_or_else(|| Error::KeyNotFound(fingerprint.clone()))
    }

    /// Exports into protected memory, refusing output beyond the blob limit.
    pub fn export(
        &mut self,
        fingerprint: &Fingerprint,
        mode: ExportMode,
    ) -> Result<SensitiveBytes> {
        let mut bytes = SensitiveBytes::new();
        {
            let writer = bytes.bounded_writer(MAX_KEY_BLOB_BYTES);
            let mut destination =
                Data::from_writer(writer).map_err(|error| gpg(Step::Export)(error.error()))?;
            self.context.set_armor(true);
            self.context
                .export([fingerprint.as_str()], mode, &mut destination)
                .map_err(gpg(Step::Export))?;
        }
        if bytes.is_empty() {
            return Err(Error::Unconfirmed(Unconfirmed::EmptyOutput));
        }
        Ok(bytes)
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<gpgme::ImportResult> {
        let data = Data::from_reader(Cursor::new(bytes))
            .map_err(|error| gpg(Step::Import)(error.error()))?;
        self.context.import(data).map_err(gpg(Step::Import))
    }

    /// The path of the gpg binary GPGME drives, so the CLI matches it.
    pub fn engine_program(&self) -> Option<String> {
        self.context.engine_info().path().ok().map(str::to_owned)
    }
}

pub(crate) fn primary_fingerprint(key: &Key) -> Option<Fingerprint> {
    key.fingerprint_raw()
        .and_then(|raw| Fingerprint::parse(&raw.to_string_lossy()).ok())
}

/// Refuses keys that cannot be edited as the user's own: GnuPG needs the
/// secret primary key with the certify capability.
pub(crate) fn require_own_primary(key: &Key, fingerprint: &Fingerprint) -> Result<()> {
    if key.is_revoked() || key.is_disabled() || key.is_invalid() {
        return Err(Error::KeyUnusable(fingerprint.clone()));
    }
    match key.primary_key() {
        Some(primary) if primary.is_secret() && primary.can_certify() => Ok(()),
        _ => Err(Error::NotCertifier(fingerprint.clone())),
    }
}

pub(crate) fn require_secret(key: &Key, fingerprint: &Fingerprint) -> Result<()> {
    if key.has_secret() {
        Ok(())
    } else {
        Err(Error::NoSecretMaterial(fingerprint.clone()))
    }
}

pub(crate) fn find_user_id<'key>(key: &'key Key, needle: &str) -> Option<gpgme::UserId<'key>> {
    key.user_ids().find(|user_id| {
        user_id
            .id_raw()
            .is_some_and(|raw| raw.to_string_lossy() == needle)
    })
}

/// Subkeys without the primary key, paired with GnuPG's 1-based edit index.
pub(crate) fn indexed_subkeys(key: &Key) -> impl Iterator<Item = (usize, gpgme::Subkey<'_>)> {
    key.subkeys()
        .skip(1)
        .enumerate()
        .map(|(index, subkey)| (index + 1, subkey))
}

pub(crate) fn subkey_fingerprint(subkey: &gpgme::Subkey<'_>) -> Option<Fingerprint> {
    subkey
        .fingerprint_raw()
        .and_then(|raw| Fingerprint::parse(&raw.to_string_lossy()).ok())
}
