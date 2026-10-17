#![allow(clippy::unwrap_used)]

use kryptopass_core::{
    CONTENT_SCHEMA_VERSION, LoginContent, LoginHistory, LoginHistoryEventKind, NoteContent,
    SecretContent, SecretKind, TotpAlgorithm, TotpConfig, decode_content, encode_content,
};
use serde_json::{Value, json};

fn envelope(kind: &str, schema_version: u32) -> Value {
    let fields = match kind {
        "login" => {
            let mut fields = json!({
                "username": "alice@example.org",
                "password": "sample password",
                "url": "https://example.org/login",
                "notes": "Recovery instructions"
            });
            if schema_version >= 2 {
                fields["urls"] = json!(["https://example.org/login"]);
                fields["totp"] = Value::Null;
                fields["history"] = json!({
                    "events": [{"timestamp": 42, "kind": "passwordRotated"}],
                    "previousPasswords": [{"password": "previous password", "changedAt": 42}]
                });
            }
            fields
        }
        "note" => json!({"value": "Private note\nSegunda línea 🔐", "notes": "Context"}),
        _ => unreachable!("fixture kind must have implemented content"),
    };
    json!({"schemaVersion": schema_version, "kind": kind, "fields": fields})
}

fn decode_fixture(envelope: &Value) -> SecretContent {
    decode_content(&serde_json::to_vec(envelope).unwrap()).unwrap()
}

fn assert_rejected(envelope: &Value, context: &str) {
    assert!(
        decode_content(&serde_json::to_vec(envelope).unwrap()).is_err(),
        "accepted invalid content: {context}"
    );
}

#[test]
fn current_note_roundtrip_preserves_text_and_emits_the_current_envelope() {
    let content = SecretContent::Note(NoteContent {
        value: "Private note\nSegunda línea 🔐".into(),
        notes: "Context".into(),
    });
    let bytes = encode_content(&content).unwrap();
    let encoded: Value = serde_json::from_slice(bytes.as_slice()).unwrap();
    assert_eq!(CONTENT_SCHEMA_VERSION, 4);
    assert_eq!(encoded, envelope("note", CONTENT_SCHEMA_VERSION));

    let decoded = decode_content(bytes.as_slice()).unwrap();
    assert_eq!(decoded.kind(), SecretKind::SecureNote);
    let SecretContent::Note(note) = &decoded else {
        panic!("expected note content");
    };
    assert_eq!(note.value, "Private note\nSegunda línea 🔐");
    assert_eq!(note.notes, "Context");
}

#[test]
fn current_login_roundtrip_preserves_credentials_sites_totp_and_history() {
    let mut history = LoginHistory::default();
    history.record_event(LoginHistoryEventKind::Create, 10);
    history
        .record_password("previous password".into(), 20)
        .unwrap();
    let content = SecretContent::Login(LoginContent {
        username: "alice@example.org".into(),
        password: "sample password".into(),
        url: "https://example.org/login".into(),
        urls: vec![
            "https://example.org/login".into(),
            "https://example.org/account".into(),
        ],
        notes: "Recovery instructions".into(),
        totp: Some(TotpConfig {
            secret: "JBSWY3DPEHPK3PXP".into(),
            algorithm: TotpAlgorithm::Sha256,
            digits: 8,
            period: 60,
            issuer: Some("Example".into()),
            account: Some("alice@example.org".into()),
            original_label: None,
            extensions: vec![],
        }),
        history: Box::new(history),
    });
    let bytes = encode_content(&content).unwrap();
    let encoded: Value = serde_json::from_slice(bytes.as_slice()).unwrap();
    assert_eq!(encoded["schemaVersion"], CONTENT_SCHEMA_VERSION);
    assert_eq!(encoded["kind"], "login");

    let decoded = decode_content(bytes.as_slice()).unwrap();
    assert_eq!(decoded.kind(), SecretKind::Login);
    let reencoded = encode_content(&decoded).unwrap();
    let actual: Value = serde_json::from_slice(reencoded.as_slice()).unwrap();
    assert_eq!(actual, encoded);
}

#[test]
fn all_supported_note_schemas_decode_without_changing_the_fields() {
    for schema in 1..=CONTENT_SCHEMA_VERSION {
        let content = decode_fixture(&envelope("note", schema));
        let SecretContent::Note(note) = &content else {
            panic!("expected note content for schema {schema}");
        };
        assert_eq!(note.value, "Private note\nSegunda línea 🔐");
        assert_eq!(note.notes, "Context");
        let bytes = encode_content(&content).unwrap();
        let upgraded: Value = serde_json::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(upgraded, envelope("note", CONTENT_SCHEMA_VERSION));
    }
}

#[test]
fn all_supported_login_schemas_preserve_credentials_and_upgrade_to_current() {
    for schema in 1..=CONTENT_SCHEMA_VERSION {
        let content = decode_fixture(&envelope("login", schema));
        let SecretContent::Login(login) = &content else {
            panic!("expected login content for schema {schema}");
        };
        assert_eq!(login.username, "alice@example.org");
        assert_eq!(login.password, "sample password");
        assert_eq!(login.url, "https://example.org/login");
        assert_eq!(login.urls, ["https://example.org/login"]);
        assert_eq!(login.notes, "Recovery instructions");
        assert!(login.totp.is_none());
        if schema == 1 {
            assert!(login.history.events().is_empty());
            assert!(login.history.password_change_timestamps().next().is_none());
        } else {
            assert_eq!(login.history.events().len(), 1);
            assert_eq!(login.history.events()[0].timestamp, 42);
            assert!(login.history.events()[0].kind == LoginHistoryEventKind::PasswordRotated);
            assert_eq!(
                login
                    .history
                    .password_change_timestamps()
                    .collect::<Vec<_>>(),
                [42]
            );
            assert_eq!(
                login.history.version_content(0).unwrap().password,
                "previous password"
            );
        }

        let bytes = encode_content(&content).unwrap();
        let upgraded: Value = serde_json::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(upgraded["schemaVersion"], CONTENT_SCHEMA_VERSION);
        let reread = decode_content(bytes.as_slice()).unwrap();
        assert!(matches!(reread, SecretContent::Login(_)));
    }
}

#[test]
fn legacy_login_with_an_empty_primary_url_upgrades_to_an_empty_site_list() {
    let mut fixture = envelope("login", 1);
    fixture["fields"]["url"] = json!("");
    let content = decode_fixture(&fixture);
    let SecretContent::Login(login) = &content else {
        panic!("expected login content");
    };
    assert!(login.url.is_empty());
    assert!(login.urls.is_empty());
    assert!(encode_content(&content).is_ok());
}

#[test]
fn login_rejects_note_fields_even_when_null() {
    for schema in 1..=CONTENT_SCHEMA_VERSION {
        for value in [Value::Null, json!("unexpected note")] {
            let mut fixture = envelope("login", schema);
            fixture["fields"]["value"] = value;
            assert_rejected(&fixture, "note value in login fields");
        }
    }
}

#[test]
fn note_rejects_login_fields_even_when_null() {
    let login_fields = [
        ("username", json!("alice")),
        ("password", json!("password")),
        ("url", json!("https://example.org")),
        ("urls", json!(["https://example.org"])),
        (
            "totp",
            json!({
                "secret": "JBSWY3DPEHPK3PXP", "algorithm": "sha1", "digits": 6,
                "period": 30, "issuer": null, "account": null
            }),
        ),
        ("history", json!({"events": [], "previousPasswords": []})),
    ];
    for schema in 1..=CONTENT_SCHEMA_VERSION {
        for (field, value) in &login_fields {
            for value in [Value::Null, value.clone()] {
                let mut fixture = envelope("note", schema);
                fixture["fields"][field] = value;
                assert_rejected(&fixture, field);
            }
        }
    }
}

#[test]
fn legacy_login_rejects_newer_fields_even_when_null() {
    let current = envelope("login", CONTENT_SCHEMA_VERSION);
    for field in ["urls", "totp", "history"] {
        for value in [Value::Null, current["fields"][field].clone()] {
            let mut fixture = envelope("login", 1);
            fixture["fields"][field] = value;
            assert_rejected(&fixture, field);
        }
    }
}

#[test]
fn unknown_fields_are_rejected_in_the_envelope_and_each_content_variant() {
    for kind in ["login", "note"] {
        for schema in 1..=CONTENT_SCHEMA_VERSION {
            for value in [Value::Null, json!("unknown")] {
                let mut fixture = envelope(kind, schema);
                fixture["unknown"] = value.clone();
                assert_rejected(&fixture, "unknown envelope field");

                let mut fixture = envelope(kind, schema);
                fixture["fields"]["unknown"] = value;
                assert_rejected(&fixture, "unknown content field");
            }
        }
    }
}

#[test]
fn unknown_or_reserved_kinds_never_decode_as_an_implemented_variant() {
    for kind in ["unknown", "Login", "secure-note", "api-token"] {
        for schema in 1..=CONTENT_SCHEMA_VERSION {
            let mut fixture = envelope("note", schema);
            fixture["kind"] = json!(kind);
            assert_rejected(&fixture, kind);
        }
    }
}

#[test]
fn unsupported_schema_versions_are_rejected_for_every_content_variant() {
    for kind in ["login", "note"] {
        for version in [
            json!(0),
            json!(5),
            json!(u32::MAX),
            json!(-1),
            json!(3.5),
            json!("3"),
            Value::Null,
        ] {
            let mut fixture = envelope(kind, 3);
            fixture["schemaVersion"] = version;
            assert_rejected(&fixture, "unsupported schema version");
        }
    }
}

#[test]
fn required_envelope_fields_cannot_be_absent_or_null() {
    for kind in ["login", "note"] {
        for schema in 1..=CONTENT_SCHEMA_VERSION {
            for field in ["schemaVersion", "kind", "fields"] {
                let mut fixture = envelope(kind, schema);
                fixture.as_object_mut().unwrap().remove(field);
                assert_rejected(&fixture, field);

                let mut fixture = envelope(kind, schema);
                fixture[field] = Value::Null;
                assert_rejected(&fixture, field);
            }
        }
    }
}

#[test]
fn required_content_fields_cannot_be_absent() {
    for kind in ["login", "note"] {
        for schema in 1..=CONTENT_SCHEMA_VERSION {
            let fixture = envelope(kind, schema);
            for field in fixture["fields"].as_object().unwrap().keys() {
                let mut malformed = fixture.clone();
                malformed["fields"].as_object_mut().unwrap().remove(field);
                assert_rejected(&malformed, field);
            }
        }
    }
}

#[test]
fn only_totp_can_be_null_among_required_content_fields() {
    for kind in ["login", "note"] {
        for schema in 1..=CONTENT_SCHEMA_VERSION {
            let fixture = envelope(kind, schema);
            for field in fixture["fields"].as_object().unwrap().keys() {
                let mut malformed = fixture.clone();
                malformed["fields"][field] = Value::Null;
                if field == "totp" {
                    assert!(decode_content(&serde_json::to_vec(&malformed).unwrap()).is_ok());
                } else {
                    assert_rejected(&malformed, field);
                }
            }
        }
    }
}
