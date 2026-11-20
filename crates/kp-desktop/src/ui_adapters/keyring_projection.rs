use cxx_qt_lib::{QString, QStringList};
use kryptopass_keyring::{
    CertificationInfo, Fingerprint, KeyInfo, SubkeyInfo, UserIdInfo, Validity,
};
use serde_json::{Value, json};
use std::time::UNIX_EPOCH;

pub(super) fn key_summaries(keys: Vec<KeyInfo>) -> (QStringList, i32) {
    let key_count = i32::try_from(keys.len()).unwrap_or(i32::MAX);
    let summaries = keys
        .into_iter()
        .map(|key| {
            let identity = key
                .user_ids
                .first()
                .map(|user_id| user_id.id.as_str())
                .unwrap_or(crate::i18n::tr_noop("No identity"));

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
        "id": key.fingerprint.as_str(),
        "title": identity.and_then(|uid| uid.name.as_deref()).filter(|name| !name.is_empty()).map(str::to_owned).unwrap_or_else(|| crate::i18n::tr(identity_title(identity))),
        "email": identity.and_then(|uid| uid.email.as_deref()).unwrap_or(""),
        "hasSecret": key.has_secret,
        "ownerTrust": owner_trust_code(key.owner_trust),
        // A key that is here with its private part but that GnuPG does not trust as the
        // user's own cannot be an encryption recipient until the user marks it.
        "needsOwnTrust": key.has_secret && key.owner_trust != Validity::Ultimate,
        "canEncrypt": can_encrypt,
        "canCertify": can_certify,
        "capabilities": capabilities(can_encrypt, can_sign, can_certify, can_authenticate),
        "validity": crate::i18n::tr(validity_label),
        "validityCode": validity_code,
        "status": crate::i18n::tr(status.label),
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
        .unwrap_or(crate::i18n::tr_noop("No name"))
}

fn validity_presentation(identity: Option<&UserIdInfo>) -> (&'static str, &'static str) {
    match identity {
        Some(user_id) if user_id.revoked => ("revoked", crate::i18n::tr_noop("Revoked identity")),
        Some(user_id) if user_id.invalid => ("invalid", crate::i18n::tr_noop("Invalid identity")),
        Some(user_id) => match user_id.validity {
            Validity::Unknown => ("unknown", crate::i18n::tr_noop("Not verified")),
            Validity::Undefined => ("undefined", crate::i18n::tr_noop("Not verified")),
            Validity::Never => (
                "never",
                crate::i18n::tr_noop("Not valid according to GnuPG"),
            ),
            Validity::Marginal => (
                "marginal",
                crate::i18n::tr_noop("Marginal validity according to GnuPG"),
            ),
            Validity::Full => (
                "full",
                crate::i18n::tr_noop("Full validity according to GnuPG"),
            ),
            Validity::Ultimate => (
                "ultimate",
                crate::i18n::tr_noop("Ultimate validity according to GnuPG"),
            ),
        },
        None => ("unknown", crate::i18n::tr_noop("Not verified")),
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
        "fingerprint": subkey.fingerprint.as_ref().map(Fingerprint::as_str).unwrap_or_default(),
        "isPrimary": is_primary,
        "canEncrypt": subkey.can_encrypt,
        "canSign": subkey.can_sign,
        "canAuthenticate": subkey.can_authenticate,
        "canCertify": subkey.can_certify,
        "algorithm": subkey.algorithm,
        "expiresAt": subkey.expires_at.and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|age| age.as_secs()),
        "capabilities": capabilities(
            subkey.can_encrypt,
            subkey.can_sign,
            subkey.can_certify,
            subkey.can_authenticate,
        ),
        "status": crate::i18n::tr(status.label),
        "statusCode": status.code,
    })
}

fn capabilities(encrypt: bool, sign: bool, certify: bool, authenticate: bool) -> Vec<String> {
    [
        (encrypt, crate::i18n::tr_noop("Encrypt")),
        (sign, crate::i18n::tr_noop("Sign")),
        (certify, crate::i18n::tr_noop("Certify")),
        (authenticate, crate::i18n::tr_noop("Authenticate")),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .map(|(_, name)| crate::i18n::tr(name))
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
            label: crate::i18n::tr_noop("Revoked"),
            code: "revoked",
        }
    } else if expired {
        StatusPresentation {
            label: crate::i18n::tr_noop("Expired"),
            code: "expired",
        }
    } else if disabled {
        StatusPresentation {
            label: crate::i18n::tr_noop("Disabled"),
            code: "disabled",
        }
    } else if invalid {
        StatusPresentation {
            label: crate::i18n::tr_noop("Invalid"),
            code: "invalid",
        }
    } else {
        StatusPresentation {
            label: crate::i18n::tr_noop("Valid"),
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
        "validity": crate::i18n::tr(label),
        "validityCode": code,
    })
}

fn signature_records(key: &KeyInfo) -> Vec<Value> {
    key.user_ids
        .iter()
        .flat_map(|identity| {
            identity
                .certifications
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
    if !signature.signer_key_id.is_empty()
        && key.fingerprint.as_str().ends_with(&signature.signer_key_id)
    {
        return None;
    }

    Some(json!({
        "identity": identity.id,
        "signer": signature.signer,
        "keyId": signature.signer_key_id,
        "status": crate::i18n::tr(signature_status_label(signature)),
    }))
}

fn signature_status_label(signature: &CertificationInfo) -> &'static str {
    if signature.revocation {
        crate::i18n::tr_noop("Revocation recorded")
    } else if signature.expired {
        crate::i18n::tr_noop("Expired")
    } else if signature.invalid {
        crate::i18n::tr_noop("Invalid")
    } else {
        crate::i18n::tr_noop("Certification recorded")
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ui_adapters/keyring_projection.rs"]
mod tests;

fn owner_trust_code(trust: Validity) -> &'static str {
    match trust {
        Validity::Ultimate => "ultimate",
        Validity::Full => "full",
        Validity::Marginal => "marginal",
        Validity::Never => "never",
        Validity::Undefined => "undefined",
        Validity::Unknown => "unknown",
    }
}
