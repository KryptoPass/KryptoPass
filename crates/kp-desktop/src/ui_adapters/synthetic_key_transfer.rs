//! Preparation for the explicit OpenPGP key transfer to a paired phone: the
//! debug synthetic fixture, or the private identity explicitly chosen by the user.
//!
//! Key bytes stay in the backend; only the send-owner DTO is returned. This
//! module contains no QML properties. The fixture path never reads the user's
//! normal GnuPG keyring; the real-key path reads only the approved fingerprint.

use kryptopass_devices::{SyntheticKeyTransferSend, SyntheticTransferError};

#[cfg(debug_assertions)]
pub(crate) fn fingerprint() -> String {
    crate::infrastructure::openpgp::synthetic_transfer_fingerprint().unwrap_or_default()
}

#[cfg(not(debug_assertions))]
pub(crate) fn fingerprint() -> String {
    String::new()
}

#[cfg(debug_assertions)]
pub(crate) fn prepare(
    expected_fingerprint: &str,
) -> Result<([u8; 16], SyntheticKeyTransferSend), SyntheticTransferError> {
    use crate::infrastructure::openpgp;
    use uuid::Uuid;

    let mut material = openpgp::prepare_synthetic_transfer(expected_fingerprint)
        .map_err(|_| SyntheticTransferError::Invalid)?;
    let transfer_id = *Uuid::new_v4().as_bytes();
    let secret_key = std::mem::take(&mut *material.protected_secret_key);
    let prepared = SyntheticKeyTransferSend::new(
        transfer_id,
        material.fingerprint,
        secret_key,
        material.encrypted_challenge,
        material.challenge_sha256,
    )?;
    Ok((transfer_id, prepared))
}

#[cfg(not(debug_assertions))]
pub(crate) fn prepare(
    _expected_fingerprint: &str,
) -> Result<([u8; 16], SyntheticKeyTransferSend), SyntheticTransferError> {
    Err(SyntheticTransferError::Unsupported)
}

/// Fingerprint of the primary key of the active vault, the identity the phone
/// needs to open the same records. Empty when no vault or key is configured.
pub(crate) fn vault_identity_fingerprint() -> String {
    vault_primary_key()
        .map(|fingerprint| fingerprint.as_str().to_owned())
        .unwrap_or_default()
}

/// Exports only the exact private identity approved locally, without changing
/// the vault's primary key. GnuPG prompts for its passphrase before release.
pub(crate) fn prepare_selected_identity(
    expected_fingerprint: &str,
) -> Result<([u8; 16], SyntheticKeyTransferSend), SyntheticTransferError> {
    use crate::infrastructure::openpgp;
    use kryptopass_devices::KeyTransferPurpose;
    use uuid::Uuid;

    let fingerprint = kryptopass_core::KeyFingerprint::parse(expected_fingerprint)
        .map_err(|_| SyntheticTransferError::Invalid)?;
    if fingerprint.as_str().len() != 40 {
        return Err(SyntheticTransferError::Invalid);
    }
    // The adapter errors are static descriptions; they carry no key material.
    let mut material = openpgp::prepare_private_key_transfer(&fingerprint).map_err(|error| {
        tracing::warn!(error = %error, "Key transfer: selected identity export failed");
        SyntheticTransferError::Invalid
    })?;
    let transfer_id = *Uuid::new_v4().as_bytes();
    let secret_key = std::mem::take(&mut *material.protected_secret_key);
    let prepared = SyntheticKeyTransferSend::with_purpose(
        KeyTransferPurpose::VaultIdentity,
        transfer_id,
        material.fingerprint,
        secret_key,
        material.encrypted_challenge,
        material.challenge_sha256,
    )?;
    Ok((transfer_id, prepared))
}

/// Whether `offered` (hex) belongs to the active vault: it is the primary key or a
/// recipient of at least one of its secrets, so importing it restores access.
fn vault_owns_key(offered: &str) -> bool {
    use crate::storage::{SqliteStorage, paths};
    use kryptopass_core::Storage as _;

    let app_id = crate::app_info::APP.desktop_file_id;
    let path = match paths::active_vault_path(app_id).ok().flatten() {
        Some(path) => path,
        None => match paths::database_path(app_id) {
            Ok(path) => path,
            Err(_) => return false,
        },
    };
    if !path.exists() {
        return false;
    }
    let Ok(storage) = SqliteStorage::open_existing(&path) else {
        return false;
    };
    if storage
        .primary_key()
        .ok()
        .flatten()
        .is_some_and(|primary| primary.fingerprint.as_str().eq_ignore_ascii_case(offered))
    {
        return true;
    }
    storage.list_secret_metadata().is_ok_and(|secrets| {
        secrets.iter().any(|secret| {
            secret
                .recipients
                .as_slice()
                .iter()
                .any(|key| key.as_str().eq_ignore_ascii_case(offered))
        })
    })
}

/// Whether the active vault has any key to compare against.
fn vault_exists() -> bool {
    use crate::storage::paths;

    let app_id = crate::app_info::APP.desktop_file_id;
    match paths::active_vault_path(app_id).ok().flatten() {
        Some(path) => path.exists(),
        None => paths::database_path(app_id).is_ok_and(|path| path.exists()),
    }
}

fn vault_primary_key() -> Option<kryptopass_core::KeyFingerprint> {
    use crate::storage::{SqliteStorage, paths};

    let app_id = crate::app_info::APP.desktop_file_id;
    let path = match paths::active_vault_path(app_id).ok()? {
        Some(path) => path,
        None => paths::database_path(app_id).ok()?,
    };
    if !path.exists() {
        return None;
    }
    let storage = SqliteStorage::open_existing(&path).ok()?;
    Some(storage.primary_key().ok()??.fingerprint)
}

/// What accepting an inbound vault identity from the phone would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InboundAssessment {
    /// A key of the vault that is not in GnuPG: the import creates it.
    New,
    /// A key of the vault whose public part GnuPG has: secret parts are added.
    AddSecret,
    /// A key of the vault that GnuPG has: the import merges new material.
    Merge,
    /// Neither the vault's primary key nor a recipient of its secrets: rejected automatically.
    VaultMismatch,
    /// No active vault to compare against: rejected.
    NoVault,
    /// GnuPG could not be queried: rejected.
    Unavailable,
}

impl InboundAssessment {
    pub(crate) const fn allows_import(self) -> bool {
        matches!(self, Self::New | Self::AddSecret | Self::Merge)
    }

    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::AddSecret => "add_secret",
            Self::Merge => "merge",
            Self::VaultMismatch => "vault_mismatch",
            Self::NoVault => "no_vault",
            Self::Unavailable => "unavailable",
        }
    }

    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::New => "Es una clave de tu bóveda y no está en GnuPG: se creará al importarla.",
            Self::AddSecret => {
                "Es una clave de tu bóveda. GnuPG solo tiene su parte pública: se añadirá la parte privada."
            }
            Self::Merge => {
                "Es una clave de tu bóveda y ya está en GnuPG: se combinará (identidades, subclaves y firmas nuevas). No se borra nada."
            }
            Self::VaultMismatch => {
                "Esta clave no es la principal de la bóveda activa ni cifra ninguno de sus elementos. Se rechazó automáticamente."
            }
            Self::NoVault => {
                "No hay una bóveda activa con la que comparar. Se rechazó automáticamente."
            }
            Self::Unavailable => {
                "No se pudo consultar GnuPG para revisar la clave. Se rechazó automáticamente."
            }
        }
    }
}

/// Uppercase hex of a v4 fingerprint, as GnuPG prints it.
pub(crate) fn fingerprint_hex(fingerprint: &[u8; 20]) -> String {
    use std::fmt::Write;
    let mut value = String::with_capacity(40);
    for byte in fingerprint {
        let _ = write!(value, "{byte:02X}");
    }
    value
}

/// Compares the offered fingerprint with the active vault and GnuPG. Reads
/// only public metadata; never prompts.
pub(crate) fn assess_inbound(fingerprint: &[u8; 20]) -> InboundAssessment {
    use crate::infrastructure::openpgp::inbound_key::{ExistingKey, existing_key};

    let offered = fingerprint_hex(fingerprint);
    if !vault_exists() {
        tracing::warn!("Inbound key transfer: no active vault to compare");
        return InboundAssessment::NoVault;
    }
    if !vault_owns_key(&offered) {
        tracing::warn!(
            "Inbound key transfer: offered fingerprint is not a key of the active vault"
        );
        return InboundAssessment::VaultMismatch;
    }
    match existing_key(&offered) {
        Ok(ExistingKey::Absent) => InboundAssessment::New,
        Ok(ExistingKey::PublicOnly) => InboundAssessment::AddSecret,
        Ok(ExistingKey::WithSecret) => InboundAssessment::Merge,
        Err(error) => {
            tracing::warn!(error = %error, "Inbound key transfer: GnuPG lookup failed");
            InboundAssessment::Unavailable
        }
    }
}

/// Bounded, secret-free outcome of a failed inbound import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InboundImportFailure {
    pub(crate) stage: &'static str,
    pub(crate) reason: &'static str,
}

impl InboundImportFailure {
    pub(crate) const fn message(self) -> &'static str {
        match self.stage.as_bytes() {
            b"vault" => "La clave ya no pertenece a la bóveda activa. No se importó nada.",
            b"validate" => {
                "El bloque recibido no es una única clave privada protegida con contraseña con la huella ofrecida. No se importó nada."
            }
            b"import" => {
                "GnuPG no pudo importar la clave. Revisa GnuPG y vuelve a intentarlo desde el teléfono."
            }
            _ => {
                "La clave se importó en GnuPG, pero no se pudo descifrar el desafío (¿contraseña cancelada o incorrecta?). El teléfono no recibirá confirmación."
            }
        }
    }
}

/// Validates, imports into GnuPG and proves the received vault identity.
/// `stage` is called before importing and before decrypting the challenge.
/// Returns SHA-256 of the decrypted challenge for the receipt.
pub(crate) fn import_inbound(
    material: &kryptopass_devices::InboundKeyMaterial,
    mut stage: impl FnMut(&'static str),
) -> Result<[u8; 32], InboundImportFailure> {
    use crate::infrastructure::openpgp::inbound_key;

    let fingerprint = material.fingerprint();
    let offered = fingerprint_hex(&fingerprint);
    if !vault_owns_key(&offered) {
        return Err(InboundImportFailure {
            stage: "vault",
            reason: "offered key is not a key of the active vault",
        });
    }
    let to_failure = |error: inbound_key::InboundKeyError| InboundImportFailure {
        stage: error.stage,
        reason: error.reason,
    };
    inbound_key::validate_protected_secret_key(material.protected_key(), &fingerprint)
        .map_err(to_failure)?;
    stage("importing");
    inbound_key::import_protected_secret_key(material.protected_key(), &offered)
        .map_err(to_failure)?;
    tracing::info!("Inbound key transfer: imported into GnuPG, decrypting challenge");
    stage("verifying");
    inbound_key::decrypt_challenge(material.encrypted_challenge(), &offered).map_err(to_failure)
}
