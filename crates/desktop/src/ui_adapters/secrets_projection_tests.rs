#![allow(clippy::unwrap_used)]

use super::*;
use kryptopass_core::{LoginContent, NoteContent, TotpAlgorithm};
use serde_json::Value;

const RECIPIENT: &str = "0123456789ABCDEF0123456789ABCDEF01234567";
const TOTP_SECRET: &str = "JBSWY3DPEHPK3PXP";

fn login() -> LoginContent {
    LoginContent {
        username: "alice@example.invalid".to_owned(),
        password: "current-password-marker".to_owned(),
        url: "https://example.invalid/login".to_owned(),
        urls: vec![
            "https://example.invalid/login".to_owned(),
            "https://alternate.invalid/login".to_owned(),
        ],
        notes: "Login notes with ñ".to_owned(),
        totp: Some(TotpConfig {
            secret: TOTP_SECRET.to_owned(),
            algorithm: TotpAlgorithm::Sha256,
            digits: 8,
            period: 60,
            issuer: Some("Example".to_owned()),
            account: Some("alice@example.invalid".to_owned()),
            original_label: None,
            extensions: vec![],
        }),
        history: Box::default(),
    }
}

fn opened(content: SecretContent) -> OpenedEntry {
    OpenedEntry {
        id: SecretId::new(),
        folder: Some(FolderId(Uuid::new_v4())),
        title: "Mi elemento".to_owned(),
        revision: 7,
        recipients: vec![RECIPIENT.to_owned()],
        icon: None,
        content,
    }
}

fn editor_json(content: SecretContent) -> Value {
    let entry = opened(content);
    serde_json::from_str(&editor_projection(&entry).unwrap()).unwrap()
}

#[test]
fn login_editor_round_trip_preserves_metadata_and_typed_fields() {
    let entry = opened(SecretContent::Login(login()));
    let projection = editor_projection(&entry).unwrap();
    let mut draft: EntryDraft = serde_json::from_str(&projection).unwrap();
    assert!(draft_matches_session(
        &draft,
        Some(entry.id),
        Some(entry.revision)
    ));
    assert!(!draft_matches_session(
        &draft,
        Some(entry.id),
        Some(entry.revision + 1)
    ));

    let prepared = prepare_save(&mut draft, Some(entry.id), Some(entry.revision)).unwrap();
    assert_eq!(prepared.id, Some(entry.id));
    assert_eq!(prepared.folder, entry.folder);
    assert_eq!(prepared.title, entry.title);
    assert_eq!(prepared.expected_revision, Some(entry.revision));
    let RecipientSelection::Explicit(recipients) = &prepared.recipients else {
        panic!("editor recipients must remain explicit");
    };
    assert_eq!(
        recipients.as_slice(),
        &[KeyFingerprint::parse(RECIPIENT).unwrap()]
    );

    let SecretContent::Login(saved) = &prepared.content else {
        panic!("login editor must save login content");
    };
    let SecretContent::Login(original) = &entry.content else {
        unreachable!();
    };
    assert_eq!(saved.username, original.username);
    assert_eq!(saved.password, original.password);
    assert_eq!(saved.url, original.url);
    assert_eq!(saved.urls, original.urls);
    assert_eq!(saved.notes, original.notes);
    assert!(saved.totp == original.totp);
}

#[test]
fn note_editor_round_trip_uses_only_note_fields() {
    let entry = opened(SecretContent::Note(NoteContent {
        value: "# Nota segura\nTexto privado".to_owned(),
        notes: "Notas adicionales".to_owned(),
    }));
    let projection = editor_projection(&entry).unwrap();
    let json: Value = serde_json::from_str(&projection).unwrap();
    assert_eq!(json["content"]["kind"], "note");
    assert_eq!(
        json["content"]["fields"],
        json!({"value": "# Nota segura\nTexto privado", "notes": "Notas adicionales"})
    );

    let mut draft: EntryDraft = serde_json::from_str(&projection).unwrap();
    let prepared = prepare_save(&mut draft, Some(entry.id), Some(entry.revision)).unwrap();
    let SecretContent::Note(saved) = &prepared.content else {
        panic!("note editor must save note content");
    };
    assert_eq!(saved.value, "# Nota segura\nTexto privado");
    assert_eq!(saved.notes, "Notas adicionales");
    assert_eq!(prepared.title, entry.title);
    assert_eq!(prepared.folder, entry.folder);
}

#[test]
fn new_editor_is_a_deserializable_empty_login() {
    let projection = new_editor_projection().unwrap();
    let json: Value = serde_json::from_str(&projection).unwrap();
    assert_eq!(json["id"], "");
    assert_eq!(json["revision"], Value::Null);
    assert_eq!(json["content"]["kind"], "login");
    assert_eq!(
        json["content"]["fields"],
        json!({
            "username": "",
            "password": "",
            "url": "",
            "urls": [],
            "totpInput": "",
            "notes": ""
        })
    );
    let draft: EntryDraft = serde_json::from_str(&projection).unwrap();
    assert!(draft_matches_session(&draft, None, None));
}

#[test]
fn draft_rejects_unknown_kinds_and_fields_from_another_variant() {
    let login = editor_json(SecretContent::Login(login()));
    let note = editor_json(SecretContent::Note(NoteContent {
        value: String::new(),
        notes: String::new(),
    }));

    let mut unknown = login.clone();
    unknown["content"]["kind"] = json!("card");
    assert!(serde_json::from_value::<EntryDraft>(unknown).is_err());

    for (mut json, field) in [(login, "value"), (note, "username")] {
        json["content"]["fields"][field] = json!("");
        assert!(
            serde_json::from_value::<EntryDraft>(json).is_err(),
            "an empty {field} must still be rejected for the other variant"
        );
    }
}

#[test]
fn draft_rejects_missing_content_and_required_variant_fields() {
    let login = editor_json(SecretContent::Login(login()));
    let note = editor_json(SecretContent::Note(NoteContent {
        value: "note".to_owned(),
        notes: String::new(),
    }));
    let mut no_content = login.clone();
    no_content.as_object_mut().unwrap().remove("content");
    assert!(serde_json::from_value::<EntryDraft>(no_content).is_err());

    for (json, fields) in [
        (login, &["username", "password", "notes"][..]),
        (note, &["value", "notes"][..]),
    ] {
        for field in fields {
            let mut incomplete = json.clone();
            incomplete["content"]["fields"]
                .as_object_mut()
                .unwrap()
                .remove(*field);
            assert!(
                serde_json::from_value::<EntryDraft>(incomplete).is_err(),
                "missing {field} must not silently erase existing data"
            );
        }
    }
}

#[test]
fn login_detail_reveals_only_the_current_password_when_requested() {
    let mut current = login();
    let mut previous = login();
    previous.password = "historical-version-password-marker".to_owned();
    previous.totp.as_mut().unwrap().secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_owned();
    current
        .history
        .record_version(&previous, "Historical title", 40)
        .unwrap();
    current
        .history
        .record_password("historical-password-marker".to_owned(), 20)
        .unwrap();
    current
        .history
        .record_event(LoginHistoryEventKind::PasswordRotated, 40);
    let content = SecretContent::Login(current);

    for reveal in [false, true] {
        let projection =
            content_projection(SecretId::new(), &content, reveal, 1_792_000_001).unwrap();
        let json: Value = serde_json::from_str(&projection).unwrap();
        let fields = &json["content"]["fields"];
        assert_eq!(json["content"]["kind"], "login");
        assert_eq!(fields["passwordRevealed"], reveal);
        assert_eq!(
            fields["password"],
            if reveal {
                "current-password-marker"
            } else {
                ""
            }
        );
        if !reveal {
            assert!(!projection.contains("current-password-marker"));
        }
        assert_eq!(fields["history"]["passwordRotations"], json!([{"at": 20}]));
        assert_eq!(
            fields["history"]["versions"],
            json!([
                {"index": 0, "at": 40, "passwordOnly": false},
                {"index": 1, "at": 20, "passwordOnly": true}
            ])
        );
        assert!(fields["totp"]["currentCode"].is_string());
        assert!(fields["totp"]["nextCode"].is_string());
        assert!(fields.get("totpInput").is_none());
        for secret in [
            TOTP_SECRET,
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ",
            "historical-version-password-marker",
            "historical-password-marker",
        ] {
            assert!(!projection.contains(secret), "detail leaked {secret}");
        }
    }
}

#[test]
fn note_detail_does_not_inherit_login_fields() {
    let id = SecretId::new();
    let content = SecretContent::Note(NoteContent {
        value: "Private note".to_owned(),
        notes: "Additional notes".to_owned(),
    });
    for reveal in [false, true] {
        let projection = content_projection(id, &content, reveal, 1_792_000_001).unwrap();
        let json: Value = serde_json::from_str(&projection).unwrap();
        assert_eq!(
            json,
            json!({
                "id": id.0.to_string(),
                "content": {
                    "kind": "note",
                    "fields": {"value": "Private note", "notes": "Additional notes"}
                }
            })
        );
    }
}
