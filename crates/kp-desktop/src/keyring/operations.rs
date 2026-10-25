//! Runs one validated request against the keyring and words its outcome.

use kryptopass_keyring::{
    DeleteScope, ExpiryTarget, Fetched, Fingerprint, ImportReport, Keyring, Published, SubkeyUsage,
};

use super::{
    errors,
    request::{self, InvalidRequest, OperationOutcome, Request},
};

/// Parses one request and runs it on the user's keyring; every passphrase
/// goes through the system pinentry agent.
pub fn execute_operation(request_json: &str) -> Result<OperationOutcome, String> {
    let request = request::parse_request(request_json).map_err(|error| match error {
        InvalidRequest::TooLarge => {
            crate::i18n::text("La solicitud OpenPGP supera el tamaño permitido.")
        }
        InvalidRequest::Malformed(detail) => {
            crate::i18n::format("La solicitud OpenPGP no es válida: %1", &[detail])
        }
        InvalidRequest::Value(input) => errors::invalid_input(input),
    })?;
    run(&Keyring::system(), request).map_err(|error| errors::describe(&error))
}

fn run(keyring: &Keyring, request: Request) -> kryptopass_keyring::Result<OperationOutcome> {
    Ok(match request {
        Request::Create { user_id, expiry } => {
            let fingerprint = keyring.create_key(&user_id, expiry)?;
            selected(
                crate::i18n::format(
                    "Clave OpenPGP creada con subclave de cifrado: %1.",
                    &[fingerprint.to_string()],
                ),
                &fingerprint,
            )
        }
        Request::Import { path } => {
            let report = keyring.import_file(&path)?;
            OperationOutcome {
                message: import_message(&report),
                public_key: None,
                selected_fingerprint: report.first_key.as_ref().map(ToString::to_string),
            }
        }
        Request::ExportPublic { fingerprint, path } => {
            let published = keyring.export_public_to(&fingerprint, &path)?;
            selected(
                crate::i18n::format(
                    "Clave pública exportada a %1.%2",
                    &[
                        published.path.display().to_string(),
                        durability_warning(&published),
                    ],
                ),
                &fingerprint,
            )
        }
        Request::CopyPublic { fingerprint } => {
            let armored = keyring.public_key_armored(&fingerprint)?;
            OperationOutcome {
                public_key: Some(armored),
                ..selected(
                    crate::i18n::format(
                        "Clave pública lista para copiar: %1.",
                        &[fingerprint.to_string()],
                    ),
                    &fingerprint,
                )
            }
        }
        Request::BackupSecret { fingerprint, path } => {
            let published = keyring.backup_secret_to(&fingerprint, &path)?;
            selected(
                crate::i18n::format(
                    "Copia privada guardada con permisos 0600 en %1.%2",
                    &[
                        published.path.display().to_string(),
                        durability_warning(&published),
                    ],
                ),
                &fingerprint,
            )
        }
        Request::ChangePassphrase { fingerprint } => {
            keyring.change_passphrase(&fingerprint)?;
            selected(
                crate::i18n::format(
                    "Frase de paso actualizada para %1.",
                    &[fingerprint.to_string()],
                ),
                &fingerprint,
            )
        }
        Request::SetExpiry {
            fingerprint,
            target,
            expiry,
        } => {
            keyring.set_expiry(&fingerprint, &target, expiry)?;
            let source = match target {
                ExpiryTarget::Primary => "Caducidad de la clave primaria actualizada para %1.",
                ExpiryTarget::Subkeys(_) => {
                    "Caducidad de las subclaves seleccionadas actualizada para %1."
                }
            };
            selected(
                crate::i18n::format(source, &[fingerprint.to_string()]),
                &fingerprint,
            )
        }
        Request::AddUserId {
            fingerprint,
            user_id,
        } => {
            keyring.add_user_id(&fingerprint, &user_id)?;
            selected(
                crate::i18n::format("Identidad añadida a %1.", &[fingerprint.to_string()]),
                &fingerprint,
            )
        }
        Request::SetPrimaryUserId {
            fingerprint,
            user_id,
        } => {
            keyring.set_primary_user_id(&fingerprint, &user_id)?;
            selected(
                crate::i18n::format(
                    "Identidad principal actualizada para %1.",
                    &[fingerprint.to_string()],
                ),
                &fingerprint,
            )
        }
        Request::RevokeUserId {
            fingerprint,
            user_id,
        } => {
            keyring.revoke_user_id(&fingerprint, &user_id)?;
            selected(
                crate::i18n::format("Identidad revocada en %1.", &[fingerprint.to_string()]),
                &fingerprint,
            )
        }
        Request::AddSubkey {
            fingerprint,
            usage,
            expiry,
        } => {
            keyring.add_subkey(&fingerprint, usage, expiry)?;
            selected(
                crate::i18n::format(
                    "Subclave de %1 creada para %2.",
                    &[usage_name(usage), fingerprint.to_string()],
                ),
                &fingerprint,
            )
        }
        Request::RevokeSubkey {
            fingerprint,
            subkey,
        } => {
            keyring.revoke_subkey(&fingerprint, &subkey)?;
            selected(
                crate::i18n::format("Subclave %1 revocada.", &[subkey.to_string()]),
                &fingerprint,
            )
        }
        Request::WriteRevocation { fingerprint, path } => {
            let published = keyring.write_revocation_certificate(&fingerprint, &path)?;
            selected(
                crate::i18n::format(
                    "Certificado de revocación preparado en %1.%2",
                    &[
                        published.path.display().to_string(),
                        durability_warning(&published),
                    ],
                ),
                &fingerprint,
            )
        }
        Request::RevokeKey { fingerprint } => {
            keyring.revoke_key(&fingerprint)?;
            selected(
                crate::i18n::format("Clave %1 revocada localmente.", &[fingerprint.to_string()]),
                &fingerprint,
            )
        }
        Request::Certify {
            target,
            signer,
            user_ids,
            kind,
        } => {
            let certified = keyring.certify(&target, &signer, &user_ids, kind)?;
            selected(
                crate::i18n::format(
                    "%1 identidad(es) certificada(s) en %2.",
                    &[certified.to_string(), target.to_string()],
                ),
                &target,
            )
        }
        Request::Fetch {
            fingerprint,
            keyserver,
        } => {
            let source = match keyring.fetch(&fingerprint, keyserver.as_ref())? {
                Fetched::Imported => "Clave %1 importada desde la fuente externa configurada.",
                Fetched::Refreshed => "Clave %1 actualizada desde la fuente externa configurada.",
            };
            selected(
                crate::i18n::format(source, &[fingerprint.to_string()]),
                &fingerprint,
            )
        }
        Request::MarkAsOwn { fingerprint } => {
            tracing::info!(
                key = %short(&fingerprint),
                "Keyring: user asked to mark a key as own (ultimate ownertrust)"
            );
            match keyring.mark_as_own(&fingerprint) {
                Ok(()) => {
                    tracing::info!(key = %short(&fingerprint), "Keyring: key marked as own");
                }
                Err(error) => {
                    tracing::warn!(
                        key = %short(&fingerprint),
                        error = %error,
                        "Keyring: could not mark the key as own"
                    );
                    return Err(error);
                }
            }
            selected(
                crate::i18n::format(
                    "Clave %1 marcada como tuya: GnuPG ya puede cifrar con ella.",
                    &[fingerprint.to_string()],
                ),
                &fingerprint,
            )
        }
        Request::Delete { fingerprint, scope } => {
            keyring.delete(&fingerprint, scope)?;
            let source = match scope {
                DeleteScope::PublicOnly => "Parte pública eliminada para %1.",
                DeleteScope::SecretOnly => {
                    "Material privado eliminado; la parte pública se conserva para %1."
                }
                DeleteScope::Everything => "Clave pública y material privado eliminados para %1.",
            };
            selected(
                crate::i18n::format(source, &[fingerprint.to_string()]),
                &fingerprint,
            )
        }
    })
}

/// Last 16 hex digits of a fingerprint: enough to tell keys apart in a log.
fn short(fingerprint: &Fingerprint) -> String {
    let text = fingerprint.as_str();
    text[text.len().saturating_sub(16)..].to_owned()
}

fn selected(message: String, fingerprint: &Fingerprint) -> OperationOutcome {
    OperationOutcome {
        message,
        public_key: None,
        selected_fingerprint: Some(fingerprint.to_string()),
    }
}

fn usage_name(usage: SubkeyUsage) -> String {
    crate::i18n::text(match usage {
        SubkeyUsage::Encrypt => "cifrado",
        SubkeyUsage::Sign => "firma",
        SubkeyUsage::Authenticate => "autenticación",
    })
}

fn durability_warning(published: &Published) -> String {
    if published.durable {
        String::new()
    } else {
        crate::i18n::text(
            " El archivo ya fue publicado, pero no se pudo sincronizar su carpeta; podría no persistir ante un corte de energía.",
        )
    }
}

fn import_message(report: &ImportReport) -> String {
    let mut message = if report.is_partial() {
        crate::i18n::format(
            "Importación OpenPGP parcial: GnuPG incorporó %1 elemento(s) y dejó %2 sin importar.",
            &[report.imported.to_string(), report.not_imported.to_string()],
        )
    } else {
        crate::i18n::format(
            "Importación OpenPGP completada; GnuPG incorporó %1 elemento(s).",
            &[report.imported.to_string()],
        )
    };
    for (count, source) in [
        (report.unchanged, " %1 elemento(s) ya estaban actualizados."),
        (report.failed, " Se detectaron %1 registro(s) con errores."),
        (
            report.without_user_id,
            " %1 clave(s) no tenían identidad de usuario.",
        ),
        (
            report.skipped_v3,
            " Se omitieron %1 clave(s) de formato v3.",
        ),
        (report.new_user_ids, " Se añadieron %1 identidad(es)."),
        (report.new_subkeys, " Se añadieron %1 subclave(s)."),
        (report.new_signatures, " Se añadieron %1 firma(s)."),
        (report.new_revocations, " Se aplicaron %1 revocación(es)."),
    ] {
        if count > 0 {
            message.push_str(&crate::i18n::format(source, &[count.to_string()]));
        }
    }
    message
}
