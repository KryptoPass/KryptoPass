//! Explicit, debug-only Desktop side of the shared synthetic Sync exercise.
//! It uses its own selected ledger and GnuPG home and never reads the normal
//! mutable secret store or the user's OpenPGP keyring.

#[cfg(debug_assertions)]
use crate::{
    infrastructure::openpgp::{IsolatedGnuPgOpenPgp, SyntheticFixture},
    vault_sync::{SyncedVault, VaultHeadProjection, VaultSyncError},
};
#[cfg(debug_assertions)]
use kryptopass_core::{LoginContent, SecretContent, SecretId, SensitiveText};
use kryptopass_identity::DeviceId;
#[cfg(debug_assertions)]
use kryptopass_peers::{PeerHost, VaultBindingInfo};
#[cfg(debug_assertions)]
use kryptopass_sync::{Id, Operation};
#[cfg(debug_assertions)]
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

#[cfg(debug_assertions)]
const SYNTHETIC_VAULT_ID: Id = [
    0x5c, 0x0a, 0xa3, 0x60, 0x54, 0xc3, 0x4b, 0x7c, 0xaa, 0x4e, 0xdc, 0xf6, 0x3c, 0xdd, 0x8c, 0x27,
];

#[derive(Clone, Copy)]
pub(crate) enum SyntheticSyncAction {
    Activate,
    Refresh,
    CreateCredential,
}

pub(crate) struct SyntheticSyncSnapshot {
    pub(crate) local_revision_count: usize,
    pub(crate) rows: Vec<String>,
    pub(crate) status: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntheticSyncError {
    ProfileUnavailable,
    NoPeerSelected,
    PeerNotAuthorized,
    BindingConflict,
    NotConfigured,
    OpenPgpUnavailable,
    LedgerUnavailable,
    ContentUnavailable,
    WriteRejected,
    LimitExceeded,
    #[cfg(not(debug_assertions))]
    DebugBuildOnly,
}

impl SyntheticSyncError {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::ProfileUnavailable => {
                "No se pudo abrir el perfil autorizado. La bóveda normal no se modificó."
            }
            Self::NoPeerSelected => {
                "Selecciona un teléfono autorizado antes de usar la prueba sintética de Sync."
            }
            Self::PeerNotAuthorized => {
                "Este teléfono ya no está autorizado en el perfil de pruebas."
            }
            Self::BindingConflict => {
                "Este teléfono ya tiene otra bóveda de Sync seleccionada. No se reemplazó."
            }
            Self::NotConfigured => {
                "Sync no está configurado para este teléfono. Pulsa Activar prueba de Sync primero."
            }
            Self::OpenPgpUnavailable => {
                "No se pudo abrir la clave sintética en el llavero aislado. No se usó el llavero normal."
            }
            Self::LedgerUnavailable => {
                "No se pudo leer la bóveda sintética ni guardar la revisión local."
            }
            Self::ContentUnavailable => {
                "No se pudo descifrar una credencial sintética con la clave y el formato esperados."
            }
            Self::WriteRejected => {
                "La bóveda cambió durante la operación. Actualiza la prueba e inténtalo otra vez."
            }
            Self::LimitExceeded => "La operación superó un límite seguro de Sync.",
            #[cfg(not(debug_assertions))]
            Self::DebugBuildOnly => "La prueba sintética solo está disponible en Desktop debug.",
        }
    }
}

impl fmt::Debug for SyntheticSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SyntheticSyncError([redacted])")
    }
}

#[cfg(debug_assertions)]
pub(crate) fn run(
    profile_directory: &Path,
    peer: DeviceId,
    action: SyntheticSyncAction,
) -> Result<SyntheticSyncSnapshot, SyntheticSyncError> {
    let fixture = SyntheticFixture::load().map_err(|_| SyntheticSyncError::OpenPgpUnavailable)?;
    let fingerprint = fixture.fingerprint().clone();
    let fingerprint_bytes = fixture.fingerprint_bytes().to_vec();
    let host = PeerHost::open_existing_system_profile(profile_directory)
        .map_err(|_| SyntheticSyncError::ProfileUnavailable)?;

    let binding = match action {
        SyntheticSyncAction::Activate => select_binding(&host, peer, &fingerprint_bytes)?,
        SyntheticSyncAction::Refresh | SyntheticSyncAction::CreateCredential => {
            existing_binding(&host, peer, &fingerprint_bytes)?
        }
    };
    let engine_home = binding
        .ledger_path
        .parent()
        .ok_or(SyntheticSyncError::ProfileUnavailable)?
        .join("synthetic-sync-openpgp-v1");
    let crypto = IsolatedGnuPgOpenPgp::open_synthetic_fixture(engine_home, fixture)
        .map_err(|_| SyntheticSyncError::OpenPgpUnavailable)?;

    host.with_sync_vault(peer, |ledger, identity, trust, bound_fingerprint| {
        if bound_fingerprint != fingerprint_bytes.as_slice() {
            return Ok(Err(SyntheticSyncError::BindingConflict));
        }
        let result = (|| {
            let mut vault = SyncedVault::new(ledger, fingerprint, identity, trust, &crypto)
                .map_err(map_service_error)?;
            let had_revisions = vault
                .ledger_revision_count()
                .map_err(map_service_error)?
                != 0;

            let status = match action {
                SyntheticSyncAction::Activate if !had_revisions => {
                    vault
                        .save(
                            SecretId::new(),
                            &[],
                            "Prueba Desktop".to_owned(),
                            false,
                            false,
                            Vec::new(),
                            kryptopass_core::IconState::Automatic,
                            synthetic_login(),
                        )
                        .map_err(map_service_error)?;
                    "Prueba activada y credencial guardada. Pulsa Sincronizar en Android."
                }
                SyntheticSyncAction::Activate => {
                    "Prueba activada. Pulsa Sincronizar en Android."
                }
                SyntheticSyncAction::Refresh => {
                    "Vista local actualizada. Para intercambiar cambios, pulsa Sincronizar en Android."
                }
                SyntheticSyncAction::CreateCredential => {
                    vault
                        .save(
                            SecretId::new(),
                            &[],
                            "Credencial de prueba".to_owned(),
                            false,
                            false,
                            Vec::new(),
                            kryptopass_core::IconState::Automatic,
                            synthetic_login(),
                        )
                        .map_err(map_service_error)?;
                    "Credencial de prueba guardada. Pulsa Sincronizar en Android."
                }
            };
            let rows = project_rows(vault.list().map_err(map_service_error)?);
            let local_revision_count = vault
                .ledger_revision_count()
                .map_err(map_service_error)?;
            Ok(SyntheticSyncSnapshot {
                local_revision_count,
                rows,
                status,
            })
        })();
        Ok(result)
    })
    .map_err(|_| SyntheticSyncError::LedgerUnavailable)?
}

#[cfg(not(debug_assertions))]
pub(crate) fn run(
    _profile_directory: &Path,
    _peer: DeviceId,
    _action: SyntheticSyncAction,
) -> Result<SyntheticSyncSnapshot, SyntheticSyncError> {
    Err(SyntheticSyncError::DebugBuildOnly)
}

#[cfg(debug_assertions)]
fn select_binding(
    host: &PeerHost,
    peer: DeviceId,
    fingerprint: &[u8],
) -> Result<VaultBindingInfo, SyntheticSyncError> {
    if let Some(binding) = host
        .sync_vault_info(peer)
        .map_err(|_| SyntheticSyncError::PeerNotAuthorized)?
    {
        if binding.vault_id != SYNTHETIC_VAULT_ID || binding.fingerprint.as_slice() != fingerprint {
            return Err(SyntheticSyncError::BindingConflict);
        }
    } else {
        host.configure_sync_vault(peer, SYNTHETIC_VAULT_ID, fingerprint)
            .map_err(|_| SyntheticSyncError::PeerNotAuthorized)?;
    }
    host.sync_vault_info(peer)
        .map_err(|_| SyntheticSyncError::ProfileUnavailable)?
        .ok_or(SyntheticSyncError::ProfileUnavailable)
}

#[cfg(debug_assertions)]
fn existing_binding(
    host: &PeerHost,
    peer: DeviceId,
    fingerprint: &[u8],
) -> Result<VaultBindingInfo, SyntheticSyncError> {
    let binding = host
        .sync_vault_info(peer)
        .map_err(|_| SyntheticSyncError::PeerNotAuthorized)?
        .ok_or(SyntheticSyncError::NotConfigured)?;
    if binding.vault_id != SYNTHETIC_VAULT_ID || binding.fingerprint.as_slice() != fingerprint {
        return Err(SyntheticSyncError::BindingConflict);
    }
    Ok(binding)
}

#[cfg(debug_assertions)]
fn synthetic_login() -> SecretContent {
    SecretContent::Login(LoginContent {
        username: SensitiveText::from("sync-test@kryptopass.invalid"),
        password: SensitiveText::from("KryptoPass synthetic test password"),
        notes: SensitiveText::from("Dato sintético. No contiene una contraseña real."),
        urls: Vec::new(),
        totp: None,
        history: Box::default(),
    })
}

#[cfg(debug_assertions)]
fn project_rows(heads: Vec<VaultHeadProjection>) -> Vec<String> {
    let mut branch_counts = HashMap::<SecretId, usize>::new();
    for head in &heads {
        *branch_counts.entry(head.record_id).or_default() += 1;
    }
    let mut rows = heads
        .into_iter()
        .map(|head| {
            let id = head.record_id.0.simple().to_string();
            let short_id = id.get(..8).unwrap_or("????????");
            let conflict = branch_counts.get(&head.record_id).copied().unwrap_or(1) > 1;
            match (head.operation, head.object) {
                (Operation::Write, Some(object)) => {
                    let marker = if conflict {
                        " · conflicto, varias ramas"
                    } else {
                        ""
                    };
                    format!("{} · {}{marker}", object.title, short_id)
                }
                (Operation::Delete, None) => {
                    let marker = if conflict {
                        " · conflicto, varias ramas"
                    } else {
                        ""
                    };
                    format!("Registro eliminado · {short_id}{marker}")
                }
                _ => "Revisión sintética no proyectable".to_owned(),
            }
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

#[cfg(debug_assertions)]
fn map_service_error(error: VaultSyncError) -> SyntheticSyncError {
    match error {
        VaultSyncError::Conflict => SyntheticSyncError::WriteRejected,
        VaultSyncError::LimitExceeded => SyntheticSyncError::LimitExceeded,
        VaultSyncError::Trust => SyntheticSyncError::PeerNotAuthorized,
        VaultSyncError::Crypto => SyntheticSyncError::OpenPgpUnavailable,
        VaultSyncError::InvalidInput | VaultSyncError::InvalidObject => {
            SyntheticSyncError::ContentUnavailable
        }
        VaultSyncError::Ledger => SyntheticSyncError::LedgerUnavailable,
    }
}
