//! Orchestration for local OpenPGP keyring management operations.

use std::{
    collections::HashSet,
    fs,
    io::{self, Cursor},
    sync::{Arc, Mutex},
    time::Duration,
};

use gpgme::{
    Context, CreateKeyFlags, Data, DeleteKeyFlags, ExportMode, Key, KeyListMode, KeySigningFlags,
    PinentryMode, Protocol,
};
use zeroize::Zeroizing;

use super::{
    edit,
    files::{self, BoundedBufferWriter, MAX_KEY_BLOB_BYTES},
    request::{
        self, DeleteMaterial, MAX_CERTIFIED_USER_IDS, OperationOutcome, OperationRequest,
        SubkeyUsage,
    },
};

/// Parses one request and runs it in a fresh GPGME context using the system
/// pinentry agent for every operation that needs a passphrase.
pub fn execute_operation(request_json: &str) -> Result<OperationOutcome, String> {
    match request::parse_request(request_json)? {
        OperationRequest::Create {
            userid,
            expires_days,
        } => create_key(&userid, expires_days),
        OperationRequest::Import { path } => import_file(&path),
        OperationRequest::ExportPublic { fingerprint, path } => export_public(&fingerprint, &path),
        OperationRequest::CopyPublic { fingerprint } => copy_public(&fingerprint),
        OperationRequest::BackupPrivate { fingerprint, path } => {
            backup_private(&fingerprint, &path)
        }
        OperationRequest::Passphrase { fingerprint } => change_passphrase(&fingerprint),
        OperationRequest::Expiry {
            fingerprint,
            subkeys,
            expires_days,
        } => change_expiry(&fingerprint, &subkeys, expires_days),
        OperationRequest::AddIdentity {
            fingerprint,
            userid,
        } => add_identity(&fingerprint, &userid),
        OperationRequest::PrimaryIdentity {
            fingerprint,
            userid,
        } => set_primary_identity(&fingerprint, &userid),
        OperationRequest::RevokeIdentity {
            fingerprint,
            userid,
        } => revoke_identity(&fingerprint, &userid),
        OperationRequest::AddSubkey {
            fingerprint,
            usage,
            expires_days,
        } => add_subkey(&fingerprint, usage, expires_days),
        OperationRequest::RevokeSubkey {
            fingerprint,
            subkey,
        } => revoke_subkey(&fingerprint, &subkey),
        OperationRequest::PrepareRevocation { fingerprint, path } => {
            prepare_revocation(&fingerprint, &path)
        }
        OperationRequest::RevokeKey { fingerprint } => revoke_key(&fingerprint),
        OperationRequest::Certify {
            fingerprint,
            signer_fingerprint,
            userids,
            local,
        } => certify_key(&fingerprint, &signer_fingerprint, &userids, local),
        OperationRequest::RefreshExternal {
            fingerprint,
            keyserver,
        } => refresh_external(&fingerprint, &keyserver),
        OperationRequest::Delete {
            fingerprint,
            material,
        } => delete_key(&fingerprint, material),
    }
}

fn new_context() -> Result<Context, String> {
    let mut context = Context::from_protocol(Protocol::OpenPgp)
        .map_err(|error| format!("No se pudo abrir GnuPG: {error}"))?;
    context
        .set_pinentry_mode(PinentryMode::Ask)
        .map_err(|error| format!("No se pudo activar pinentry: {error}"))?;
    context
        .add_key_list_mode(KeyListMode::WITH_SECRET | KeyListMode::SIGS)
        .map_err(|error| format!("No se pudo preparar la consulta de claves: {error}"))?;
    Ok(context)
}

fn key_by_fingerprint(context: &mut Context, fingerprint: &str) -> Result<Option<Key>, String> {
    let mut keys = context
        .keys()
        .map_err(|error| format!("No se pudo consultar el llavero OpenPGP: {error}"))?;
    let mut found = None;
    for entry in keys.by_ref() {
        let key = entry.map_err(|error| format!("No se pudo leer una clave OpenPGP: {error}"))?;
        let candidate = key
            .fingerprint_raw()
            .map(|value| value.to_string_lossy().to_ascii_uppercase());
        if candidate.as_deref() == Some(fingerprint) {
            found = Some(key);
            break;
        }
    }
    keys.finish()
        .map_err(|error| format!("No se pudo cerrar la consulta de claves: {error}"))?;
    Ok(found)
}

fn required_key(context: &mut Context, fingerprint: &str) -> Result<Key, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    key_by_fingerprint(context, &fingerprint)?
        .ok_or_else(|| format!("No existe una clave local con la huella primaria {fingerprint}."))
}

fn primary_fingerprint(key: &Key) -> Result<String, String> {
    key.fingerprint_raw()
        .map(|value| value.to_string_lossy().to_ascii_uppercase())
        .ok_or_else(|| "La clave no tiene una huella primaria.".into())
}

fn require_own_primary_key(key: &Key) -> Result<(), String> {
    if key.is_revoked() || key.is_disabled() || key.is_invalid() {
        return Err("La clave propia está revocada, deshabilitada o no es válida.".into());
    }
    let primary = key
        .primary_key()
        .ok_or_else(|| "La clave no contiene una clave primaria.".to_owned())?;
    if !primary.is_secret() || !primary.can_certify() {
        return Err(
            "La operación requiere la clave privada primaria propia con capacidad de certificación."
                .into(),
        );
    }
    Ok(())
}

fn require_secret_material(key: &Key) -> Result<(), String> {
    if !key.has_secret() {
        return Err("La clave no tiene material privado local.".into());
    }
    Ok(())
}

fn operation_outcome(
    message: impl Into<String>,
    fingerprint: Option<String>,
    public_key: Option<String>,
) -> OperationOutcome {
    OperationOutcome {
        message: message.into(),
        public_key,
        selected_fingerprint: fingerprint,
    }
}

fn create_key(userid: &str, expires_days: u32) -> Result<OperationOutcome, String> {
    let userid = request::validated_user_id(userid)?;
    let expires = request::expiry_duration(expires_days)?;
    let mut context = new_context()?;
    let mut primary_flags = CreateKeyFlags::SIGN | CreateKeyFlags::CERT;
    if expires_days == 0 {
        primary_flags |= CreateKeyFlags::NOEXPIRE;
    }

    let generated = context
        .create_key_with_flags(userid, "default", expires, primary_flags)
        .map_err(|error| format!("No se pudo crear la clave primaria: {error}"))?;
    let fingerprint = generated
        .fingerprint()
        .map_err(|_| "GnuPG creó la clave pero no devolvió su huella.".to_owned())?
        .to_ascii_uppercase();
    drop(generated);

    let key = key_by_fingerprint(&mut context, &fingerprint)?.ok_or_else(|| {
        format!("La clave primaria {fingerprint} se creó, pero no aparece en el llavero.")
    })?;
    if let Err(error) = require_own_primary_key(&key) {
        return Err(format!(
            "La clave primaria {fingerprint} se creó, pero no se pudo validar: {error}"
        ));
    }
    let has_encryption_subkey = key
        .subkeys()
        .skip(1)
        .any(|subkey| subkey.can_encrypt() && !subkey.is_revoked());
    if !has_encryption_subkey {
        let mut subkey_flags = CreateKeyFlags::ENCR;
        if expires_days == 0 {
            subkey_flags |= CreateKeyFlags::NOEXPIRE;
        }
        context
            .create_subkey_with_flags(&key, "default", expires, subkey_flags)
            .map_err(|error| {
                format!(
                    "La clave primaria {fingerprint} ya se creó, pero falló la subclave de cifrado: {error}"
                )
            })?;
    }

    Ok(operation_outcome(
        format!("Clave OpenPGP creada con subclave de cifrado: {fingerprint}."),
        Some(fingerprint),
        None,
    ))
}

fn import_file(path: &str) -> Result<OperationOutcome, String> {
    let path = request::local_path(path)?;
    let bytes = files::read_bounded_file(
        &path,
        files::MAX_INPUT_FILE_BYTES,
        "El archivo de importación",
    )?;
    let mut context = new_context()?;
    let source = Cursor::new(bytes.as_slice());
    let data = Data::from_reader(source).map_err(|error| {
        format!(
            "No se pudo leer el archivo de importación: {}",
            error.error()
        )
    })?;
    let result = context
        .import(data)
        .map_err(|error| format!("No se pudo importar el material OpenPGP: {error}"))?;
    let considered = result.considered();
    let imported = result.imported() + result.secret_imported();
    let unchanged = result.unchanged() + result.secret_unchanged();
    let new_user_ids = result.new_user_ids();
    let new_subkeys = result.new_subkeys();
    let new_signatures = result.new_signatures();
    let new_revocations = result.new_revocations();
    let not_imported = result.not_imported();
    let without_user_id = result.without_user_id();
    let skipped_v3 = result.skipped_v3_keys();
    if considered == 0 && new_revocations == 0 {
        return Err(
            "El archivo no contiene claves ni revocaciones OpenPGP que GnuPG pueda importar."
                .into(),
        );
    }

    let mut item_errors = 0u32;
    let mut first_successful_fingerprint = None;
    for item in result.imports() {
        let fingerprint = item.fingerprint().ok().map(str::to_owned);
        if item.result().is_err() {
            item_errors += 1;
        } else if first_successful_fingerprint.is_none() {
            first_successful_fingerprint = fingerprint;
        }
    }
    drop(result);

    let selected_fingerprint = match first_successful_fingerprint {
        Some(fingerprint) => {
            let fingerprint = request::normalize_fingerprint(&fingerprint).ok();
            match fingerprint {
                Some(fingerprint) if key_by_fingerprint(&mut context, &fingerprint)?.is_some() => {
                    Some(fingerprint)
                }
                _ => None,
            }
        }
        None => None,
    };
    let successful_work = imported > 0
        || unchanged > 0
        || new_user_ids > 0
        || new_subkeys > 0
        || new_signatures > 0
        || new_revocations > 0
        || selected_fingerprint.is_some();
    if !successful_work {
        return Err(format!(
            "La importación no aplicó ningún cambio: {not_imported} elemento(s) no importado(s) y {item_errors} registro(s) con error."
        ));
    }

    let partial = not_imported > 0 || item_errors > 0 || without_user_id > 0 || skipped_v3 > 0;
    let mut message = if partial {
        format!(
            "Importación OpenPGP parcial: GnuPG incorporó {imported} elemento(s) y dejó {not_imported} sin importar."
        )
    } else {
        format!("Importación OpenPGP completada; GnuPG incorporó {imported} elemento(s).")
    };
    if unchanged > 0 {
        message.push_str(&format!(
            " {unchanged} elemento(s) ya estaban actualizados."
        ));
    }
    if item_errors > 0 {
        message.push_str(&format!(
            " Se detectaron {item_errors} registro(s) con errores."
        ));
    }
    if without_user_id > 0 {
        message.push_str(&format!(
            " {without_user_id} clave(s) no tenían identidad de usuario."
        ));
    }
    if skipped_v3 > 0 {
        message.push_str(&format!(
            " Se omitieron {skipped_v3} clave(s) de formato v3."
        ));
    }
    if new_user_ids > 0 {
        message.push_str(&format!(" Se añadieron {new_user_ids} identidad(es)."));
    }
    if new_subkeys > 0 {
        message.push_str(&format!(" Se añadieron {new_subkeys} subclave(s)."));
    }
    if new_signatures > 0 {
        message.push_str(&format!(" Se añadieron {new_signatures} firma(s)."));
    }
    if new_revocations > 0 {
        message.push_str(&format!(" Se aplicaron {new_revocations} revocación(es)."));
    }
    Ok(operation_outcome(message, selected_fingerprint, None))
}

fn export_bytes(
    context: &mut Context,
    fingerprint: &str,
    mode: ExportMode,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let storage = Arc::new(Mutex::new(Zeroizing::new(Vec::new())));
    let writer = BoundedBufferWriter::new(Arc::clone(&storage), MAX_KEY_BLOB_BYTES);
    let mut destination = Data::from_writer(writer)
        .map_err(|error| format!("No se pudo preparar la exportación: {}", error.error()))?;
    context
        .export([fingerprint], mode, &mut destination)
        .map_err(|error| format!("GnuPG no pudo exportar la clave: {error}"))?;
    drop(destination);

    let mutex = Arc::try_unwrap(storage)
        .map_err(|_| "No se pudo recuperar el resultado acotado de la exportación.".to_owned())?;
    let bytes = mutex
        .into_inner()
        .map_err(|_| "No se pudo recuperar el resultado de la exportación.".to_owned())?;
    if bytes.is_empty() {
        return Err("GnuPG no produjo datos para la exportación solicitada.".into());
    }
    Ok(bytes)
}

fn export_public(fingerprint: &str, path: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let path = request::local_path(path)?;
    let mut context = new_context()?;
    let key = key_by_fingerprint(&mut context, &fingerprint)?
        .ok_or_else(|| format!("No existe la clave {fingerprint} en el llavero local."))?;
    let actual_fingerprint = primary_fingerprint(&key)?;
    context.set_armor(true);
    let bytes = export_bytes(&mut context, &actual_fingerprint, ExportMode::empty())?;
    let publication = files::write_atomic_new(&path, &bytes, &context)?;
    Ok(operation_outcome(
        format!(
            "Clave pública exportada a {}.{}",
            path.display(),
            publication_warning(&publication)
        ),
        Some(actual_fingerprint),
        None,
    ))
}

fn copy_public(fingerprint: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let mut context = new_context()?;
    let key = key_by_fingerprint(&mut context, &fingerprint)?
        .ok_or_else(|| format!("No existe la clave {fingerprint} en el llavero local."))?;
    let actual_fingerprint = primary_fingerprint(&key)?;
    context.set_armor(true);
    let bytes = export_bytes(&mut context, &actual_fingerprint, ExportMode::empty())?;
    let public_key = String::from_utf8(bytes.to_vec())
        .map_err(|_| "GnuPG devolvió una clave pública que no es ASCII/UTF-8.".to_owned())?;
    Ok(operation_outcome(
        format!("Clave pública lista para copiar: {actual_fingerprint}."),
        Some(actual_fingerprint),
        Some(public_key),
    ))
}

fn backup_private(fingerprint: &str, path: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let path = request::local_path(path)?;
    let mut context = new_context()?;
    let key = key_by_fingerprint(&mut context, &fingerprint)?
        .ok_or_else(|| format!("No existe la clave {fingerprint} en el llavero local."))?;
    require_secret_material(&key)?;
    let actual_fingerprint = primary_fingerprint(&key)?;
    context.set_armor(true);
    let bytes = export_bytes(&mut context, &actual_fingerprint, ExportMode::SECRET)?;
    let publication = files::write_atomic_new(&path, &bytes, &context)?;
    Ok(operation_outcome(
        format!(
            "Copia privada guardada con permisos 0600 en {}.{}",
            path.display(),
            publication_warning(&publication)
        ),
        Some(actual_fingerprint),
        None,
    ))
}

fn change_passphrase(fingerprint: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    require_secret_material(&key)?;
    context
        .change_key_passphrase(&key)
        .map_err(|error| format!("No se pudo cambiar la frase de paso: {error}"))?;
    Ok(operation_outcome(
        format!("Frase de paso actualizada para {fingerprint}."),
        Some(fingerprint),
        None,
    ))
}

fn change_expiry(
    fingerprint: &str,
    subkeys: &[String],
    expires_days: u32,
) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let expires = request::expiry_duration(expires_days)?;
    if subkeys.len() > 64 {
        return Err("La solicitud de caducidad puede incluir como máximo 64 subclaves.".into());
    }
    let mut normalized_subkeys = Vec::with_capacity(subkeys.len());
    let mut unique = HashSet::with_capacity(subkeys.len());
    for subkey in subkeys {
        let subkey = request::normalize_fingerprint(subkey)?;
        if !unique.insert(subkey.clone()) {
            return Err("La lista de huellas de subclave contiene duplicados.".into());
        }
        normalized_subkeys.push(subkey);
    }

    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    require_own_primary_key(&key)?;
    let available = subkey_fingerprints(&key);
    for subkey in &normalized_subkeys {
        if !available.contains(subkey) {
            return Err(format!(
                "La subclave {subkey} no pertenece a la clave primaria {fingerprint}."
            ));
        }
    }
    context
        .set_expire(&key, expires, normalized_subkeys.iter().map(String::as_str))
        .map_err(|error| format!("No se pudo actualizar la caducidad: {error}"))?;
    let scope = if normalized_subkeys.is_empty() {
        "la clave primaria"
    } else {
        "las subclaves seleccionadas"
    };
    Ok(operation_outcome(
        format!("Caducidad de {scope} actualizada para {fingerprint}."),
        Some(fingerprint),
        None,
    ))
}

fn add_identity(fingerprint: &str, userid: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let userid = request::validated_user_id(userid)?;
    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    require_own_primary_key(&key)?;
    if find_user_id(&key, userid).is_some() {
        return Err("La identidad ya está presente en esta clave.".into());
    }
    context
        .add_uid(&key, userid)
        .map_err(|error| format!("No se pudo añadir la identidad: {error}"))?;
    Ok(operation_outcome(
        format!("Identidad añadida a {fingerprint}."),
        Some(fingerprint),
        None,
    ))
}

fn set_primary_identity(fingerprint: &str, userid: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let userid = request::validated_user_id(userid)?;
    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    require_own_primary_key(&key)?;
    match find_user_id(&key, userid) {
        Some(user_id) if !user_id.is_revoked() => {}
        Some(_) => return Err("No se puede elegir como principal una identidad revocada.".into()),
        None => return Err("La identidad indicada no pertenece a esta clave.".into()),
    }
    context
        .set_primary_uid(&key, userid)
        .map_err(|error| format!("No se pudo cambiar la identidad principal: {error}"))?;
    Ok(operation_outcome(
        format!("Identidad principal actualizada para {fingerprint}."),
        Some(fingerprint),
        None,
    ))
}

fn revoke_identity(fingerprint: &str, userid: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let userid = request::validated_user_id(userid)?;
    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    require_own_primary_key(&key)?;
    match find_user_id(&key, userid) {
        Some(user_id) if !user_id.is_revoked() => {}
        Some(_) => return Err("La identidad ya está revocada.".into()),
        None => return Err("La identidad indicada no pertenece a esta clave.".into()),
    }
    context
        .revoke_uid(&key, userid)
        .map_err(|error| format!("No se pudo revocar la identidad: {error}"))?;
    Ok(operation_outcome(
        format!("Identidad revocada en {fingerprint}."),
        Some(fingerprint),
        None,
    ))
}

fn add_subkey(
    fingerprint: &str,
    usage: SubkeyUsage,
    expires_days: u32,
) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let expires = request::expiry_duration(expires_days)?;
    let mut flags = match usage {
        SubkeyUsage::Encrypt => CreateKeyFlags::ENCR,
        SubkeyUsage::Sign => CreateKeyFlags::SIGN,
        SubkeyUsage::Authenticate => CreateKeyFlags::AUTH,
    };
    if expires_days == 0 {
        flags |= CreateKeyFlags::NOEXPIRE;
    }
    let usage_name = match usage {
        SubkeyUsage::Encrypt => "cifrado",
        SubkeyUsage::Sign => "firma",
        SubkeyUsage::Authenticate => "autenticación",
    };
    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    require_own_primary_key(&key)?;
    context
        .create_subkey_with_flags(&key, "default", expires, flags)
        .map_err(|error| format!("No se pudo crear la subclave de {usage_name}: {error}"))?;
    Ok(operation_outcome(
        format!("Subclave de {usage_name} creada para {fingerprint}."),
        Some(fingerprint),
        None,
    ))
}

fn revoke_subkey(fingerprint: &str, subkey_fingerprint: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let subkey_fingerprint = request::normalize_fingerprint(subkey_fingerprint)?;
    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    require_own_primary_key(&key)?;

    let primary_fingerprint = primary_fingerprint(&key)?;
    let mut subkey_index = 0usize;
    let mut selected_index = None;
    for subkey in key.subkeys().skip(1) {
        subkey_index += 1;
        let actual = subkey
            .fingerprint_raw()
            .map(|value| value.to_string_lossy().to_ascii_uppercase());
        if actual.as_deref() == Some(&subkey_fingerprint) {
            if subkey.is_revoked() {
                return Err("La subclave ya está revocada.".into());
            }
            selected_index = Some(subkey_index);
            break;
        }
    }
    let selected_index = selected_index.ok_or_else(|| {
        format!("La subclave {subkey_fingerprint} no pertenece a {primary_fingerprint}.")
    })?;

    edit::interact_revoking_subkey(&mut context, &key, selected_index)?;

    let mut verification_context = new_context()?;
    let updated_key = required_key(&mut verification_context, &primary_fingerprint)?;
    let revoked = updated_key.subkeys().any(|subkey| {
        subkey
            .fingerprint_raw()
            .map(|value| {
                value
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&subkey_fingerprint)
            })
            .unwrap_or(false)
            && subkey.is_revoked()
    });
    if !revoked {
        return Err(format!(
            "GnuPG no confirmó la revocación de la subclave {subkey_fingerprint}."
        ));
    }
    Ok(operation_outcome(
        format!("Subclave {subkey_fingerprint} revocada."),
        Some(primary_fingerprint),
        None,
    ))
}

fn prepare_revocation(fingerprint: &str, path: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let path = request::local_path(path)?;
    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    require_own_primary_key(&key)?;
    let certificate = revocation_certificate(&context, &fingerprint)?;
    let publication = files::write_atomic_new(&path, &certificate, &context)?;
    Ok(operation_outcome(
        format!(
            "Certificado de revocación preparado en {}.{}",
            path.display(),
            publication_warning(&publication)
        ),
        Some(fingerprint),
        None,
    ))
}

fn revoke_key(fingerprint: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    require_own_primary_key(&key)?;
    if key.is_revoked() {
        return Err("La clave ya está revocada.".into());
    }
    let certificate = revocation_certificate(&context, &fingerprint)?;
    let data = Data::from_reader(Cursor::new(certificate.as_slice())).map_err(|error| {
        format!(
            "No se pudo preparar el certificado de revocación: {}",
            error.error()
        )
    })?;
    context
        .import(data)
        .map_err(|error| format!("No se pudo aplicar el certificado de revocación: {error}"))?;

    let mut verification_context = new_context()?;
    let updated = required_key(&mut verification_context, &fingerprint)?;
    if !updated.is_revoked() {
        return Err(format!(
            "GnuPG importó el certificado, pero no confirmó la revocación de {fingerprint}."
        ));
    }
    Ok(operation_outcome(
        format!("Clave {fingerprint} revocada localmente."),
        Some(fingerprint),
        None,
    ))
}

fn certify_key(
    fingerprint: &str,
    signer_fingerprint: &str,
    userids: &[String],
    local: bool,
) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let signer_fingerprint = request::normalize_fingerprint(signer_fingerprint)?;
    if fingerprint == signer_fingerprint {
        return Err("La certificación debe dirigirse a una clave distinta de la firmante.".into());
    }
    if userids.is_empty() || userids.len() > MAX_CERTIFIED_USER_IDS {
        return Err("Selecciona entre 1 y 64 identidades para certificar.".into());
    }
    let mut normalized_userids = Vec::with_capacity(userids.len());
    let mut unique = HashSet::with_capacity(userids.len());
    for userid in userids {
        let userid = request::validated_user_id(userid)?.to_owned();
        if !unique.insert(userid.clone()) {
            return Err("La lista de identidades contiene duplicados.".into());
        }
        normalized_userids.push(userid);
    }

    let mut context = new_context()?;
    let target = required_key(&mut context, &fingerprint)?;
    if target.is_revoked() || target.is_invalid() {
        return Err("No se puede certificar una clave revocada o no válida.".into());
    }
    let signer = required_key(&mut context, &signer_fingerprint)?;
    require_own_primary_key(&signer)?;
    if signer.is_expired() {
        return Err("La clave firmante está caducada.".into());
    }
    for userid in &normalized_userids {
        match find_user_id(&target, userid) {
            Some(user_id) if user_id.is_revoked() => {
                return Err("No se puede certificar una identidad revocada.".into());
            }
            Some(user_id) if user_id.is_invalid() => {
                return Err("No se puede certificar una identidad no válida.".into());
            }
            Some(_) => {}
            None => {
                return Err(format!(
                    "La identidad `{userid}` no pertenece a la clave {fingerprint}."
                ));
            }
        }
    }
    context
        .add_signer(&signer)
        .map_err(|error| format!("No se pudo seleccionar la clave firmante: {error}"))?;
    let mut flags = KeySigningFlags::empty();
    if local {
        flags |= KeySigningFlags::LOCAL;
    }
    context
        .sign_key_with_flags(
            &target,
            normalized_userids.iter().map(String::as_str),
            Duration::ZERO,
            flags,
        )
        .map_err(|error| format!("No se pudo certificar las identidades: {error}"))?;
    Ok(operation_outcome(
        format!(
            "{}identidad(es) certificada(s) en {fingerprint}.",
            normalized_userids.len()
        ),
        Some(fingerprint),
        None,
    ))
}

fn refresh_external(fingerprint: &str, keyserver: &str) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let mut context = new_context()?;
    let already_present = key_by_fingerprint(&mut context, &fingerprint)?.is_some();
    let mut arguments = if already_present {
        vec![
            "--no-tty",
            "--batch",
            "--refresh-keys",
            fingerprint.as_str(),
        ]
    } else {
        vec!["--no-tty", "--batch", "--recv-keys", fingerprint.as_str()]
    };
    if !keyserver.is_empty() {
        crate::keyring::online::validate_keyserver(keyserver)?;
        arguments.splice(0..0, ["--keyserver", keyserver]);
    }
    edit::run_gpg_quiet(&arguments).map_err(|error| {
        format!("No se pudo actualizar la clave desde la fuente configurada: {error}")
    })?;

    let mut verification_context = new_context()?;
    let updated =
        key_by_fingerprint(&mut verification_context, &fingerprint)?.ok_or_else(|| {
            format!("La fuente externa no devolvió la clave {fingerprint} solicitada.")
        })?;
    let actual = primary_fingerprint(&updated)?;
    if actual != fingerprint {
        return Err(format!(
            "La fuente externa devolvió una huella distinta de la solicitada: {actual}."
        ));
    }
    let verb = if already_present {
        "actualizada"
    } else {
        "importada"
    };
    Ok(operation_outcome(
        format!("Clave {fingerprint} {verb} desde la fuente externa configurada."),
        Some(fingerprint),
        None,
    ))
}

fn delete_key(fingerprint: &str, material: DeleteMaterial) -> Result<OperationOutcome, String> {
    let fingerprint = request::normalize_fingerprint(fingerprint)?;
    let mut context = new_context()?;
    let key = required_key(&mut context, &fingerprint)?;
    let has_secret = key.has_secret();
    match material {
        DeleteMaterial::Public => {
            if has_secret {
                return Err(
                    "GnuPG no permite borrar solo la parte pública mientras existe material secreto; elimina primero ese material o borra la clave completa."
                        .into(),
                );
            }
            context
                .delete_key_with_flags(&key, DeleteKeyFlags::FORCE)
                .map_err(|error| format!("No se pudo eliminar la clave pública: {error}"))?;
            verify_key_absent(&fingerprint)?;
            Ok(operation_outcome(
                format!("Parte pública eliminada para {fingerprint}."),
                Some(fingerprint),
                None,
            ))
        }
        DeleteMaterial::Private => {
            if !has_secret {
                return Err("La clave no tiene material privado que eliminar.".into());
            }
            edit::run_gpg_quiet(&[
                "--no-tty",
                "--batch",
                "--yes",
                "--delete-secret-keys",
                fingerprint.as_str(),
            ])
            .map_err(|error| format!("No se pudo eliminar solo el material privado: {error}"))?;
            let mut verification_context = new_context()?;
            match key_by_fingerprint(&mut verification_context, &fingerprint)? {
                Some(remaining) if !remaining.has_secret() => {}
                Some(_) => {
                    return Err(format!(
                        "GnuPG terminó, pero el material privado de {fingerprint} sigue presente."
                    ));
                }
                None => {
                    return Err(format!(
                        "GnuPG eliminó también la parte pública de {fingerprint}; la operación quedó parcialmente aplicada."
                    ));
                }
            }
            Ok(operation_outcome(
                format!(
                    "Material privado eliminado; la parte pública se conserva para {fingerprint}."
                ),
                Some(fingerprint),
                None,
            ))
        }
        DeleteMaterial::Both => {
            let flags = if has_secret {
                DeleteKeyFlags::ALLOW_SECRET | DeleteKeyFlags::FORCE
            } else {
                DeleteKeyFlags::FORCE
            };
            context
                .delete_key_with_flags(&key, flags)
                .map_err(|error| format!("No se pudo eliminar la clave completa: {error}"))?;
            verify_key_absent(&fingerprint)?;
            Ok(operation_outcome(
                format!("Clave pública y material privado eliminados para {fingerprint}."),
                Some(fingerprint),
                None,
            ))
        }
    }
}

fn find_user_id<'key>(key: &'key Key, needle: &str) -> Option<gpgme::UserId<'key>> {
    key.user_ids().find(|user_id| {
        user_id
            .id_raw()
            .map(|value| value.to_string_lossy() == needle)
            .unwrap_or(false)
    })
}

fn verify_key_absent(fingerprint: &str) -> Result<(), String> {
    let mut context = new_context()?;
    if key_by_fingerprint(&mut context, fingerprint)?.is_some() {
        return Err(format!(
            "GnuPG terminó, pero la clave {fingerprint} aún aparece en el llavero."
        ));
    }
    Ok(())
}

fn subkey_fingerprints(key: &Key) -> HashSet<String> {
    key.subkeys()
        .skip(1)
        .filter_map(|subkey| {
            subkey
                .fingerprint_raw()
                .map(|value| value.to_string_lossy().to_ascii_uppercase())
        })
        .collect()
}

fn revocation_certificate(
    context: &Context,
    fingerprint: &str,
) -> Result<Zeroizing<Vec<u8>>, String> {
    if let Some(path) = edit::revocation_file_path(context, fingerprint) {
        let directory = path
            .parent()
            .ok_or_else(|| "La ruta del certificado de revocación no es válida.".to_owned())?;
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(
                    "La carpeta de certificados de revocación no es una carpeta local segura."
                        .into(),
                );
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "No se pudo consultar la carpeta de revocaciones: {error}"
                ));
            }
        }
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => {
                let raw = files::read_bounded_file(
                    &path,
                    MAX_KEY_BLOB_BYTES as u64,
                    "El certificado de revocación",
                )?;
                return edit::normalize_revocation_certificate(raw);
            }
            Ok(_) => {
                return Err(
                    "El certificado de revocación debe ser un archivo local regular.".into(),
                );
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "No se pudo consultar el certificado de revocación: {error}"
                ));
            }
        }
    }
    edit::generate_revocation_certificate(fingerprint)
}

fn publication_warning(publication: &files::PublishOutcome) -> &'static str {
    if publication.durability_warning {
        " El archivo ya fue publicado, pero no se pudo sincronizar su carpeta; podría no persistir ante un corte de energía."
    } else {
        ""
    }
}
