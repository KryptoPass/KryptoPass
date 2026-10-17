use super::{key_records, key_summaries};
use cxx_qt_lib::QStringList;
use kryptopass_core::keyring::{CertificationInfo, SubkeyInfo};
use kryptopass_core::{KeyInfo, KeyValidity, UserIdInfo};
use serde_json::{Value, json};

fn key(fingerprint: &str) -> KeyInfo {
    KeyInfo {
        fingerprint: fingerprint.into(),
        user_ids: Vec::new(),
        subkeys: Vec::new(),
        has_secret: false,
        can_encrypt: false,
        can_sign: false,
        can_certify: false,
        can_authenticate: false,
        revoked: false,
        expired: false,
        disabled: false,
        invalid: false,
    }
}

fn user_id(id: &str) -> UserIdInfo {
    UserIdInfo {
        signatures: Vec::new(),
        id: id.into(),
        name: None,
        email: None,
        comment: None,
        validity: KeyValidity::Unknown,
        revoked: false,
        invalid: false,
    }
}

fn subkey(fingerprint: &str) -> SubkeyInfo {
    SubkeyInfo {
        fingerprint: fingerprint.into(),
        algorithm: "rsa".into(),
        expires_at: None,
        can_encrypt: false,
        can_sign: false,
        can_certify: false,
        can_authenticate: false,
        revoked: false,
        expired: false,
        disabled: false,
        invalid: false,
    }
}

fn certification(signer_key_id: &str, signer: &str) -> CertificationInfo {
    CertificationInfo {
        signer_key_id: signer_key_id.into(),
        signer: signer.into(),
        revoked: false,
        expired: false,
        invalid: false,
    }
}

fn record_values(keys: &[KeyInfo]) -> Result<Value, serde_json::Error> {
    let records = key_records(keys)
        .iter()
        .map(|record| serde_json::from_str(&record.to_string()))
        .collect::<Result<Vec<Value>, _>>()?;
    Ok(Value::Array(records))
}

fn strings(values: &QStringList) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

#[test]
fn records_keep_identity_validity_and_certification_contract() -> Result<(), serde_json::Error> {
    let fingerprint = "0123456789ABCDEF0123456789ABCDEF01234567";

    let mut revoked_identity = user_id("Revoked User <revoked@example.test>");
    revoked_identity.name = Some("Revoked User".into());
    revoked_identity.email = Some("revoked@example.test".into());
    revoked_identity.validity = KeyValidity::Ultimate;
    revoked_identity.revoked = true;
    revoked_identity
        .signatures
        .push(certification("CAFE1234", "Earlier signer"));

    let mut invalid_identity = user_id("Invalid User <invalid@example.test>");
    invalid_identity.name = Some("Invalid User".into());
    invalid_identity.validity = KeyValidity::Never;
    invalid_identity.invalid = true;

    let mut preferred_identity = user_id("Preferred User <preferred@example.test>");
    preferred_identity.name = Some("Preferred Name".into());
    preferred_identity.email = Some("preferred@example.test".into());
    preferred_identity.validity = KeyValidity::Marginal;

    let mut self_certification = certification("01234567", "Self signer");
    self_certification.signer_key_id = "01234567".into();

    let mut signer_without_key_id = certification("", "Signer without key ID");
    signer_without_key_id.revoked = true;
    signer_without_key_id.expired = true;
    signer_without_key_id.invalid = true;

    let mut expired_certification = certification("FEDCBA98", "Expired signer");
    expired_certification.expired = true;
    expired_certification.invalid = true;

    let mut invalid_certification = certification("ABCDEF99", "Invalid signer");
    invalid_certification.invalid = true;

    preferred_identity.signatures = vec![
        self_certification,
        signer_without_key_id,
        expired_certification,
        invalid_certification,
        certification("1234ABCD", "Registered signer"),
    ];

    let mut unknown_identity = user_id("Unknown <unknown@example.test>");
    unknown_identity.validity = KeyValidity::Unknown;

    let mut undefined_identity = user_id("Undefined <undefined@example.test>");
    undefined_identity.validity = KeyValidity::Undefined;

    let mut never_identity = user_id("Never <never@example.test>");
    never_identity.validity = KeyValidity::Never;

    let mut full_identity = user_id("Full <full@example.test>");
    full_identity.validity = KeyValidity::Full;

    let mut ultimate_identity = user_id("Ultimate <ultimate@example.test>");
    ultimate_identity.validity = KeyValidity::Ultimate;

    let mut public_key = key(fingerprint);
    public_key.has_secret = true;
    public_key.user_ids = vec![
        revoked_identity,
        invalid_identity,
        preferred_identity,
        unknown_identity,
        undefined_identity,
        never_identity,
        full_identity,
        ultimate_identity,
    ];

    let mut subkey = subkey("subkey-fingerprint");
    subkey.algorithm = "ed25519".into();
    subkey.expires_at = Some(1_700_000_000);
    subkey.can_encrypt = true;
    subkey.can_sign = true;
    subkey.can_authenticate = true;
    public_key.subkeys.push(subkey);

    assert_eq!(
        record_values(&[public_key])?,
        json!([{
            "id": fingerprint,
            "title": "Preferred Name",
            "email": "preferred@example.test",
            "hasSecret": true,
            "canEncrypt": true,
            "canCertify": false,
            "capabilities": ["Cifrar", "Firmar", "Autenticar"],
            "validity": "Validez marginal según GnuPG",
            "validityCode": "marginal",
            "status": "Vigente",
            "statusCode": "valid",
            "identities": [
                {"id": "Revoked User <revoked@example.test>", "primary": false, "revoked": true, "invalid": false, "validity": "Identidad revocada", "validityCode": "revoked"},
                {"id": "Invalid User <invalid@example.test>", "primary": false, "revoked": false, "invalid": true, "validity": "Identidad inválida", "validityCode": "invalid"},
                {"id": "Preferred User <preferred@example.test>", "primary": true, "revoked": false, "invalid": false, "validity": "Validez marginal según GnuPG", "validityCode": "marginal"},
                {"id": "Unknown <unknown@example.test>", "primary": false, "revoked": false, "invalid": false, "validity": "Sin verificar", "validityCode": "unknown"},
                {"id": "Undefined <undefined@example.test>", "primary": false, "revoked": false, "invalid": false, "validity": "Sin verificar", "validityCode": "undefined"},
                {"id": "Never <never@example.test>", "primary": false, "revoked": false, "invalid": false, "validity": "No válida según GnuPG", "validityCode": "never"},
                {"id": "Full <full@example.test>", "primary": false, "revoked": false, "invalid": false, "validity": "Validez plena según GnuPG", "validityCode": "full"},
                {"id": "Ultimate <ultimate@example.test>", "primary": false, "revoked": false, "invalid": false, "validity": "Validez absoluta según GnuPG", "validityCode": "ultimate"},
            ],
            "subkeys": [{
                "fingerprint": "subkey-fingerprint",
                "isPrimary": true,
                "algorithm": "ed25519",
                "expiresAt": 1_700_000_000_u64,
                "capabilities": ["Cifrar", "Firmar", "Autenticar"],
                "status": "Vigente",
                "statusCode": "valid",
            }],
            "signatures": [
                {"identity": "Revoked User <revoked@example.test>", "signer": "Earlier signer", "keyId": "CAFE1234", "status": "Certificación registrada"},
                {"identity": "Preferred User <preferred@example.test>", "signer": "Signer without key ID", "keyId": "", "status": "Revocación registrada"},
                {"identity": "Preferred User <preferred@example.test>", "signer": "Expired signer", "keyId": "FEDCBA98", "status": "Caducada"},
                {"identity": "Preferred User <preferred@example.test>", "signer": "Invalid signer", "keyId": "ABCDEF99", "status": "Inválida"},
                {"identity": "Preferred User <preferred@example.test>", "signer": "Registered signer", "keyId": "1234ABCD", "status": "Certificación registrada"},
            ],
        }])
    );

    Ok(())
}

#[test]
fn records_keep_status_precedence_capability_order_and_optional_fields()
-> Result<(), serde_json::Error> {
    let mut revoked = key("key-revoked");
    revoked.has_secret = true;
    revoked.revoked = true;
    revoked.expired = true;
    revoked.disabled = true;
    revoked.invalid = true;

    let mut expired = key("key-expired");
    expired.expired = true;
    expired.disabled = true;
    expired.invalid = true;

    let mut disabled = key("key-disabled");
    disabled.disabled = true;
    disabled.invalid = true;

    let mut invalid = key("key-invalid");
    invalid.invalid = true;

    let mut status_revoked = subkey("subkey-revoked");
    status_revoked.expires_at = Some(1_700_000_000);
    status_revoked.can_encrypt = true;
    status_revoked.can_sign = true;
    status_revoked.can_certify = true;
    status_revoked.can_authenticate = true;
    status_revoked.revoked = true;
    status_revoked.expired = true;
    status_revoked.disabled = true;
    status_revoked.invalid = true;

    let mut status_expired = subkey("subkey-expired");
    status_expired.can_encrypt = true;
    status_expired.can_sign = true;
    status_expired.expired = true;
    status_expired.disabled = true;
    status_expired.invalid = true;

    let mut status_disabled = subkey("subkey-disabled");
    status_disabled.expires_at = Some(0);
    status_disabled.can_sign = true;
    status_disabled.can_authenticate = true;
    status_disabled.disabled = true;
    status_disabled.invalid = true;

    let mut status_invalid = subkey("subkey-invalid");
    status_invalid.expires_at = Some(1);
    status_invalid.can_certify = true;
    status_invalid.invalid = true;

    let status_valid = subkey("subkey-valid");

    let mut valid = key("key-valid");
    valid.subkeys = vec![
        status_revoked,
        status_expired,
        status_disabled,
        status_invalid,
        status_valid,
    ];

    assert_eq!(
        record_values(&[revoked, expired, disabled, invalid, valid])?,
        json!([
            {
                "id": "key-revoked", "title": "Sin nombre", "email": "", "hasSecret": true,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Revocada", "statusCode": "revoked",
                "identities": [], "subkeys": [], "signatures": [],
            },
            {
                "id": "key-expired", "title": "Sin nombre", "email": "", "hasSecret": false,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Caducada", "statusCode": "expired",
                "identities": [], "subkeys": [], "signatures": [],
            },
            {
                "id": "key-disabled", "title": "Sin nombre", "email": "", "hasSecret": false,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Deshabilitada", "statusCode": "disabled",
                "identities": [], "subkeys": [], "signatures": [],
            },
            {
                "id": "key-invalid", "title": "Sin nombre", "email": "", "hasSecret": false,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Inválida", "statusCode": "invalid",
                "identities": [], "subkeys": [], "signatures": [],
            },
            {
                "id": "key-valid", "title": "Sin nombre", "email": "", "hasSecret": false,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Vigente", "statusCode": "valid",
                "identities": [],
                "subkeys": [
                    {"fingerprint": "subkey-revoked", "isPrimary": true, "algorithm": "rsa", "expiresAt": 1_700_000_000_u64, "capabilities": ["Cifrar", "Firmar", "Certificar", "Autenticar"], "status": "Revocada", "statusCode": "revoked"},
                    {"fingerprint": "subkey-expired", "isPrimary": false, "algorithm": "rsa", "expiresAt": null, "capabilities": ["Cifrar", "Firmar"], "status": "Caducada", "statusCode": "expired"},
                    {"fingerprint": "subkey-disabled", "isPrimary": false, "algorithm": "rsa", "expiresAt": 0, "capabilities": ["Firmar", "Autenticar"], "status": "Deshabilitada", "statusCode": "disabled"},
                    {"fingerprint": "subkey-invalid", "isPrimary": false, "algorithm": "rsa", "expiresAt": 1, "capabilities": ["Certificar"], "status": "Inválida", "statusCode": "invalid"},
                    {"fingerprint": "subkey-valid", "isPrimary": false, "algorithm": "rsa", "expiresAt": null, "capabilities": [], "status": "Vigente", "statusCode": "valid"},
                ],
                "signatures": [],
            },
        ])
    );

    Ok(())
}

#[test]
fn summaries_use_the_first_identity_and_keep_key_order() {
    let mut first = key("KEY-1");
    let mut first_identity = user_id("First identity");
    first_identity.revoked = true;
    let mut preferred_identity = user_id("Preferred identity");
    preferred_identity.name = Some("Preferred Name".into());
    first.user_ids = vec![first_identity, preferred_identity];

    let second = key("KEY-2");

    let mut third = key("KEY-3");
    third.user_ids.push(user_id("Third identity"));

    let (summaries, key_count) = key_summaries(vec![first, second, third]);

    assert_eq!(key_count, 3);
    assert_eq!(
        strings(&summaries),
        [
            "First identity\nKEY-1",
            "Sin identidad\nKEY-2",
            "Third identity\nKEY-3",
        ]
    );
}
