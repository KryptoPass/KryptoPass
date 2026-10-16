#![allow(clippy::unwrap_used)]

use kryptopass_core::{CustomContent, ImportItem, SecretContent, SshKeyContent, WifiSecurity};
use kryptopass_import::{ExportImporter, IssueKind, proton::ProtonPassImporter};
use serde_json::{Value, json};

const SYNTHETIC_SSH_PRIVATE_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\nQyNTUxOQAAACDJLwpNKucaM6vcEr3XW2efJklnoPcqbNv+TwOcr3D/NAAAAKCYplIImKZS\nCAAAAAtzc2gtZWQyNTUxOQAAACDJLwpNKucaM6vcEr3XW2efJklnoPcqbNv+TwOcr3D/NA\nAAAED0Onegf8CG/t3VAnCjg2FZfM3U8BLTOpuaJb8PH61VSskvCk0q5xozq9wSvddbZ58m\nSWeg9yps2/5PA5yvcP80AAAAGHN5bnRoZXRpYyBpbXBvcnQgZml4dHVyZQECAwQF\n-----END OPENSSH PRIVATE KEY-----\n";

fn source_item(id: &str, kind: &str, content: Value) -> Value {
    json!({
        "itemId": id,
        "state": 1,
        "data": {
            "metadata": { "name": id, "note": "source notes" },
            "type": kind,
            "content": content
        }
    })
}

fn export(items: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "version": "1",
        "vaults": {
            "vault-id": { "name": "Personal", "items": items }
        }
    }))
    .unwrap()
}

fn import(items: Vec<Value>) -> kryptopass_import::ImportPlan {
    ProtonPassImporter.parse(&export(items)).unwrap()
}

fn content(item: &ImportItem) -> &SecretContent {
    &item.content
}

fn with_item_data_extra(mut item: Value, value: &str) -> Value {
    item["data"]["extraFields"] = json!({ "providerSecret": value });
    item
}

fn concealed_source<'a>(item: &'a ImportItem, key: &str, marker: &str) -> &'a CustomContent {
    let SecretContent::Custom(custom) = content(item) else {
        panic!("{} should be preserved as custom content", item.title);
    };
    assert_eq!(custom.notes, "source notes");
    let source = custom
        .records
        .iter()
        .find(|record| record.key == key)
        .unwrap_or_else(|| panic!("missing concealed source record {key}"));
    assert!(source.concealed);
    assert!(source.value.contains(marker));
    assert!(!custom.notes.contains(marker));
    custom
}

#[test]
fn proton_credit_card_identity_custom_wifi_and_ssh_map_without_losing_unrepresented_fields() {
    let card = source_item(
        "card",
        "creditCard",
        json!({
            "cardholderName": "Ada Example",
            "cardType": "visa",
            "number": "4111-1111 1111-1111",
            "verificationNumber": "123",
            "pin": "2468",
            "expirationDate": "2032-12"
        }),
    );
    let identity_content = json!({
        "firstName": "Ada",
        "lastName": "Example",
        "email": "ada@example.invalid",
        "phoneNumber": "+1 555 0100",
        "streetAddress": "1 Example Street",
        "city": "London",
        "countryOrRegion": "GB",
        "socialSecurityNumber": "123-45-6789",
        "passportNumber": "P123456",
        "licenseNumber": "D1234567"
    });
    let identity = source_item("identity", "identity", identity_content.clone());
    let custom = source_item(
        "custom",
        "custom",
        json!({
            "sections": [{
                "sectionName": "Recovery",
                "sectionFields": [
                    { "fieldName": "Code", "type": "hidden", "data": { "content": "recovery-value" } },
                    { "fieldName": "Memo", "type": "text", "data": { "content": "line one\nline two" } },
                    { "fieldName": "Future", "type": "future-field", "data": { "secret": "opaque-source-value" } }
                ]
            }]
        }),
    );
    let wifi = source_item(
        "wifi",
        "wifi",
        json!({
            "ssid": "Lab network",
            "password": "example-password",
            "security": 2,
            "sections": []
        }),
    );
    let ssh = source_item(
        "ssh",
        "sshKey",
        json!({
            "privateKey": SYNTHETIC_SSH_PRIVATE_KEY,
            "publicKey": "untrusted mismatched source public key",
            "sections": []
        }),
    );

    let plan = import(vec![card, identity, custom, wifi, ssh]);

    assert_eq!(plan.summary.source_items, 5);
    let imported_kinds: Vec<_> = plan
        .batch
        .items
        .iter()
        .map(|item| item.content.kind().as_str())
        .collect();
    assert_eq!(
        plan.summary.other_items, 5,
        "unexpected import summary: other_items={}, notes={}, skipped={}, kinds={imported_kinds:?}",
        plan.summary.other_items, plan.summary.notes, plan.summary.skipped
    );
    assert_eq!(plan.batch.items.len(), 5);
    let SecretContent::Card(card) = content(&plan.batch.items[0]) else {
        panic!("creditCard should import as a card");
    };
    assert_eq!(card.holder, "Ada Example");
    assert_eq!(card.number, "4111-1111 1111-1111");
    assert_eq!((card.expiry_month, card.expiry_year), (12, 2032));
    assert_eq!(card.cvv, "123");
    assert_eq!(card.pin, "2468");
    assert!(card.notes.contains("visa"));

    let SecretContent::Identity(identity) = content(&plan.batch.items[1]) else {
        panic!(
            "identity should import as an identity, got {}",
            content(&plan.batch.items[1]).kind().as_str()
        );
    };
    assert_eq!(identity.first_name, "Ada");
    assert_eq!(identity.last_name, "Example");
    assert_eq!(identity.emails, ["ada@example.invalid"]);
    assert_eq!(identity.phones, ["+1 555 0100"]);
    assert_eq!(
        identity.address,
        "streetAddress: 1 Example Street\ncity: London\ncountryOrRegion: GB"
    );
    assert_eq!(identity.documents.len(), 3);
    assert!(identity.documents.iter().any(|document| {
        document.document_type.to_lowercase().contains("passport") && document.number == "P123456"
    }));
    assert_eq!(identity.notes, "source notes");

    let SecretContent::Custom(custom) = content(&plan.batch.items[2]) else {
        panic!("custom should import as custom content");
    };
    assert_custom_values(custom);
    assert_eq!(custom.notes, "source notes");
    let future = custom
        .records
        .iter()
        .find(|record| record.key == "Recovery / Future")
        .unwrap();
    assert!(future.concealed);
    assert!(future.value.contains("opaque-source-value"));

    let SecretContent::Wifi(wifi) = content(&plan.batch.items[3]) else {
        panic!("wifi should import as Wi-Fi content");
    };
    assert_eq!(wifi.ssid, "Lab network");
    assert_eq!(wifi.password, "example-password");
    assert!(wifi.security == WifiSecurity::Wpa);
    assert_eq!(wifi.notes, "source notes");

    let SecretContent::SshKey(ssh) = content(&plan.batch.items[4]) else {
        panic!("sshKey should import as an SSH key");
    };
    assert_eq!(ssh.private_key, SYNTHETIC_SSH_PRIVATE_KEY);
    assert_ne!(ssh.public_key, "untrusted mismatched source public key");
    assert!(ssh.notes.contains("untrusted mismatched source public key"));
    assert!(!ssh.notes.contains("BEGIN OPENSSH PRIVATE KEY"));
    assert!(ssh.fingerprint.starts_with("SHA256:"));
    assert!(!plan.issues.is_empty());
    assert!(
        plan.issues
            .iter()
            .any(|issue| issue.kind == IssueKind::ExtraFieldsInNotes)
    );

    let second = import(vec![source_item("identity", "identity", identity_content)]);
    let first_identity = match content(&plan.batch.items[1]) {
        SecretContent::Identity(value) => value,
        _ => unreachable!(),
    };
    let second_identity = match content(&second.batch.items[0]) {
        SecretContent::Identity(value) => value,
        _ => unreachable!(),
    };
    assert_eq!(
        first_identity.documents[0].id,
        second_identity.documents[0].id
    );
}

#[test]
fn card_identity_and_wifi_source_extras_are_retained_as_concealed_custom_records() {
    let items = vec![
        source_item(
            "card-content-extra",
            "creditCard",
            json!({
                "cardholderName": "Ada Example",
                "number": "4111111111111111",
                "verificationNumber": "123",
                "expirationDate": "2032-12",
                "futureCardField": "synthetic-card-extra"
            }),
        ),
        source_item(
            "card-object-type",
            "creditCard",
            json!({
                "cardholderName": "Ada Example",
                "number": "4111111111111111",
                "verificationNumber": "123",
                "expirationDate": "2032-12",
                "cardType": { "brand": "synthetic-card-brand" }
            }),
        ),
        source_item(
            "card-array-type",
            "creditCard",
            json!({
                "cardholderName": "Ada Example",
                "number": "4111111111111111",
                "verificationNumber": "123",
                "expirationDate": "2032-12",
                "cardType": ["synthetic-card-array"]
            }),
        ),
        with_item_data_extra(
            source_item(
                "card-item-extra",
                "creditCard",
                json!({
                    "cardholderName": "Ada Example",
                    "number": "4111111111111111",
                    "verificationNumber": "123",
                    "expirationDate": "2032-12"
                }),
            ),
            "synthetic-card-item-extra",
        ),
        source_item(
            "identity-content-extra",
            "identity",
            json!({
                "firstName": "Ada",
                "lastName": "Example",
                "futureIdentityField": "synthetic-identity-extra"
            }),
        ),
        with_item_data_extra(
            source_item(
                "identity-item-extra",
                "identity",
                json!({ "firstName": "Ada", "lastName": "Example" }),
            ),
            "synthetic-identity-item-extra",
        ),
        source_item(
            "wifi-content-extra",
            "wifi",
            json!({
                "ssid": "Lab network",
                "password": "synthetic-wifi-password",
                "security": 2,
                "futureWifiField": "synthetic-wifi-extra"
            }),
        ),
        with_item_data_extra(
            source_item(
                "wifi-item-extra",
                "wifi",
                json!({
                    "ssid": "Lab network",
                    "password": "synthetic-wifi-password",
                    "security": 2
                }),
            ),
            "synthetic-wifi-item-extra",
        ),
    ];
    let plan = import(items);

    assert_eq!(plan.summary.other_items, 8);
    assert_eq!(plan.summary.notes, 0);
    assert_eq!(plan.summary.skipped, 0);
    assert_eq!(plan.batch.items.len(), 8);
    for (index, marker) in [
        (0, "synthetic-card-extra"),
        (1, "synthetic-card-brand"),
        (2, "synthetic-card-array"),
        (4, "synthetic-identity-extra"),
        (6, "synthetic-wifi-extra"),
    ] {
        let kind = match index {
            0..=2 => "creditCard",
            4 => "identity",
            _ => "wifi",
        };
        let _ = concealed_source(&plan.batch.items[index], &format!("{kind} source"), marker);
    }
    for (index, marker, source_key, source_marker) in [
        (3, "synthetic-card-item-extra", "creditCard source", "Ada"),
        (5, "synthetic-identity-item-extra", "identity source", "Ada"),
        (7, "synthetic-wifi-item-extra", "wifi source", "Lab"),
    ] {
        let custom = concealed_source(&plan.batch.items[index], source_key, source_marker);
        let Some(extra) = custom
            .records
            .iter()
            .find(|record| record.key == "item data / providerSecret")
        else {
            panic!("item.data extra field should be added as its own record");
        };
        assert!(extra.concealed);
        assert!(extra.value.contains(marker));
        assert!(!custom.notes.contains(marker));
    }
    assert_eq!(
        plan.issues
            .iter()
            .filter(|issue| issue.kind == IssueKind::SourceFieldsPreserved)
            .count(),
        8
    );
}

fn assert_custom_values(custom: &CustomContent) {
    assert_eq!(custom.records.len(), 3);
    let hidden = custom
        .records
        .iter()
        .find(|record| record.key == "Recovery / Code")
        .unwrap();
    assert_eq!(hidden.value, "recovery-value");
    assert!(hidden.concealed);

    let memo = custom
        .records
        .iter()
        .find(|record| record.key == "Recovery / Memo")
        .unwrap();
    assert!(memo.multiline);
}

#[test]
fn invalid_card_fields_are_retained_as_a_concealed_source_reference() {
    let plan = import(vec![source_item(
        "invalid-card",
        "creditCard",
        json!({
            "cardholderName": "Ada Example",
            "number": "4111111111111111",
            "verificationNumber": "123",
            "expirationDate": "2032-13"
        }),
    )]);

    assert_eq!(plan.summary.other_items, 1);
    assert_eq!(plan.summary.notes, 0);
    let SecretContent::Custom(custom) = &plan.batch.items[0].content else {
        panic!("invalid card should be retained as a custom source reference");
    };
    assert_eq!(custom.notes, "source notes");
    assert_eq!(custom.records.len(), 1);
    let source = &custom.records[0];
    assert_eq!(source.key, "creditCard source");
    assert!(source.concealed);
    assert_eq!(source.id, "00000000-0000-4000-8000-000000000001");
    assert!(source.value.contains("4111111111111111"));
    assert!(source.value.contains("2032-13"));
    assert!(!custom.notes.contains("4111111111111111"));
    assert!(!custom.notes.contains("123"));
    assert!(
        plan.issues
            .iter()
            .any(|issue| issue.kind == IssueKind::SourceFieldsPreserved)
    );
}

#[test]
fn unsupported_wifi_security_is_retained_as_a_concealed_source_reference() {
    let plan = import(vec![source_item(
        "future-wifi",
        "wifi",
        json!({ "ssid": "Future network", "password": "secret-value", "security": 99 }),
    )]);

    assert_eq!(plan.summary.other_items, 1);
    assert_eq!(plan.summary.notes, 0);
    let SecretContent::Custom(custom) = &plan.batch.items[0].content else {
        panic!("unknown security should be retained as a custom source reference");
    };
    assert_eq!(custom.notes, "source notes");
    assert_eq!(custom.records.len(), 1);
    let source = &custom.records[0];
    assert_eq!(source.key, "wifi source");
    assert!(source.concealed);
    assert_eq!(source.id, "00000000-0000-4000-8000-000000000001");
    assert!(source.value.contains("Future network"));
    assert!(source.value.contains("secret-value"));
    assert!(!custom.notes.contains("Future network"));
    assert!(!custom.notes.contains("secret-value"));
    assert!(
        plan.issues
            .iter()
            .any(|issue| issue.kind == IssueKind::SourceFieldsPreserved)
    );
}

#[test]
fn wifi_with_sections_is_preserved_as_one_concealed_source_reference() {
    let plan = import(vec![source_item(
        "wifi-with-sections",
        "wifi",
        json!({
            "ssid": "Private network",
            "password": "synthetic-wifi-password",
            "security": 2,
            "sections": [{
                "sectionName": "Credentials",
                "sectionFields": [{
                    "fieldName": "Recovery",
                    "type": "hidden",
                    "data": { "content": "synthetic-hidden-marker" }
                }]
            }]
        }),
    )]);

    assert_eq!(plan.summary.other_items, 1);
    assert_eq!(plan.summary.notes, 0);
    let SecretContent::Custom(custom) = &plan.batch.items[0].content else {
        panic!("Wi-Fi sections should use a custom source reference");
    };
    assert_eq!(custom.notes, "source notes");
    assert_eq!(custom.records.len(), 1);
    let source = &custom.records[0];
    assert_eq!(source.key, "wifi source");
    assert_eq!(source.id, "00000000-0000-4000-8000-000000000001");
    assert!(source.concealed);
    assert!(source.value.contains("synthetic-wifi-password"));
    assert!(source.value.contains("synthetic-hidden-marker"));
    assert!(
        plan.issues
            .iter()
            .any(|issue| issue.kind == IssueKind::SourceFieldsPreserved)
    );
}

#[test]
fn proton_ssh_import_derives_metadata_and_never_copies_an_invalid_private_key_to_notes() {
    let valid = import(vec![source_item(
        "ssh",
        "sshKey",
        json!({ "privateKey": SYNTHETIC_SSH_PRIVATE_KEY }),
    )]);
    let SecretContent::SshKey(ssh) = &valid.batch.items[0].content else {
        panic!("valid OpenSSH private key should import natively");
    };
    let derived =
        SshKeyContent::import(SYNTHETIC_SSH_PRIVATE_KEY.into(), Default::default()).unwrap();
    assert_eq!(ssh.public_key, derived.public_key);
    assert_eq!(ssh.fingerprint, derived.fingerprint);
    assert_eq!(ssh.algorithm, derived.algorithm);
    assert_eq!(ssh.notes, "source notes");

    let invalid = import(vec![source_item(
        "ssh-invalid",
        "sshKey",
        json!({ "privateKey": "not an OpenSSH private key" }),
    )]);
    assert_eq!(invalid.summary.other_items, 0);
    assert_eq!(invalid.summary.skipped, 1);
    assert!(invalid.batch.items.is_empty());
    assert!(
        invalid
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::UnsupportedItem)
    );
}

#[test]
fn ssh_with_sections_content_extras_or_item_extras_is_skipped() {
    let with_sections = source_item(
        "ssh-with-sections",
        "sshKey",
        json!({
            "privateKey": SYNTHETIC_SSH_PRIVATE_KEY,
            "sections": [{ "sectionName": "Metadata", "sectionFields": [] }]
        }),
    );
    let content_extra = source_item(
        "ssh-with-content-extra",
        "sshKey",
        json!({
            "privateKey": SYNTHETIC_SSH_PRIVATE_KEY,
            "futurePrivateMetadata": "synthetic-content-extra"
        }),
    );
    let mut item_data_extra = source_item(
        "ssh-with-item-extra",
        "sshKey",
        json!({ "privateKey": SYNTHETIC_SSH_PRIVATE_KEY }),
    );
    item_data_extra["data"]["extraFields"] = json!({
        "futureMetadata": "synthetic-item-extra"
    });

    let plan = import(vec![with_sections, content_extra, item_data_extra]);
    assert_eq!(plan.summary.source_items, 3);
    assert_eq!(
        plan.summary.other_items,
        0,
        "unexpected native SSH items: {:?}",
        plan.batch
            .items
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(plan.summary.skipped, 3);
    assert!(plan.batch.items.is_empty());
    assert_eq!(
        plan.issues
            .iter()
            .filter(|issue| issue.kind == IssueKind::UnsupportedItem)
            .count(),
        3
    );
}

#[test]
fn unknown_login_fields_are_reported_and_skipped_instead_of_disappearing_silently() {
    let plan = import(vec![source_item(
        "login-with-future-field",
        "login",
        json!({
            "username": "ada",
            "password": "synthetic-password",
            "futureCredential": "synthetic-future-value"
        }),
    )]);

    assert_eq!(plan.summary.source_items, 1);
    assert_eq!(plan.summary.logins, 0);
    assert_eq!(plan.summary.skipped, 1);
    assert!(plan.batch.items.is_empty());
    assert!(
        plan.issues
            .iter()
            .any(|issue| issue.kind == IssueKind::UnsupportedItem)
    );
}
