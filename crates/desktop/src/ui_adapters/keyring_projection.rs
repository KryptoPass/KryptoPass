use cxx_qt_lib::{QString, QStringList};
use kryptopass_core::keyring::{CertificationInfo, SubkeyInfo};
use kryptopass_core::{KeyInfo, KeyValidity, UserIdInfo};
use serde_json::{Value, json};

pub(super) fn key_summaries(keys: Vec<KeyInfo>) -> (QStringList, i32) {
    let key_count = i32::try_from(keys.len()).unwrap_or(i32::MAX);
    let summaries = keys
        .into_iter()
        .map(|key| {
            let identity = key
                .user_ids
                .first()
                .map(|user_id| user_id.id.as_str())
                .unwrap_or("Sin identidad");

            QString::from(format!("{identity}\n{}", key.fingerprint))
        })
        .collect();

    (summaries, key_count)
}

// Serialize only public display metadata across the QML boundary.
pub(super) fn key_records(keys: &[KeyInfo]) -> QStringList {
    keys.iter()
        .map(key_record)
        .map(|record| QString::from(record.to_string()))
        .collect()
}

fn key_record(key: &KeyInfo) -> Value {
    let identity = preferred_identity(key);
    let (validity_code, validity_label) = validity_presentation(identity);
    let status = status_presentation(key.revoked, key.expired, key.disabled, key.invalid);
    let can_encrypt = has_usable_subkey(key, |subkey| subkey.can_encrypt);
    let can_sign = has_usable_subkey(key, |subkey| subkey.can_sign);
    let can_certify = primary_can_certify(key);
    let can_authenticate = has_usable_subkey(key, |subkey| subkey.can_authenticate);

    json!({
        "id": key.fingerprint,
        "title": identity_title(identity),
        "email": identity.and_then(|uid| uid.email.as_deref()).unwrap_or(""),
        "hasSecret": key.has_secret,
        "canEncrypt": can_encrypt,
        "canCertify": can_certify,
        "capabilities": capabilities(can_encrypt, can_sign, can_certify, can_authenticate),
        "validity": validity_label,
        "validityCode": validity_code,
        "status": status.label,
        "statusCode": status.code,
        "identities": identity_records(key),
        "subkeys": subkey_records(key),
        "signatures": signature_records(key),
    })
}

fn has_usable_subkey(key: &KeyInfo, capability: impl Fn(&SubkeyInfo) -> bool) -> bool {
    key_is_usable(key)
        && key
            .subkeys
            .iter()
            .any(|subkey| subkey_is_usable(subkey) && capability(subkey))
}

fn primary_can_certify(key: &KeyInfo) -> bool {
    key_is_usable(key)
        && key
            .subkeys
            .first()
            .is_some_and(|subkey| subkey_is_usable(subkey) && subkey.can_certify)
}

fn key_is_usable(key: &KeyInfo) -> bool {
    !key.revoked && !key.expired && !key.disabled && !key.invalid
}

fn subkey_is_usable(subkey: &SubkeyInfo) -> bool {
    !subkey.revoked && !subkey.expired && !subkey.disabled && !subkey.invalid
}

fn preferred_identity(key: &KeyInfo) -> Option<&UserIdInfo> {
    key.user_ids
        .iter()
        .find(|user_id| !user_id.revoked && !user_id.invalid)
        .or_else(|| key.user_ids.first())
}

fn identity_title(identity: Option<&UserIdInfo>) -> &str {
    identity
        .and_then(|user_id| user_id.name.as_deref())
        .filter(|name| !name.is_empty())
        .unwrap_or("Sin nombre")
}

fn validity_presentation(identity: Option<&UserIdInfo>) -> (&'static str, &'static str) {
    match identity {
        Some(user_id) if user_id.revoked => ("revoked", "Identidad revocada"),
        Some(user_id) if user_id.invalid => ("invalid", "Identidad inválida"),
        Some(user_id) => match user_id.validity {
            KeyValidity::Unknown => ("unknown", "Sin verificar"),
            KeyValidity::Undefined => ("undefined", "Sin verificar"),
            KeyValidity::Never => ("never", "No válida según GnuPG"),
            KeyValidity::Marginal => ("marginal", "Validez marginal según GnuPG"),
            KeyValidity::Full => ("full", "Validez plena según GnuPG"),
            KeyValidity::Ultimate => ("ultimate", "Validez absoluta según GnuPG"),
        },
        None => ("unknown", "Sin verificar"),
    }
}

fn subkey_records(key: &KeyInfo) -> Vec<Value> {
    key.subkeys
        .iter()
        .enumerate()
        .map(|(index, subkey)| subkey_record(subkey, index == 0))
        .collect()
}

fn subkey_record(subkey: &SubkeyInfo, is_primary: bool) -> Value {
    let status = status_presentation(
        subkey.revoked,
        subkey.expired,
        subkey.disabled,
        subkey.invalid,
    );

    json!({
        "fingerprint": subkey.fingerprint,
        "isPrimary": is_primary,
        "algorithm": subkey.algorithm,
        "expiresAt": subkey.expires_at,
        "capabilities": capabilities(
            subkey.can_encrypt,
            subkey.can_sign,
            subkey.can_certify,
            subkey.can_authenticate,
        ),
        "status": status.label,
        "statusCode": status.code,
    })
}

fn capabilities(encrypt: bool, sign: bool, certify: bool, authenticate: bool) -> Vec<&'static str> {
    [
        (encrypt, "Cifrar"),
        (sign, "Firmar"),
        (certify, "Certificar"),
        (authenticate, "Autenticar"),
    ]
    .into_iter()
    .filter_map(|(enabled, name)| enabled.then_some(name))
    .collect()
}

#[derive(Clone, Copy)]
struct StatusPresentation {
    label: &'static str,
    code: &'static str,
}

fn status_presentation(
    revoked: bool,
    expired: bool,
    disabled: bool,
    invalid: bool,
) -> StatusPresentation {
    if revoked {
        StatusPresentation {
            label: "Revocada",
            code: "revoked",
        }
    } else if expired {
        StatusPresentation {
            label: "Caducada",
            code: "expired",
        }
    } else if disabled {
        StatusPresentation {
            label: "Deshabilitada",
            code: "disabled",
        }
    } else if invalid {
        StatusPresentation {
            label: "Inválida",
            code: "invalid",
        }
    } else {
        StatusPresentation {
            label: "Vigente",
            code: "valid",
        }
    }
}

fn identity_records(key: &KeyInfo) -> Vec<Value> {
    let primary_identity = key
        .user_ids
        .iter()
        .position(|identity| !identity.revoked && !identity.invalid);

    key.user_ids
        .iter()
        .enumerate()
        .map(|(index, identity)| identity_record(identity, Some(index) == primary_identity))
        .collect()
}

fn identity_record(identity: &UserIdInfo, primary: bool) -> Value {
    let (code, label) = validity_presentation(Some(identity));
    json!({
        "id": identity.id,
        "primary": primary,
        "revoked": identity.revoked,
        "invalid": identity.invalid,
        "validity": label,
        "validityCode": code,
    })
}

fn signature_records(key: &KeyInfo) -> Vec<Value> {
    key.user_ids
        .iter()
        .flat_map(|identity| {
            identity
                .signatures
                .iter()
                .filter_map(|signature| signature_record(key, identity, signature))
        })
        .collect()
}

fn signature_record(
    key: &KeyInfo,
    identity: &UserIdInfo,
    signature: &CertificationInfo,
) -> Option<Value> {
    // Self-certifications belong to the key structure, not third-party endorsements.
    if !signature.signer_key_id.is_empty() && key.fingerprint.ends_with(&signature.signer_key_id) {
        return None;
    }

    Some(json!({
        "identity": identity.id,
        "signer": signature.signer,
        "keyId": signature.signer_key_id,
        "status": signature_status_label(signature),
    }))
}

fn signature_status_label(signature: &CertificationInfo) -> &'static str {
    if signature.revoked {
        "Revocación registrada"
    } else if signature.expired {
        "Caducada"
    } else if signature.invalid {
        "Inválida"
    } else {
        "Certificación registrada"
    }
}

#[cfg(test)]
#[path = "keyring_projection_tests.rs"]
mod tests;
