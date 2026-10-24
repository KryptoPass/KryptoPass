#![allow(clippy::unwrap_used)]

use kryptopass_core::{
    CONTENT_SCHEMA_VERSION, CardContent, CustomContent, CustomRecord, IdentityContent,
    IdentityDocument, LicenseContent, SecretContent, SshKeyContent, TotpContent, WifiContent,
    WifiSecurity, decode_content, encode_content, parse_totp_input,
};
use serde_json::Value;
use ssh_key::{LineEnding, PrivateKey, private::Ed25519Keypair};
use uuid::Uuid;

fn sample_totp() -> kryptopass_core::TotpContent {
    let config = parse_totp_input(
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&issuer=Example&algorithm=SHA256&digits=8&period=86400&image=https%3A%2F%2Fcdn.example.invalid%2Ficon.png&vendor=one%2Btwo",
    )
    .unwrap()
    .unwrap();
    TotpContent {
        config,
        notes: "Authenticator notes".into(),
    }
}

fn sample_ssh_key() -> SshKeyContent {
    let keypair = Ed25519Keypair::from_seed(&[7; 32]);
    let key = PrivateKey::new(keypair.into(), "synthetic test key").unwrap();
    let private_key = key.to_openssh(LineEnding::LF).unwrap().to_string();
    let mut content = SshKeyContent::import(private_key.into(), Default::default()).unwrap();
    content.user = "ada".into();
    content.hosts = vec!["example.invalid".into()];
    content.notes = "SSH notes".into();
    content
}

fn sample_contents() -> Vec<(&'static str, SecretContent)> {
    vec![
        (
            "custom",
            SecretContent::Custom(CustomContent {
                records: vec![CustomRecord {
                    id: Uuid::from_u128(1).to_string().into(),
                    key: "Recovery code".into(),
                    value: "  value with spaces\nand a newline  ".into(),
                    multiline: true,
                    concealed: true,
                }],
                notes: "Custom notes".into(),
            }),
        ),
        (
            "card",
            SecretContent::Card(CardContent {
                holder: "Ada Example".into(),
                number: "4111-1111 1111-1111".into(),
                expiry_month: 12,
                expiry_year: 2032,
                cvv: "123".into(),
                pin: "2468".into(),
                notes: "Card notes".into(),
            }),
        ),
        ("totp", SecretContent::Totp(sample_totp())),
        (
            "identity",
            SecretContent::Identity(IdentityContent {
                first_name: "Ada".into(),
                last_name: "Example".into(),
                documents: vec![IdentityDocument {
                    id: Uuid::from_u128(2).to_string().into(),
                    document_type: "Passport".into(),
                    number: "P123456".into(),
                    country: "GB".into(),
                    expires: "2032-12-31".into(),
                }],
                emails: vec!["ada@example.invalid".into()],
                phones: vec!["+1 555 0100".into()],
                address: "1 Example Street".into(),
                notes: "Identity notes".into(),
            }),
        ),
        (
            "wifi",
            SecretContent::Wifi(WifiContent {
                ssid: "Example network".into(),
                security: WifiSecurity::Wpa3,
                password: "example-network-password".into(),
                hidden: true,
                notes: "Wi-Fi notes".into(),
            }),
        ),
        (
            "license",
            SecretContent::License(LicenseContent {
                product: "Example Product".into(),
                license_key: "AAAA-BBBB-CCCC-DDDD".into(),
                holder: "Ada Example".into(),
                email: "ada@example.invalid".into(),
                version: "2.1".into(),
                purchased: "2025-01-02".into(),
                expires: "2030-01-02".into(),
                notes: "License notes".into(),
            }),
        ),
        ("ssh-key", SecretContent::SshKey(sample_ssh_key())),
    ]
}

fn assert_rejected(content: &SecretContent, context: &str) {
    assert!(
        encode_content(content).is_err(),
        "accepted invalid content: {context}"
    );
}

#[test]
fn every_expanded_content_kind_roundtrips_in_the_current_envelope() {
    for (kind, content) in sample_contents() {
        let encoded = encode_content(&content).unwrap();
        let envelope: Value = serde_json::from_slice(encoded.as_slice()).unwrap();
        assert_eq!(envelope["kind"], kind);
        assert_eq!(envelope["schemaVersion"], CONTENT_SCHEMA_VERSION);

        let decoded = decode_content(encoded.as_slice()).unwrap();
        let reencoded = encode_content(&decoded).unwrap();
        let actual: Value = serde_json::from_slice(reencoded.as_slice()).unwrap();
        assert_eq!(actual, envelope, "round-trip changed the {kind} payload");
    }
}

#[test]
fn expanded_variants_reject_missing_and_unknown_fields() {
    for (kind, content) in sample_contents() {
        let bytes = encode_content(&content).unwrap();
        let envelope: Value = serde_json::from_slice(bytes.as_slice()).unwrap();

        let mut unknown = envelope.clone();
        unknown["fields"]["unexpected"] = Value::Null;
        assert!(
            decode_content(&serde_json::to_vec(&unknown).unwrap()).is_err(),
            "accepted an unknown field for {kind}"
        );

        for field in envelope["fields"].as_object().unwrap().keys() {
            let mut missing = envelope.clone();
            missing["fields"].as_object_mut().unwrap().remove(field);
            assert!(
                decode_content(&serde_json::to_vec(&missing).unwrap()).is_err(),
                "accepted missing {kind}.{field}"
            );
        }

        for unsupported_version in [0, CONTENT_SCHEMA_VERSION + 1] {
            let mut unsupported = envelope.clone();
            unsupported["schemaVersion"] = unsupported_version.into();
            assert!(
                decode_content(&serde_json::to_vec(&unsupported).unwrap()).is_err(),
                "accepted {kind} schema version {unsupported_version}"
            );
        }
    }
}

#[test]
fn custom_records_preserve_duplicate_keys_and_exact_values_but_enforce_ids_and_limits() {
    let mut content = CustomContent {
        records: vec![
            CustomRecord {
                id: Uuid::from_u128(10).to_string().into(),
                key: "Repeated key".into(),
                value: " first\nvalue ".into(),
                multiline: true,
                concealed: true,
            },
            CustomRecord {
                id: Uuid::from_u128(11).to_string().into(),
                key: "Repeated key".into(),
                value: "second value".into(),
                multiline: false,
                concealed: false,
            },
        ],
        notes: Default::default(),
    };
    let encoded = encode_content(&SecretContent::Custom(content.clone())).unwrap();
    let decoded = decode_content(encoded.as_slice()).unwrap();
    let SecretContent::Custom(decoded) = &decoded else {
        panic!("expected custom content");
    };
    assert_eq!(decoded.records[0].key, decoded.records[1].key);
    assert_eq!(decoded.records[0].value, " first\nvalue ");
    assert!(decoded.records[0].concealed);

    content.records[1].id = content.records[0].id.clone();
    assert_rejected(
        &SecretContent::Custom(content.clone()),
        "duplicate custom IDs",
    );
    content.records[1].id = Uuid::from_u128(11).to_string().into();
    content.records[0].id = Uuid::nil().to_string().into();
    assert_rejected(&SecretContent::Custom(content.clone()), "nil custom ID");
    content.records[0].id = Uuid::from_u128(10).to_string().into();
    content.records[0].key.clear();
    assert_rejected(&SecretContent::Custom(content.clone()), "empty custom key");

    content.records[0].key = "k".repeat(512).into();
    content.records[0].key.push('k');
    assert_rejected(
        &SecretContent::Custom(content.clone()),
        "custom key longer than 512 bytes",
    );
    let key_len = content.records[0].key.len();
    content.records[0].key.truncate(key_len - 1);
    content.records.resize_with(100, CustomRecord::default);
    for (index, record) in content.records.iter_mut().enumerate() {
        record.id = Uuid::from_u128(index as u128 + 20).to_string().into();
        if record.key.is_empty() {
            record.key = "field".into();
        }
    }
    content.records[0].value = "v".repeat(256 * 1024).into();
    content.notes = "n".repeat(256 * 1024).into();
    assert!(encode_content(&SecretContent::Custom(content.clone())).is_ok());
    content.records[0].value.push('v');
    assert_rejected(
        &SecretContent::Custom(content.clone()),
        "custom value longer than 256 KiB",
    );
    let value_len = content.records[0].value.len();
    content.records[0].value.truncate(value_len - 1);
    content.notes.push('n');
    assert_rejected(
        &SecretContent::Custom(content.clone()),
        "custom notes longer than 256 KiB",
    );
    content.notes.truncate(content.notes.len() - 1);
    let mut extra_record = CustomRecord::default();
    extra_record.id = Uuid::from_u128(500).to_string().into();
    extra_record.key = "extra".into();
    content.records.push(extra_record);
    assert_rejected(
        &SecretContent::Custom(content),
        "more than 100 custom records",
    );
}

#[test]
fn card_validation_accepts_supported_boundaries_and_rejects_invalid_digits_and_dates() {
    let mut card = CardContent {
        holder: Default::default(),
        number: "1234 5678-9012".into(),
        expiry_month: 1,
        expiry_year: 1900,
        cvv: "123".into(),
        pin: Default::default(),
        notes: Default::default(),
    };
    assert!(encode_content(&SecretContent::Card(card.clone())).is_ok());
    assert_eq!(card.normalized_number().as_str(), "123456789012");
    card.number = "1234567890123456789".into();
    card.expiry_year = 9999;
    card.cvv = "1234".into();
    assert!(encode_content(&SecretContent::Card(card.clone())).is_ok());
    card.number = "123456x890123".into();
    assert_rejected(
        &SecretContent::Card(card.clone()),
        "non-digit card separator",
    );
    card.number = "123456789012".into();
    card.cvv = "1234".into();

    card.expiry_month = 0;
    assert_rejected(&SecretContent::Card(card.clone()), "card month zero");
    card.expiry_month = 13;
    assert_rejected(&SecretContent::Card(card.clone()), "card month 13");
    card.expiry_month = 12;
    card.expiry_year = 1899;
    assert_rejected(
        &SecretContent::Card(card.clone()),
        "card year before supported range",
    );
    card.expiry_year = 10_000;
    assert_rejected(
        &SecretContent::Card(card.clone()),
        "card year after supported range",
    );
    card.expiry_year = 9999;
    card.number = "12345678901".into();
    assert_rejected(&SecretContent::Card(card.clone()), "short card number");
    card.number = "12345678901234567890".into();
    assert_rejected(&SecretContent::Card(card.clone()), "long card number");
    card.number = "123456789012".into();
    card.cvv = "12x".into();
    assert_rejected(
        &SecretContent::Card(card.clone()),
        "non-numeric card security code",
    );
    card.cvv.clear();
    card.pin = "12x".into();
    assert_rejected(&SecretContent::Card(card), "non-numeric card PIN");
}

#[test]
fn identity_document_and_contact_collections_enforce_stable_unique_ids_and_limits() {
    let mut identity = IdentityContent::default();
    identity.documents.push(IdentityDocument {
        id: Uuid::from_u128(30).to_string().into(),
        document_type: "Passport".into(),
        number: "P123456".into(),
        country: "GB".into(),
        expires: "not-normalized date text".into(),
    });
    identity.emails = vec!["ada@example.invalid".into()];
    assert!(encode_content(&SecretContent::Identity(identity.clone())).is_ok());

    let duplicate = identity.documents[0].clone();
    identity.documents.push(duplicate);
    assert_rejected(
        &SecretContent::Identity(identity.clone()),
        "duplicate identity document ID",
    );
    identity.documents.pop();
    identity.documents[0].id = Uuid::nil().to_string().into();
    assert_rejected(
        &SecretContent::Identity(identity.clone()),
        "nil identity document ID",
    );
    identity.documents[0].id = Uuid::from_u128(30).to_string().into();
    identity.documents = (0..100)
        .map(|index| {
            let mut document = IdentityDocument::default();
            document.id = Uuid::from_u128(index + 100).to_string().into();
            document
        })
        .collect();
    assert!(encode_content(&SecretContent::Identity(identity.clone())).is_ok());
    identity.emails = vec!["person@example.invalid".into(); 100];
    assert!(encode_content(&SecretContent::Identity(identity.clone())).is_ok());
    identity.phones = vec!["+1 555 0100".into(); 101];
    assert_rejected(
        &SecretContent::Identity(identity.clone()),
        "more than 100 identity phones",
    );
    identity.phones.clear();
    let mut extra_document = IdentityDocument::default();
    extra_document.id = Uuid::from_u128(300).to_string().into();
    identity.documents.push(extra_document);
    assert_rejected(
        &SecretContent::Identity(identity.clone()),
        "more than 100 documents",
    );
    identity.documents.clear();
    identity.emails = vec!["person@example.invalid".into(); 101];
    assert_rejected(&SecretContent::Identity(identity), "more than 100 emails");
}

#[test]
fn wifi_credentials_enforce_security_specific_passwords_and_escape_qr_fields() {
    let mut wifi = WifiContent {
        ssid: "office;\"wifi".into(),
        security: WifiSecurity::Wpa,
        password: "password;with,delimiters".into(),
        hidden: true,
        notes: Default::default(),
    };
    assert!(encode_content(&SecretContent::Wifi(wifi.clone())).is_ok());
    assert_eq!(
        kryptopass_core::wifi_qr_payload(&wifi).unwrap().as_str(),
        "WIFI:T:WPA;S:office\\;\\\"wifi;P:password\\;with\\,delimiters;H:true;;"
    );
    wifi.ssid = "office:network\\".into();
    wifi.password = "password:with\\slash".into();
    let escaped = kryptopass_core::wifi_qr_payload(&wifi).unwrap();
    assert!(escaped.contains("S:office\\:network\\\\;"));
    assert!(escaped.contains("P:password\\:with\\\\slash;"));

    wifi.security = WifiSecurity::Open;
    wifi.password.clear();
    assert!(encode_content(&SecretContent::Wifi(wifi.clone())).is_ok());
    assert!(
        kryptopass_core::wifi_qr_payload(&wifi)
            .unwrap()
            .contains("T:nopass;S:")
    );
    wifi.password = "unexpected".into();
    assert_rejected(&SecretContent::Wifi(wifi.clone()), "open Wi-Fi password");
    wifi.password.clear();
    wifi.ssid = "".into();
    assert_rejected(&SecretContent::Wifi(wifi.clone()), "empty Wi-Fi SSID");
    wifi.ssid = "n".repeat(33).into();
    assert_rejected(&SecretContent::Wifi(wifi.clone()), "overlong Wi-Fi SSID");
    wifi.ssid = "s".repeat(32).into();
    wifi.security = WifiSecurity::Wpa;
    wifi.password = "f".repeat(64).into();
    assert!(encode_content(&SecretContent::Wifi(wifi.clone())).is_ok());
    wifi.password = format!("{}g", "f".repeat(63)).into();
    assert_rejected(
        &SecretContent::Wifi(wifi.clone()),
        "non-hex 64-byte WPA key",
    );

    wifi.ssid = "WEP".into();
    wifi.security = WifiSecurity::Wep;
    wifi.password = "abcde".into();
    assert!(encode_content(&SecretContent::Wifi(wifi.clone())).is_ok());
    wifi.password = "abc".into();
    assert_rejected(&SecretContent::Wifi(wifi.clone()), "short WEP password");

    wifi.security = WifiSecurity::Wpa3;
    wifi.password = "a".repeat(8).into();
    assert!(encode_content(&SecretContent::Wifi(wifi.clone())).is_ok());
    assert!(kryptopass_core::wifi_qr_payload(&wifi).is_err());
    wifi.security = WifiSecurity::Wpa;
    assert!(kryptopass_core::wifi_qr_payload(&wifi).is_ok());
    wifi.password = "a".repeat(7).into();
    assert_rejected(&SecretContent::Wifi(wifi), "short WPA3 password");
}

#[test]
fn license_required_fields_and_size_limits_are_enforced() {
    let mut license = LicenseContent::default();
    license.product = "Product".into();
    license.license_key = "license-key".into();
    assert!(encode_content(&SecretContent::License(license.clone())).is_ok());
    license.product.clear();
    assert_rejected(
        &SecretContent::License(license.clone()),
        "empty license product",
    );
    license.product = "Product".into();
    license.license_key.clear();
    assert_rejected(
        &SecretContent::License(license.clone()),
        "empty license key",
    );
    license.license_key = "license-key".into();
    license.product = "p".repeat(16 * 1024).into();
    license.notes = "n".repeat(256 * 1024).into();
    assert!(encode_content(&SecretContent::License(license.clone())).is_ok());
    license.notes.push('n');
    assert_rejected(
        &SecretContent::License(license.clone()),
        "license notes longer than 256 KiB",
    );
    license.notes.truncate(license.notes.len() - 1);
    license.product = "p".repeat(16 * 1024 + 1).into();
    assert_rejected(
        &SecretContent::License(license),
        "oversized license product",
    );
}

#[test]
fn ssh_private_key_metadata_must_match_a_synthetic_in_memory_key() {
    let mut ssh = sample_ssh_key();
    assert!(encode_content(&SecretContent::SshKey(ssh.clone())).is_ok());
    assert!(
        SshKeyContent::import(ssh.private_key.clone(), "unexpected passphrase".into()).is_err()
    );

    ssh.public_key.push('x');
    assert_rejected(
        &SecretContent::SshKey(ssh.clone()),
        "mismatched SSH public key",
    );
    ssh = sample_ssh_key();
    ssh.fingerprint.push('x');
    assert_rejected(
        &SecretContent::SshKey(ssh.clone()),
        "mismatched SSH fingerprint",
    );
    ssh = sample_ssh_key();
    ssh.algorithm = "ssh-rsa".into();
    assert_rejected(
        &SecretContent::SshKey(ssh.clone()),
        "mismatched SSH algorithm",
    );
    ssh = sample_ssh_key();
    ssh.hosts = vec!["host.example.invalid".into(); 100];
    assert!(encode_content(&SecretContent::SshKey(ssh.clone())).is_ok());
    ssh.hosts.push("too-many.example.invalid".into());
    assert_rejected(
        &SecretContent::SshKey(ssh.clone()),
        "more than 100 SSH hosts",
    );
    ssh.hosts.clear();
    ssh.private_key = "x".repeat(512 * 1024 + 1).into();
    assert_rejected(&SecretContent::SshKey(ssh), "oversized SSH private key");
}
