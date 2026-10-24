#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;

use kryptopass_core::{
    KeyFingerprint, LoginContent, LoginHistory, LoginHistoryEventKind, NoteContent, SecretContent,
    SecretId, TotpAlgorithm, TotpConfig, TotpUriParameter, VAULT_OBJECT_FORMAT, VaultObject,
    VaultObjectError, decode_vault_object, encode_vault_object,
};
use serde_json::Value;
use uuid::Uuid;

const FINGERPRINT: &str = "A1B2C3D4E5F60718293A4B5C6D7E8F9012345678";

fn record_id() -> SecretId {
    SecretId(Uuid::from_u128(0x1234567890abcdef1234567890abcdef))
}

fn fingerprint() -> KeyFingerprint {
    KeyFingerprint::parse(FINGERPRINT).unwrap()
}

fn sample_login_object() -> VaultObject {
    let old_totp = TotpConfig {
        secret: "JBSWY3DPEHPK3PXP".into(),
        algorithm: TotpAlgorithm::Sha1,
        digits: 6,
        period: 30,
        issuer: Some("Example".into()),
        account: Some("old-user@example.invalid".into()),
        original_label: Some("Example:old-user%40example.invalid".into()),
        extensions: vec![],
    };
    let mut previous = LoginContent {
        username: "old-user@example.invalid".into(),
        password: "synthetic-old-password".into(),
        notes: "old synthetic note".into(),
        urls: vec!["https://old.example.invalid/login".into()],
        totp: Some(old_totp),
        history: Box::default(),
    };

    let mut history = LoginHistory::default();
    history.record_event(LoginHistoryEventKind::Create, 10);
    history
        .record_version(&previous, "Previous synthetic title", 30)
        .unwrap();

    previous.username = "ada@example.invalid".into();
    previous.password = "synthetic-current-password".into();
    previous.notes = "current synthetic note".into();
    previous.urls = vec![
        "https://example.invalid/account".into(),
        "https://example.invalid/security".into(),
    ];
    previous.totp = Some(TotpConfig {
        secret: "JBSWY3DPEHPK3PXP".into(),
        algorithm: TotpAlgorithm::Sha256,
        digits: 8,
        period: 45,
        issuer: Some("Synthetic Issuer".into()),
        account: Some("ada@example.invalid".into()),
        original_label: Some("Synthetic%20Issuer:ada%40example.invalid".into()),
        extensions: vec![TotpUriParameter {
            key: "vendor".into(),
            value: "synthetic-extension".into(),
        }],
    });
    previous.history = Box::new(history);

    VaultObject {
        record_id: record_id(),
        key_fingerprint: fingerprint(),
        title: "Synthetic login".into(),
        favorite: true,
        trashed: false,
        content: SecretContent::Login(previous),
    }
}

fn decode(
    bytes: &[u8],
    id: SecretId,
    key: &KeyFingerprint,
) -> Result<VaultObject, VaultObjectError> {
    decode_vault_object(bytes, id, key)
}

fn append_outer_member(bytes: &[u8], member: &[u8]) -> Vec<u8> {
    assert_eq!(bytes.last(), Some(&b'}'));
    let mut output = bytes[..bytes.len() - 1].to_vec();
    output.push(b',');
    output.extend_from_slice(member);
    output.push(b'}');
    output
}

fn contains_key(value: &Value, expected: &str) -> bool {
    match value {
        Value::Object(object) => object
            .iter()
            .any(|(key, value)| key == expected || contains_key(value, expected)),
        Value::Array(values) => values.iter().any(|value| contains_key(value, expected)),
        _ => false,
    }
}

#[test]
fn login_and_note_roundtrip_keep_metadata_totp_and_history() {
    let login = sample_login_object();
    let encoded = encode_vault_object(&login).unwrap();
    let envelope: Value = serde_json::from_slice(encoded.as_slice()).unwrap();

    assert_eq!(envelope["format"], VAULT_OBJECT_FORMAT);
    assert_eq!(envelope["recordId"], record_id().0.to_string());
    assert_eq!(envelope["keyFingerprint"], FINGERPRINT);
    assert_eq!(envelope["content"]["schemaVersion"], 1);
    assert_eq!(envelope["favorite"], true);
    assert_eq!(envelope["trashed"], false);
    assert_eq!(
        envelope
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        [
            "content",
            "favorite",
            "format",
            "keyFingerprint",
            "recordId",
            "title",
            "trashed",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    for sync_only_field in [
        "cas",
        "casCounter",
        "counter",
        "parent",
        "revision",
        "signature",
    ] {
        assert!(!contains_key(&envelope, sync_only_field));
    }

    let decoded = decode(encoded.as_slice(), record_id(), &fingerprint()).unwrap();
    assert_eq!(decoded.record_id, login.record_id);
    assert_eq!(decoded.key_fingerprint, login.key_fingerprint);
    assert_eq!(decoded.title, login.title);
    assert_eq!(decoded.favorite, login.favorite);
    assert_eq!(decoded.trashed, login.trashed);
    let SecretContent::Login(actual) = &decoded.content else {
        panic!("expected login payload");
    };
    let SecretContent::Login(expected) = &login.content else {
        unreachable!();
    };
    assert_eq!(actual.username, expected.username);
    assert_eq!(actual.password, expected.password);
    assert_eq!(actual.urls, expected.urls);
    assert_eq!(actual.notes, expected.notes);

    let actual_totp = actual.totp.as_ref().unwrap();
    let expected_totp = expected.totp.as_ref().unwrap();
    assert_eq!(actual_totp.secret, expected_totp.secret);
    assert!(actual_totp.algorithm == expected_totp.algorithm);
    assert_eq!(actual_totp.digits, expected_totp.digits);
    assert_eq!(actual_totp.period, expected_totp.period);
    assert_eq!(actual_totp.issuer, expected_totp.issuer);
    assert_eq!(actual_totp.account, expected_totp.account);
    assert_eq!(actual_totp.original_label, expected_totp.original_label);
    assert!(
        actual_totp.extensions == expected_totp.extensions,
        "TOTP URI extensions survive the vault-object roundtrip"
    );

    assert_eq!(actual.history.events().len(), 1);
    assert_eq!(actual.history.events()[0].timestamp, 10);
    assert!(actual.history.events()[0].kind == LoginHistoryEventKind::Create);
    assert_eq!(actual.history.version_metadata(), vec![30]);
    assert_eq!(
        actual.history.version_title(0),
        Some("Previous synthetic title")
    );
    let prior = actual.history.version_content(0).unwrap();
    assert_eq!(prior.username, "old-user@example.invalid");
    assert_eq!(prior.password, "synthetic-old-password");
    assert_eq!(prior.totp.as_ref().unwrap().secret, "JBSWY3DPEHPK3PXP");

    let note = VaultObject {
        record_id: SecretId(Uuid::from_u128(0x2234567890abcdef1234567890abcdef)),
        key_fingerprint: fingerprint(),
        title: "Synthetic secure note".into(),
        favorite: false,
        trashed: true,
        content: SecretContent::Note(NoteContent {
            value: "synthetic note body\nsecond line".into(),
            notes: "synthetic note metadata".into(),
        }),
    };
    let note_bytes = encode_vault_object(&note).unwrap();
    let decoded_note =
        decode(note_bytes.as_slice(), note.record_id, &note.key_fingerprint).unwrap();
    assert_eq!(decoded_note.title, note.title);
    assert!(!decoded_note.favorite);
    assert!(decoded_note.trashed);
    let SecretContent::Note(actual_note) = &decoded_note.content else {
        panic!("expected note payload");
    };
    let SecretContent::Note(expected_note) = &note.content else {
        unreachable!();
    };
    assert_eq!(actual_note.value, expected_note.value);
    assert_eq!(actual_note.notes, expected_note.notes);
}

#[test]
fn envelope_rejects_duplicate_unknown_unsupported_or_mismatched_fields() {
    let object = sample_login_object();
    let encoded = encode_vault_object(&object).unwrap();
    let id = object.record_id;
    let key = object.key_fingerprint.clone();

    let duplicate = append_outer_member(encoded.as_slice(), br#""favorite":false"#);
    assert!(decode(&duplicate, id, &key).is_err());

    let unknown = append_outer_member(encoded.as_slice(), br#""unexpected":null"#);
    assert!(decode(&unknown, id, &key).is_err());

    let mut unsupported: Value = serde_json::from_slice(encoded.as_slice()).unwrap();
    unsupported["content"]["schemaVersion"] = 2.into();
    let unsupported = serde_json::to_vec(&unsupported).unwrap();
    assert!(decode(&unsupported, id, &key).is_err());

    let wrong_record = SecretId(Uuid::from_u128(0x3234567890abcdef1234567890abcdef));
    assert!(decode(encoded.as_slice(), wrong_record, &key).is_err());

    let wrong_key = KeyFingerprint::parse("B1B2C3D4E5F60718293A4B5C6D7E8F9012345678").unwrap();
    assert!(decode(encoded.as_slice(), id, &wrong_key).is_err());

    let mut noncanonical_id: Value = serde_json::from_slice(encoded.as_slice()).unwrap();
    let uppercase_id = noncanonical_id["recordId"]
        .as_str()
        .unwrap()
        .to_ascii_uppercase();
    noncanonical_id["recordId"] = uppercase_id.into();
    let noncanonical_id = serde_json::to_vec(&noncanonical_id).unwrap();
    assert!(decode(&noncanonical_id, id, &key).is_err());
}

#[test]
fn envelope_rejects_oversize_and_invalid_identity_and_fingerprint() {
    let object = sample_login_object();
    let oversized = vec![b' '; kryptopass_core::MAX_VAULT_OBJECT_BYTES + 1];
    assert!(decode(&oversized, object.record_id, &object.key_fingerprint).is_err());

    let mut nil_id = sample_login_object();
    nil_id.record_id = SecretId(Uuid::nil());
    assert!(matches!(
        encode_vault_object(&nil_id),
        Err(VaultObjectError::Invalid)
    ));
    let valid = encode_vault_object(&object).unwrap();
    assert!(decode(&valid, SecretId(Uuid::nil()), &object.key_fingerprint).is_err());

    let mut lowercase_fingerprint = sample_login_object();
    lowercase_fingerprint.key_fingerprint =
        KeyFingerprint::parse(&FINGERPRINT.to_ascii_lowercase()).unwrap();
    let encoded = encode_vault_object(&lowercase_fingerprint).unwrap();
    let envelope: Value = serde_json::from_slice(encoded.as_slice()).unwrap();
    assert_eq!(envelope["keyFingerprint"], FINGERPRINT);

    let long_fingerprint = KeyFingerprint::parse(&"0123456789ABCDEF".repeat(4)).unwrap();
    let mut long_key_object = sample_login_object();
    long_key_object.key_fingerprint = long_fingerprint.clone();
    let encoded = encode_vault_object(&long_key_object).unwrap();
    let envelope: Value = serde_json::from_slice(encoded.as_slice()).unwrap();
    assert_eq!(envelope["keyFingerprint"], long_fingerprint.as_str());
    assert!(decode(&encoded, long_key_object.record_id, &long_fingerprint).is_ok());
}

#[test]
fn title_limit_counts_utf8_bytes_and_debug_output_redacts_plaintext() {
    let mut object = sample_login_object();
    object.title = "é".repeat(256);
    assert!(encode_vault_object(&object).is_ok());

    object.title.push('é');
    assert!(matches!(
        encode_vault_object(&object),
        Err(VaultObjectError::Invalid)
    ));

    let object = sample_login_object();
    let debug = format!("{object:?}");
    assert!(!debug.contains("Synthetic login"));
    assert!(!debug.contains("synthetic-current-password"));
    assert!(!debug.contains("ada@example.invalid"));
    assert_eq!(debug, "VaultObject([redacted])");

    let encoded = encode_vault_object(&object).unwrap();
    let encoded_debug = format!("{encoded:?}");
    assert!(!encoded_debug.contains("synthetic-current-password"));
    assert!(!encoded_debug.contains("ada@example.invalid"));
}
