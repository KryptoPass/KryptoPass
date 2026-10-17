#![allow(clippy::unwrap_used)]

use kryptopass_import::{
    CustomEntry, Entry, EntryContent, EntryField, EntryLocation, EntryType, EntryWarningKind,
    ImportLimits, ImportedEntries, Importer, OneTimePassword, WifiSecurity,
    importers::proton::ProtonPassImporter,
};
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

fn import(items: Vec<Value>) -> ImportedEntries {
    ProtonPassImporter::new()
        .import(&export(items), &ImportLimits::default())
        .unwrap()
}

fn content(item: &Entry) -> &EntryContent {
    &item.content
}

fn with_item_data_extra(mut item: Value, value: &str) -> Value {
    item["data"]["extraFields"] = json!({ "providerSecret": value });
    item
}

fn concealed_source<'a>(item: &'a Entry, label: &str, marker: &str) -> &'a CustomEntry {
    let EntryContent::Custom(custom) = content(item) else {
        panic!(
            "{} should be preserved as custom content",
            item.title.as_str()
        );
    };
    assert_eq!(custom.notes.as_str(), "source notes");
    let source = custom
        .fields
        .iter()
        .find(|field| field.label == label)
        .unwrap_or_else(|| panic!("missing concealed source field {label}"));
    assert!(source.concealed);
    assert!(source.value.contains(marker));
    assert!(!custom.notes.contains(marker));
    custom
}

fn concealed_additional_field<'a>(item: &'a Entry, label: &str) -> &'a EntryField {
    let field = item
        .additional_fields
        .iter()
        .find(|field| field.label == label)
        .unwrap_or_else(|| panic!("missing concealed additional field {label}"));
    assert!(field.concealed);
    field
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
    let identity = source_item("identity", "identity", identity_content);
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

    assert_eq!(plan.statistics().source_entries, 5);
    assert_eq!(plan.statistics().imported_entries, 5);
    assert_eq!(plan.statistics().skipped_entries, 0);
    let imported_kinds: Vec<_> = plan
        .entries()
        .iter()
        .map(|item| item.content.entry_type())
        .collect();
    assert_eq!(
        imported_kinds,
        [
            EntryType::Card,
            EntryType::Identity,
            EntryType::Custom,
            EntryType::Wifi,
            EntryType::SshKey,
        ]
    );
    for kind in imported_kinds {
        assert_eq!(plan.statistics().entries_by_type.get(&kind), Some(&1));
    }
    assert_eq!(plan.entries().len(), 5);
    let EntryContent::Card(card) = content(&plan.entries()[0]) else {
        panic!("creditCard should import as a card");
    };
    assert_eq!(card.holder.as_str(), "Ada Example");
    assert_eq!(card.number.as_str(), "4111-1111 1111-1111");
    assert_eq!((card.expiry_month, card.expiry_year), (12, 2032));
    assert_eq!(card.cvv.as_str(), "123");
    assert_eq!(card.pin.as_str(), "2468");
    assert_eq!(card.notes.as_str(), "source notes");
    let card_type = concealed_additional_field(&plan.entries()[0], "Proton Pass card type");
    assert_eq!(
        serde_json::from_str::<Value>(card_type.value.as_str()).unwrap(),
        json!("visa")
    );

    let EntryContent::Identity(identity) = content(&plan.entries()[1]) else {
        panic!(
            "identity should import as an identity, got {:?}",
            content(&plan.entries()[1]).entry_type()
        );
    };
    assert_eq!(identity.first_name.as_str(), "Ada");
    assert_eq!(identity.last_name.as_str(), "Example");
    assert_eq!(
        identity
            .emails
            .iter()
            .map(|email| email.as_str())
            .collect::<Vec<_>>(),
        ["ada@example.invalid"]
    );
    assert_eq!(
        identity
            .phones
            .iter()
            .map(|phone| phone.as_str())
            .collect::<Vec<_>>(),
        ["+1 555 0100"]
    );
    assert_eq!(
        identity.address.as_str(),
        "streetAddress: 1 Example Street\ncity: London\ncountryOrRegion: GB"
    );
    let documents: Vec<_> = identity
        .documents
        .iter()
        .map(|document| {
            (
                document.document_type.as_str(),
                document.number.as_str(),
                document.country.as_str(),
                document.expires.as_str(),
            )
        })
        .collect();
    assert_eq!(
        documents,
        [
            ("socialSecurityNumber", "123-45-6789", "GB", ""),
            ("passportNumber", "P123456", "GB", ""),
            ("licenseNumber", "D1234567", "GB", ""),
        ]
    );
    assert_eq!(identity.notes.as_str(), "source notes");

    let EntryContent::Custom(custom) = content(&plan.entries()[2]) else {
        panic!("custom should import as custom content");
    };
    assert_custom_values(custom);
    assert_eq!(custom.notes.as_str(), "source notes");
    let future = custom
        .fields
        .iter()
        .find(|field| field.label == "Recovery / Future")
        .unwrap();
    assert!(future.concealed);
    assert_eq!(
        serde_json::from_str::<Value>(future.value.as_str()).unwrap(),
        json!({ "secret": "opaque-source-value" })
    );

    let EntryContent::Wifi(wifi) = content(&plan.entries()[3]) else {
        panic!("wifi should import as Wi-Fi content");
    };
    assert_eq!(wifi.ssid.as_str(), "Lab network");
    assert_eq!(wifi.password.as_str(), "example-password");
    assert_eq!(wifi.security, WifiSecurity::Wpa);
    assert_eq!(wifi.notes.as_str(), "source notes");

    let EntryContent::SshKey(ssh) = content(&plan.entries()[4]) else {
        panic!("sshKey should import as an SSH key");
    };
    assert_eq!(ssh.private_key.as_str(), SYNTHETIC_SSH_PRIVATE_KEY);
    assert_ne!(
        ssh.public_key.as_str(),
        "untrusted mismatched source public key"
    );
    assert_eq!(ssh.notes.as_str(), "source notes");
    let public_key =
        concealed_additional_field(&plan.entries()[4], "Proton Pass source public key");
    assert_eq!(
        public_key.value.as_str(),
        "untrusted mismatched source public key"
    );
    assert!(ssh.fingerprint.starts_with("SHA256:"));
    assert_eq!(
        plan.warnings()
            .iter()
            .map(|warning| (warning.location.entry, warning.kind))
            .collect::<Vec<_>>(),
        [
            (Some(0), EntryWarningKind::SourceFieldsPreserved),
            (Some(4), EntryWarningKind::ConflictingPublicKey),
        ]
    );
}

#[test]
fn card_identity_and_wifi_source_extras_are_retained_as_concealed_fields() {
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

    assert_eq!(plan.statistics().source_entries, 8);
    assert_eq!(plan.statistics().imported_entries, 8);
    assert_eq!(
        plan.statistics().entries_by_type.get(&EntryType::Custom),
        Some(&8)
    );
    assert_eq!(plan.statistics().skipped_entries, 0);
    assert_eq!(plan.entries().len(), 8);
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
        let _ = concealed_source(&plan.entries()[index], &format!("{kind} source"), marker);
    }
    for (index, marker, source_key, source_marker) in [
        (3, "synthetic-card-item-extra", "creditCard source", "Ada"),
        (5, "synthetic-identity-item-extra", "identity source", "Ada"),
        (7, "synthetic-wifi-item-extra", "wifi source", "Lab"),
    ] {
        let item = &plan.entries()[index];
        let custom = concealed_source(item, source_key, source_marker);
        assert_eq!(custom.fields.len(), 1);
        let extra = concealed_additional_field(item, "Proton Pass item data / providerSecret");
        assert_eq!(
            serde_json::from_str::<Value>(extra.value.as_str()).unwrap(),
            json!(marker)
        );
        assert!(!custom.notes.contains(marker));
    }
    assert_eq!(
        plan.warnings()
            .iter()
            .filter(|warning| warning.kind == EntryWarningKind::SourceFieldsPreserved)
            .count(),
        8
    );
}

fn assert_custom_values(custom: &CustomEntry) {
    assert_eq!(custom.fields.len(), 3);
    let hidden = custom
        .fields
        .iter()
        .find(|field| field.label == "Recovery / Code")
        .unwrap();
    assert_eq!(hidden.value.as_str(), "recovery-value");
    assert!(hidden.concealed);

    let memo = custom
        .fields
        .iter()
        .find(|field| field.label == "Recovery / Memo")
        .unwrap();
    assert_eq!(memo.value.as_str(), "line one\nline two");
    assert!(!memo.concealed);
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

    assert_eq!(plan.statistics().imported_entries, 1);
    assert_eq!(plan.statistics().skipped_entries, 0);
    let EntryContent::Custom(custom) = &plan.entries()[0].content else {
        panic!("invalid card should be retained as a custom source reference");
    };
    assert_eq!(custom.notes.as_str(), "source notes");
    assert_eq!(custom.fields.len(), 1);
    let source = &custom.fields[0];
    assert_eq!(source.label.as_str(), "creditCard source");
    assert!(source.concealed);
    assert!(source.value.contains("4111111111111111"));
    assert!(source.value.contains("2032-13"));
    assert!(!custom.notes.contains("4111111111111111"));
    assert!(!custom.notes.contains("123"));
    assert!(
        plan.warnings()
            .iter()
            .any(|warning| warning.kind == EntryWarningKind::SourceFieldsPreserved)
    );
}

#[test]
fn unsupported_wifi_security_is_retained_as_a_concealed_source_reference() {
    let plan = import(vec![source_item(
        "future-wifi",
        "wifi",
        json!({ "ssid": "Future network", "password": "secret-value", "security": 99 }),
    )]);

    assert_eq!(plan.statistics().imported_entries, 1);
    assert_eq!(plan.statistics().skipped_entries, 0);
    let EntryContent::Custom(custom) = &plan.entries()[0].content else {
        panic!("unknown security should be retained as a custom source reference");
    };
    assert_eq!(custom.notes.as_str(), "source notes");
    assert_eq!(custom.fields.len(), 1);
    let source = &custom.fields[0];
    assert_eq!(source.label.as_str(), "wifi source");
    assert!(source.concealed);
    assert_eq!(
        serde_json::from_str::<Value>(source.value.as_str()).unwrap(),
        json!({ "ssid": "Future network", "password": "secret-value", "security": 99 })
    );
    assert!(!custom.notes.contains("Future network"));
    assert!(!custom.notes.contains("secret-value"));
    assert!(
        plan.warnings()
            .iter()
            .any(|warning| warning.kind == EntryWarningKind::SourceFieldsPreserved)
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

    assert_eq!(plan.statistics().imported_entries, 1);
    assert_eq!(plan.statistics().skipped_entries, 0);
    let EntryContent::Custom(custom) = &plan.entries()[0].content else {
        panic!("Wi-Fi sections should use a custom source reference");
    };
    assert_eq!(custom.notes.as_str(), "source notes");
    assert_eq!(custom.fields.len(), 1);
    let source = &custom.fields[0];
    assert_eq!(source.label.as_str(), "wifi source");
    assert!(source.concealed);
    assert!(source.value.contains("synthetic-wifi-password"));
    assert!(source.value.contains("synthetic-hidden-marker"));
    assert!(
        plan.warnings()
            .iter()
            .any(|warning| warning.kind == EntryWarningKind::SourceFieldsPreserved)
    );
}

#[test]
fn proton_ssh_import_derives_metadata_and_never_copies_an_invalid_private_key_to_notes() {
    let valid = import(vec![source_item(
        "ssh",
        "sshKey",
        json!({ "privateKey": SYNTHETIC_SSH_PRIVATE_KEY }),
    )]);
    let EntryContent::SshKey(ssh) = &valid.entries()[0].content else {
        panic!("valid OpenSSH private key should import natively");
    };
    let derived = ssh_key::PrivateKey::from_openssh(SYNTHETIC_SSH_PRIVATE_KEY).unwrap();
    assert_eq!(ssh.private_key.as_str(), SYNTHETIC_SSH_PRIVATE_KEY);
    assert_eq!(
        ssh.public_key.as_str(),
        derived.public_key().to_openssh().unwrap()
    );
    assert_eq!(
        ssh.fingerprint.as_str(),
        derived.fingerprint(ssh_key::HashAlg::Sha256).to_string()
    );
    assert_eq!(ssh.algorithm.as_str(), derived.algorithm().to_string());
    assert_eq!(ssh.user.as_str(), "synthetic import fixture");
    assert_eq!(ssh.notes.as_str(), "source notes");
    assert!(valid.entries()[0].additional_fields.is_empty());
    assert!(valid.warnings().is_empty());

    let invalid = import(vec![source_item(
        "ssh-invalid",
        "sshKey",
        json!({ "privateKey": "not an OpenSSH private key" }),
    )]);
    assert_eq!(invalid.statistics().imported_entries, 0);
    assert_eq!(invalid.statistics().skipped_entries, 1);
    assert!(invalid.entries().is_empty());
    assert!(
        invalid
            .warnings()
            .iter()
            .any(|warning| warning.kind == EntryWarningKind::UnreadableSshKey)
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
    assert_eq!(plan.statistics().source_entries, 3);
    assert_eq!(
        plan.statistics().imported_entries,
        0,
        "unexpected native SSH items: {:?}",
        plan.entries()
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(plan.statistics().skipped_entries, 3);
    assert!(plan.entries().is_empty());
    assert_eq!(
        plan.warnings()
            .iter()
            .filter(|warning| warning.kind == EntryWarningKind::UnreadableSshKey)
            .count(),
        3
    );
}

#[test]
fn unknown_login_fields_are_preserved_as_concealed_additional_fields() {
    let plan = import(vec![source_item(
        "login-with-future-field",
        "login",
        json!({
            "username": "ada",
            "password": "synthetic-password",
            "futureCredential": "synthetic-future-value"
        }),
    )]);

    assert_eq!(plan.statistics().source_entries, 1);
    assert_eq!(plan.statistics().imported_entries, 1);
    assert_eq!(
        plan.statistics().entries_by_type.get(&EntryType::Login),
        Some(&1)
    );
    assert_eq!(plan.statistics().skipped_entries, 0);
    assert_eq!(plan.entries().len(), 1);
    let item = &plan.entries()[0];
    let EntryContent::Login(login) = &item.content else {
        panic!("unknown login fields should not discard the login");
    };
    assert_eq!(login.username.as_str(), "ada");
    assert!(login.email.is_empty());
    assert_eq!(login.password.as_str(), "synthetic-password");
    assert_eq!(login.notes.as_str(), "source notes");
    assert_eq!(item.created_at, None);
    assert_eq!(item.modified_at, None);
    assert_eq!(item.additional_fields.len(), 1);
    let future = concealed_additional_field(item, "Proton Pass login / futureCredential");
    assert_eq!(
        serde_json::from_str::<Value>(future.value.as_str()).unwrap(),
        json!("synthetic-future-value")
    );
    assert_eq!(plan.warnings().len(), 1);
    assert_eq!(
        plan.warnings()[0].kind,
        EntryWarningKind::SourceFieldsPreserved
    );
    assert_eq!(
        plan.warnings()[0].location,
        EntryLocation {
            collection: 0,
            entry: Some(0)
        }
    );
}

#[test]
fn proton_login_preserves_separate_email_timestamps_and_otp_source_values() {
    for (username, otp) in [
        (
            "  ada  ",
            "otpauth://totp/Synthetic?secret=synthetic-source&digits=9",
        ),
        ("", "synthetic-base32-source"),
    ] {
        let mut source = with_item_data_extra(
            source_item(
                "login-source-data",
                "login",
                json!({
                    "itemUsername": username,
                    "itemEmail": "ada@example.invalid",
                    "password": " synthetic-password ",
                    "urls": ["https://example.invalid/account", "synthetic source URL"],
                    "totpUri": otp,
                    "passkeys": [{ "credentialId": "synthetic-passkey-reference" }],
                    "autofillUrls": [{ "url": "synthetic autofill rule" }]
                }),
            ),
            "synthetic-item-extra",
        );
        source["createTime"] = json!(1_700_000_123u64);
        source["modifyTime"] = json!(1_700_000_987u64);
        source["pinned"] = json!(true);
        source["state"] = json!(2);

        let imported = import(vec![source]);
        assert_eq!(imported.statistics().source_entries, 1);
        assert_eq!(imported.statistics().imported_entries, 1);
        assert_eq!(imported.statistics().skipped_entries, 0);
        assert_eq!(imported.statistics().trashed_entries, 1);
        assert_eq!(imported.entries().len(), 1);
        let entry = &imported.entries()[0];
        assert_eq!(entry.title.as_str(), "login-source-data");
        assert_eq!(entry.created_at, Some(1_700_000_123));
        assert_eq!(entry.modified_at, Some(1_700_000_987));
        assert!(entry.favorite);
        assert!(entry.trashed);
        let EntryContent::Login(login) = &entry.content else {
            panic!("login source data should import without destination policies");
        };
        assert_eq!(login.username.as_str(), username);
        assert_eq!(login.email.as_str(), "ada@example.invalid");
        assert_eq!(login.password.as_str(), " synthetic-password ");
        assert_eq!(login.notes.as_str(), "source notes");
        assert_eq!(
            login
                .urls
                .iter()
                .map(|url| url.as_str())
                .collect::<Vec<_>>(),
            ["https://example.invalid/account", "synthetic source URL"]
        );
        match (&login.otp, otp.starts_with("otpauth://")) {
            (Some(OneTimePassword::Uri(value)), true)
            | (Some(OneTimePassword::Base32(value)), false) => {
                assert_eq!(value.as_str(), otp);
            }
            _ => panic!("OTP source value and representation should be preserved"),
        }
        assert_eq!(entry.additional_fields.len(), 3);
        for (label, expected) in [
            (
                "Proton Pass passkeys",
                json!([{ "credentialId": "synthetic-passkey-reference" }]),
            ),
            (
                "Proton Pass autofill rules",
                json!([{ "url": "synthetic autofill rule" }]),
            ),
            (
                "Proton Pass item data / providerSecret",
                json!("synthetic-item-extra"),
            ),
        ] {
            let field = concealed_additional_field(entry, label);
            assert_eq!(
                serde_json::from_str::<Value>(field.value.as_str()).unwrap(),
                expected
            );
        }
        assert_eq!(
            imported
                .warnings()
                .iter()
                .map(|warning| warning.kind)
                .collect::<Vec<_>>(),
            [
                EntryWarningKind::PasskeysAsReferences,
                EntryWarningKind::AutofillRulesAsReferences,
                EntryWarningKind::SourceFieldsPreserved,
            ]
        );
    }
}
