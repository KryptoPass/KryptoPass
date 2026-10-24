use super::{key_records, key_summaries};
use cxx_qt_lib::QStringList;
use kryptopass_keyring::{
    CertificationInfo, Fingerprint, KeyInfo, SubkeyInfo, UserIdInfo, Validity,
};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn fingerprint(value: &str) -> Fingerprint {
    match Fingerprint::parse(value) {
        Ok(fingerprint) => fingerprint,
        Err(error) => panic!("synthetic fingerprint {value} rejected: {error:?}"),
    }
}

fn at(seconds: u64) -> Option<SystemTime> {
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

fn key(fingerprint: &str) -> KeyInfo {
    KeyInfo {
        fingerprint: self::fingerprint(fingerprint),
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
        id: id.into(),
        name: None,
        email: None,
        comment: None,
        validity: Validity::Unknown,
        revoked: false,
        invalid: false,
        certifications: Vec::new(),
    }
}

fn subkey(fingerprint: &str) -> SubkeyInfo {
    SubkeyInfo {
        fingerprint: Some(self::fingerprint(fingerprint)),
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
        revocation: false,
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
    revoked_identity.validity = Validity::Ultimate;
    revoked_identity.revoked = true;
    revoked_identity
        .certifications
        .push(certification("CAFE1234", "Earlier signer"));

    let mut invalid_identity = user_id("Invalid User <invalid@example.test>");
    invalid_identity.name = Some("Invalid User".into());
    invalid_identity.validity = Validity::Never;
    invalid_identity.invalid = true;

    let mut preferred_identity = user_id("Preferred User <preferred@example.test>");
    preferred_identity.name = Some("Preferred Name".into());
    preferred_identity.email = Some("preferred@example.test".into());
    preferred_identity.validity = Validity::Marginal;

    let mut self_certification = certification("01234567", "Self signer");
    self_certification.signer_key_id = "01234567".into();

    let mut signer_without_key_id = certification("", "Signer without key ID");
    signer_without_key_id.revocation = true;
    signer_without_key_id.expired = true;
    signer_without_key_id.invalid = true;

    let mut expired_certification = certification("FEDCBA98", "Expired signer");
    expired_certification.expired = true;
    expired_certification.invalid = true;

    let mut invalid_certification = certification("ABCDEF99", "Invalid signer");
    invalid_certification.invalid = true;

    preferred_identity.certifications = vec![
        self_certification,
        signer_without_key_id,
        expired_certification,
        invalid_certification,
        certification("1234ABCD", "Registered signer"),
    ];

    let mut unknown_identity = user_id("Unknown <unknown@example.test>");
    unknown_identity.validity = Validity::Unknown;

    let mut undefined_identity = user_id("Undefined <undefined@example.test>");
    undefined_identity.validity = Validity::Undefined;

    let mut never_identity = user_id("Never <never@example.test>");
    never_identity.validity = Validity::Never;

    let mut full_identity = user_id("Full <full@example.test>");
    full_identity.validity = Validity::Full;

    let mut ultimate_identity = user_id("Ultimate <ultimate@example.test>");
    ultimate_identity.validity = Validity::Ultimate;

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

    let mut subkey = subkey("5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B");
    subkey.algorithm = "ed25519".into();
    subkey.expires_at = at(1_700_000_000);
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
                "fingerprint": "5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B",
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
    let mut revoked = key("A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1");
    revoked.has_secret = true;
    revoked.revoked = true;
    revoked.expired = true;
    revoked.disabled = true;
    revoked.invalid = true;

    let mut expired = key("A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2");
    expired.expired = true;
    expired.disabled = true;
    expired.invalid = true;

    let mut disabled = key("A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3");
    disabled.disabled = true;
    disabled.invalid = true;

    let mut invalid = key("A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4");
    invalid.invalid = true;

    let mut status_revoked = subkey("B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1");
    status_revoked.expires_at = at(1_700_000_000);
    status_revoked.can_encrypt = true;
    status_revoked.can_sign = true;
    status_revoked.can_certify = true;
    status_revoked.can_authenticate = true;
    status_revoked.revoked = true;
    status_revoked.expired = true;
    status_revoked.disabled = true;
    status_revoked.invalid = true;

    let mut status_expired = subkey("B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2");
    status_expired.can_encrypt = true;
    status_expired.can_sign = true;
    status_expired.expired = true;
    status_expired.disabled = true;
    status_expired.invalid = true;

    let mut status_disabled = subkey("B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3");
    status_disabled.expires_at = at(0);
    status_disabled.can_sign = true;
    status_disabled.can_authenticate = true;
    status_disabled.disabled = true;
    status_disabled.invalid = true;

    let mut status_invalid = subkey("B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4");
    status_invalid.expires_at = at(1);
    status_invalid.can_certify = true;
    status_invalid.invalid = true;

    let status_valid = subkey("B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5");

    let mut valid = key("A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5");
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
                "id": "A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1A1", "title": "Sin nombre", "email": "", "hasSecret": true,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Revocada", "statusCode": "revoked",
                "identities": [], "subkeys": [], "signatures": [],
            },
            {
                "id": "A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2A2", "title": "Sin nombre", "email": "", "hasSecret": false,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Caducada", "statusCode": "expired",
                "identities": [], "subkeys": [], "signatures": [],
            },
            {
                "id": "A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3A3", "title": "Sin nombre", "email": "", "hasSecret": false,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Deshabilitada", "statusCode": "disabled",
                "identities": [], "subkeys": [], "signatures": [],
            },
            {
                "id": "A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4A4", "title": "Sin nombre", "email": "", "hasSecret": false,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Inválida", "statusCode": "invalid",
                "identities": [], "subkeys": [], "signatures": [],
            },
            {
                "id": "A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5A5", "title": "Sin nombre", "email": "", "hasSecret": false,
                "canEncrypt": false, "canCertify": false, "capabilities": [],
                "validity": "Sin verificar", "validityCode": "unknown", "status": "Vigente", "statusCode": "valid",
                "identities": [],
                "subkeys": [
                    {"fingerprint": "B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1B1", "isPrimary": true, "algorithm": "rsa", "expiresAt": 1_700_000_000_u64, "capabilities": ["Cifrar", "Firmar", "Certificar", "Autenticar"], "status": "Revocada", "statusCode": "revoked"},
                    {"fingerprint": "B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2B2", "isPrimary": false, "algorithm": "rsa", "expiresAt": null, "capabilities": ["Cifrar", "Firmar"], "status": "Caducada", "statusCode": "expired"},
                    {"fingerprint": "B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3B3", "isPrimary": false, "algorithm": "rsa", "expiresAt": 0, "capabilities": ["Firmar", "Autenticar"], "status": "Deshabilitada", "statusCode": "disabled"},
                    {"fingerprint": "B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4B4", "isPrimary": false, "algorithm": "rsa", "expiresAt": 1, "capabilities": ["Certificar"], "status": "Inválida", "statusCode": "invalid"},
                    {"fingerprint": "B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5B5", "isPrimary": false, "algorithm": "rsa", "expiresAt": null, "capabilities": [], "status": "Vigente", "statusCode": "valid"},
                ],
                "signatures": [],
            },
        ])
    );

    Ok(())
}

#[test]
fn summaries_use_the_first_identity_and_keep_key_order() {
    let mut first = key("C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1");
    let mut first_identity = user_id("First identity");
    first_identity.revoked = true;
    let mut preferred_identity = user_id("Preferred identity");
    preferred_identity.name = Some("Preferred Name".into());
    first.user_ids = vec![first_identity, preferred_identity];

    let second = key("C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2");

    let mut third = key("C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3");
    third.user_ids.push(user_id("Third identity"));

    let (summaries, key_count) = key_summaries(vec![first, second, third]);

    assert_eq!(key_count, 3);
    assert_eq!(
        strings(&summaries),
        [
            "First identity\nC1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1C1",
            "Sin identidad\nC2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2C2",
            "Third identity\nC3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3C3",
        ]
    );
}
