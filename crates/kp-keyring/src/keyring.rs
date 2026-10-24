//! The public entry point: one method per keyring operation.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

use gpgme::{CreateKeyFlags, DeleteKeyFlags, ExportMode, KeySigningFlags};

use crate::{
    backend::{self, Session},
    error::{Error, InvalidInput, Result, Step, Unconfirmed, gpg},
    files,
    gpg_cli::Gpg,
    model::KeyInfo,
    revocation,
    types::{
        CertificationKind, DeleteScope, Expiry, ExpiryTarget, Fetched, Fingerprint, ImportReport,
        Keyserver, MAX_LISTED_ITEMS, Published, SubkeyUsage, UserId,
    },
};

/// A handle on one GnuPG home. It holds no open context: each call opens
/// its own, so the handle is cheap to clone and safe to keep around.
#[derive(Clone, Debug, Default)]
pub struct Keyring {
    home: Option<PathBuf>,
}

impl Keyring {
    /// The user's keyring, wherever GnuPG resolves it (`GNUPGHOME` or
    /// `~/.gnupg`).
    pub fn system() -> Self {
        Self::default()
    }

    /// A keyring in an explicit GnuPG home, such as an isolated test home.
    /// The `gpg` subprocesses some operations need use the same home.
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home: Some(home.into()),
        }
    }

    fn session(&self) -> Result<Session> {
        Session::open(self.home.as_deref())
    }

    fn gpg(&self, session: &Session) -> Gpg {
        Gpg::new(session.engine_program(), self.home.as_deref())
    }

    /// The configured home, or the one GnuPG resolves by default.
    fn resolved_home(&self) -> Option<PathBuf> {
        self.home.clone().or_else(|| {
            gpgme::init()
                .get_dir_info("homedir")
                .ok()
                .map(PathBuf::from)
        })
    }

    // Reading

    pub fn list_keys(&self) -> Result<Vec<KeyInfo>> {
        let mut session = self.session()?;
        collect(session.context.keys().map_err(gpg(Step::ListKeys))?)
    }

    pub fn list_secret_keys(&self) -> Result<Vec<KeyInfo>> {
        let mut session = self.session()?;
        collect(session.context.secret_keys().map_err(gpg(Step::ListKeys))?)
    }

    pub fn key(&self, fingerprint: &Fingerprint) -> Result<Option<KeyInfo>> {
        let mut session = self.session()?;
        session
            .find(fingerprint)?
            .map(|key| KeyInfo::from_gpgme(&key).map_err(Error::Unconfirmed))
            .transpose()
    }

    /// Signs and discards a fixed message with the first secret key, which
    /// makes the agent ask for its passphrase unless it has it cached.
    pub fn probe_agent(&self) -> Result<()> {
        let mut session = self.session()?;
        let key = {
            let mut keys = session.context.secret_keys().map_err(gpg(Step::ListKeys))?;
            let key = keys.next().ok_or(Error::NoSecretKey)?;
            keys.finish().map_err(gpg(Step::ListKeys))?;
            key.map_err(gpg(Step::ListKeys))?
        };
        session
            .context
            .add_signer(&key)
            .map_err(gpg(Step::SelectSigner))?;
        let mut signature = Vec::new();
        session
            .context
            .sign_detached("KryptoPass agent probe", &mut signature)
            .map_err(gpg(Step::Sign))?;
        Ok(())
    }

    // Vault lifecycle

    /// Creates a sign+certify primary key plus an encryption subkey, both
    /// with the same expiry.
    pub fn create_key(&self, user_id: &UserId, expiry: Expiry) -> Result<Fingerprint> {
        let expires = expiry.duration()?;
        let mut session = self.session()?;
        let primary_flags = with_expiry(CreateKeyFlags::SIGN | CreateKeyFlags::CERT, expiry);
        let fingerprint = {
            let generated = session
                .context
                .create_key_with_flags(user_id.as_str(), "default", expires, primary_flags)
                .map_err(gpg(Step::CreateKey))?;
            generated
                .fingerprint()
                .ok()
                .and_then(|raw| Fingerprint::parse(raw).ok())
                .ok_or(Error::Unconfirmed(Unconfirmed::MissingFingerprint))?
        };

        let key = session.find(&fingerprint)?.ok_or_else(|| {
            Error::Unconfirmed(Unconfirmed::CreatedKeyMissing(fingerprint.clone()))
        })?;
        backend::require_own_primary(&key, &fingerprint)?;
        // GnuPG's "default" algorithm may already add an encryption subkey.
        let has_encryption = key
            .subkeys()
            .skip(1)
            .any(|subkey| subkey.can_encrypt() && !subkey.is_revoked());
        if !has_encryption {
            session
                .context
                .create_subkey_with_flags(
                    &key,
                    "default",
                    expires,
                    with_expiry(CreateKeyFlags::ENCR, expiry),
                )
                .map_err(|error| match gpg(Step::AddSubkey)(error) {
                    Error::Gpg { source, .. } => Error::EncryptionSubkeyFailed {
                        fingerprint: fingerprint.clone(),
                        source,
                    },
                    other => other,
                })?;
        }
        Ok(fingerprint)
    }

    pub fn import_file(&self, path: &Path) -> Result<ImportReport> {
        files::check_path(path)?;
        let bytes = files::read_bounded(path, files::MAX_INPUT_FILE_BYTES)?;
        self.import(&bytes)
    }

    /// Imports keys or revocations. Fails only when nothing was applied; a
    /// partial import succeeds with [`ImportReport::is_partial`] set.
    pub fn import(&self, bytes: &[u8]) -> Result<ImportReport> {
        let mut session = self.session()?;
        let result = session.import(bytes)?;
        let mut report = ImportReport {
            considered: result.considered(),
            imported: result.imported() + result.secret_imported(),
            unchanged: result.unchanged() + result.secret_unchanged(),
            new_user_ids: result.new_user_ids(),
            new_subkeys: result.new_subkeys(),
            new_signatures: result.new_signatures(),
            new_revocations: result.new_revocations(),
            not_imported: result.not_imported(),
            without_user_id: result.without_user_id(),
            skipped_v3: result.skipped_v3_keys(),
            ..ImportReport::default()
        };
        if report.considered == 0 && report.new_revocations == 0 {
            return Err(Error::NothingToImport);
        }
        let mut first_success = None;
        for item in result.imports() {
            if item.result().is_err() {
                report.failed += 1;
            } else if first_success.is_none() {
                first_success = item
                    .fingerprint()
                    .ok()
                    .and_then(|raw| Fingerprint::parse(raw).ok());
            }
        }
        drop(result);
        if let Some(fingerprint) = first_success
            && session.find(&fingerprint)?.is_some()
        {
            report.first_key = Some(fingerprint);
        }
        if !report.changed_anything() {
            return Err(Error::ImportChangedNothing {
                not_imported: report.not_imported,
                failed: report.failed,
            });
        }
        Ok(report)
    }

    /// The armored public key, ready to paste or share.
    pub fn public_key_armored(&self, fingerprint: &Fingerprint) -> Result<String> {
        let mut session = self.session()?;
        session.require(fingerprint)?;
        let bytes = session.export(fingerprint, ExportMode::empty())?;
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::Unconfirmed(Unconfirmed::NotUtf8))
    }

    pub fn export_public_to(&self, fingerprint: &Fingerprint, path: &Path) -> Result<Published> {
        files::check_path(path)?;
        let mut session = self.session()?;
        session.require(fingerprint)?;
        let bytes = session.export(fingerprint, ExportMode::empty())?;
        Ok(files::publish_new(
            path,
            &bytes,
            self.resolved_home().as_deref(),
        )?)
    }

    /// Writes the armored secret key to a new owner-only file. The secret
    /// bytes stay in protected memory and are never returned.
    pub fn backup_secret_to(&self, fingerprint: &Fingerprint, path: &Path) -> Result<Published> {
        files::check_path(path)?;
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_secret(&key, fingerprint)?;
        let bytes = session.export(fingerprint, ExportMode::SECRET)?;
        Ok(files::publish_new(
            path,
            &bytes,
            self.resolved_home().as_deref(),
        )?)
    }

    /// Imports a key from a keyserver, or refreshes it when already present.
    /// Without `keyserver`, GnuPG's configured sources are used.
    pub fn fetch(
        &self,
        fingerprint: &Fingerprint,
        keyserver: Option<&Keyserver>,
    ) -> Result<Fetched> {
        let mut session = self.session()?;
        let present = session.find(fingerprint)?.is_some();
        let mut arguments = Vec::with_capacity(4);
        if let Some(keyserver) = keyserver {
            arguments.extend(["--keyserver", keyserver.as_str()]);
        }
        arguments.push(if present {
            "--refresh-keys"
        } else {
            "--recv-keys"
        });
        arguments.push(fingerprint.as_str());
        self.gpg(&session).run_batch(&arguments)?;

        let mut verification = self.session()?;
        match verification.find(fingerprint)? {
            Some(_) if present => Ok(Fetched::Refreshed),
            Some(_) => Ok(Fetched::Imported),
            None => Err(Error::Unconfirmed(Unconfirmed::NotFetched(
                fingerprint.clone(),
            ))),
        }
    }

    // Key management

    pub fn change_passphrase(&self, fingerprint: &Fingerprint) -> Result<()> {
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_secret(&key, fingerprint)?;
        session
            .context
            .change_key_passphrase(&key)
            .map_err(gpg(Step::ChangePassphrase))
    }

    pub fn set_expiry(
        &self,
        fingerprint: &Fingerprint,
        target: &ExpiryTarget,
        expiry: Expiry,
    ) -> Result<()> {
        let expires = expiry.duration()?;
        let subkeys = target.validated_subkeys()?;
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_own_primary(&key, fingerprint)?;
        let owned: HashSet<Fingerprint> = backend::indexed_subkeys(&key)
            .filter_map(|(_, subkey)| backend::subkey_fingerprint(&subkey))
            .collect();
        if let Some(stranger) = subkeys.iter().find(|subkey| !owned.contains(*subkey)) {
            return Err(Error::SubkeyNotFound {
                key: fingerprint.clone(),
                subkey: stranger.clone(),
            });
        }
        session
            .context
            .set_expire(&key, expires, subkeys.iter().map(Fingerprint::as_str))
            .map_err(gpg(Step::SetExpiry))
    }

    pub fn add_user_id(&self, fingerprint: &Fingerprint, user_id: &UserId) -> Result<()> {
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_own_primary(&key, fingerprint)?;
        if backend::find_user_id(&key, user_id.as_str()).is_some() {
            return Err(Error::UserIdAlreadyPresent(fingerprint.clone()));
        }
        session
            .context
            .add_uid(&key, user_id.as_str())
            .map_err(gpg(Step::AddUserId))
    }

    pub fn set_primary_user_id(&self, fingerprint: &Fingerprint, user_id: &UserId) -> Result<()> {
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_own_primary(&key, fingerprint)?;
        require_active_user_id(&key, fingerprint, user_id)?;
        session
            .context
            .set_primary_uid(&key, user_id.as_str())
            .map_err(gpg(Step::SetPrimaryUserId))
    }

    pub fn revoke_user_id(&self, fingerprint: &Fingerprint, user_id: &UserId) -> Result<()> {
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_own_primary(&key, fingerprint)?;
        require_active_user_id(&key, fingerprint, user_id)?;
        session
            .context
            .revoke_uid(&key, user_id.as_str())
            .map_err(gpg(Step::RevokeUserId))
    }

    pub fn add_subkey(
        &self,
        fingerprint: &Fingerprint,
        usage: SubkeyUsage,
        expiry: Expiry,
    ) -> Result<()> {
        let expires = expiry.duration()?;
        let flags = match usage {
            SubkeyUsage::Encrypt => CreateKeyFlags::ENCR,
            SubkeyUsage::Sign => CreateKeyFlags::SIGN,
            SubkeyUsage::Authenticate => CreateKeyFlags::AUTH,
        };
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_own_primary(&key, fingerprint)?;
        session
            .context
            .create_subkey_with_flags(&key, "default", expires, with_expiry(flags, expiry))
            .map(drop)
            .map_err(gpg(Step::AddSubkey))
    }

    pub fn revoke_subkey(&self, fingerprint: &Fingerprint, subkey: &Fingerprint) -> Result<()> {
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_own_primary(&key, fingerprint)?;
        let (index, revoked) = backend::indexed_subkeys(&key)
            .find(|(_, candidate)| backend::subkey_fingerprint(candidate).as_ref() == Some(subkey))
            .map(|(index, candidate)| (index, candidate.is_revoked()))
            .ok_or_else(|| Error::SubkeyNotFound {
                key: fingerprint.clone(),
                subkey: subkey.clone(),
            })?;
        if revoked {
            return Err(Error::AlreadyRevoked(subkey.clone()));
        }
        revocation::revoke_subkey(&mut session.context, &key, index)?;

        let mut verification = self.session()?;
        let updated = verification.require(fingerprint)?;
        let confirmed = backend::indexed_subkeys(&updated).any(|(_, candidate)| {
            candidate.is_revoked()
                && backend::subkey_fingerprint(&candidate).as_ref() == Some(subkey)
        });
        if !confirmed {
            return Err(Error::Unconfirmed(Unconfirmed::SubkeyNotRevoked(
                subkey.clone(),
            )));
        }
        Ok(())
    }

    /// Saves a revocation certificate to keep offline, without revoking.
    pub fn write_revocation_certificate(
        &self,
        fingerprint: &Fingerprint,
        path: &Path,
    ) -> Result<Published> {
        files::check_path(path)?;
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_own_primary(&key, fingerprint)?;
        let certificate = self.revocation_certificate(&session, fingerprint)?;
        Ok(files::publish_new(
            path,
            &certificate,
            self.resolved_home().as_deref(),
        )?)
    }

    /// Revokes the key in the local keyring. Publishing the revocation is a
    /// separate, explicit step.
    pub fn revoke_key(&self, fingerprint: &Fingerprint) -> Result<()> {
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        backend::require_own_primary(&key, fingerprint)?;
        let certificate = self.revocation_certificate(&session, fingerprint)?;
        session.import(&certificate)?;

        let mut verification = self.session()?;
        if !verification.require(fingerprint)?.is_revoked() {
            return Err(Error::Unconfirmed(Unconfirmed::KeyNotRevoked(
                fingerprint.clone(),
            )));
        }
        Ok(())
    }

    /// Certifies `user_ids` of another key with the user's own `signer`.
    /// Returns how many user IDs were certified.
    pub fn certify(
        &self,
        target: &Fingerprint,
        signer: &Fingerprint,
        user_ids: &[UserId],
        kind: CertificationKind,
    ) -> Result<usize> {
        if target == signer {
            return Err(InvalidInput::SelfCertification.into());
        }
        if user_ids.is_empty() || user_ids.len() > MAX_LISTED_ITEMS {
            return Err(InvalidInput::UserIdCount.into());
        }
        let mut seen = HashSet::with_capacity(user_ids.len());
        if !user_ids.iter().all(|user_id| seen.insert(user_id)) {
            return Err(InvalidInput::DuplicateUserId.into());
        }

        let mut session = self.session()?;
        let target_key = session.require(target)?;
        if target_key.is_revoked() || target_key.is_invalid() {
            return Err(Error::KeyUnusable(target.clone()));
        }
        let signer_key = session.require(signer)?;
        backend::require_own_primary(&signer_key, signer)?;
        if signer_key.is_expired() {
            return Err(Error::KeyExpired(signer.clone()));
        }
        for user_id in user_ids {
            match backend::find_user_id(&target_key, user_id.as_str()) {
                Some(found) if found.is_revoked() => return Err(Error::UserIdRevoked),
                Some(found) if found.is_invalid() => return Err(Error::UserIdInvalid),
                Some(_) => {}
                None => return Err(Error::UserIdNotFound(target.clone())),
            }
        }
        session
            .context
            .add_signer(&signer_key)
            .map_err(gpg(Step::SelectSigner))?;
        let flags = match kind {
            CertificationKind::Local => KeySigningFlags::LOCAL,
            CertificationKind::Exportable => KeySigningFlags::empty(),
        };
        session
            .context
            .sign_key_with_flags(
                &target_key,
                user_ids.iter().map(UserId::as_str),
                Duration::ZERO,
                flags,
            )
            .map_err(gpg(Step::Certify))?;
        Ok(user_ids.len())
    }

    pub fn delete(&self, fingerprint: &Fingerprint, scope: DeleteScope) -> Result<()> {
        let mut session = self.session()?;
        let key = session.require(fingerprint)?;
        let has_secret = key.has_secret();
        match scope {
            DeleteScope::PublicOnly if has_secret => {
                Err(Error::SecretMaterialPresent(fingerprint.clone()))
            }
            DeleteScope::SecretOnly if !has_secret => {
                Err(Error::NoSecretMaterial(fingerprint.clone()))
            }
            DeleteScope::SecretOnly => {
                // GPGME cannot delete only the secret part.
                self.gpg(&session).run_batch(&[
                    "--yes",
                    "--delete-secret-keys",
                    fingerprint.as_str(),
                ])?;
                match self.session()?.find(fingerprint)? {
                    Some(remaining) if !remaining.has_secret() => Ok(()),
                    Some(_) => Err(Error::Unconfirmed(Unconfirmed::SecretStillPresent(
                        fingerprint.clone(),
                    ))),
                    None => Err(Error::Unconfirmed(Unconfirmed::PublicPartDeleted(
                        fingerprint.clone(),
                    ))),
                }
            }
            DeleteScope::PublicOnly | DeleteScope::Everything => {
                let mut flags = DeleteKeyFlags::FORCE;
                if has_secret {
                    flags |= DeleteKeyFlags::ALLOW_SECRET;
                }
                session
                    .context
                    .delete_key_with_flags(&key, flags)
                    .map_err(gpg(Step::Delete))?;
                if self.session()?.find(fingerprint)?.is_some() {
                    return Err(Error::Unconfirmed(Unconfirmed::KeyStillPresent(
                        fingerprint.clone(),
                    )));
                }
                Ok(())
            }
        }
    }

    /// gpg's pre-generated certificate when present, a fresh one otherwise.
    fn revocation_certificate(
        &self,
        session: &Session,
        fingerprint: &Fingerprint,
    ) -> Result<kryptopass_secure_memory::SensitiveBytes> {
        if let Some(home) = self.resolved_home() {
            let path = revocation::stored_certificate_path(&home, fingerprint.as_str());
            if let Some(certificate) = revocation::read_stored_certificate(&path)? {
                return Ok(certificate);
            }
        }
        self.gpg(session).generate_revocation(fingerprint)
    }
}

fn collect(mut keys: gpgme::context::Keys<'_>) -> Result<Vec<KeyInfo>> {
    let mut result = Vec::new();
    for key in keys.by_ref() {
        let key = key.map_err(gpg(Step::ListKeys))?;
        result.push(KeyInfo::from_gpgme(&key).map_err(Error::Unconfirmed)?);
    }
    keys.finish().map_err(gpg(Step::ListKeys))?;
    Ok(result)
}

fn with_expiry(flags: CreateKeyFlags, expiry: Expiry) -> CreateKeyFlags {
    if expiry.is_never() {
        flags | CreateKeyFlags::NOEXPIRE
    } else {
        flags
    }
}

fn require_active_user_id(
    key: &gpgme::Key,
    fingerprint: &Fingerprint,
    user_id: &UserId,
) -> Result<()> {
    match backend::find_user_id(key, user_id.as_str()) {
        Some(found) if found.is_revoked() => Err(Error::UserIdRevoked),
        Some(_) => Ok(()),
        None => Err(Error::UserIdNotFound(fingerprint.clone())),
    }
}
