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
        notes: "Authenticator notes".to_owned(),
    }
}

fn sample_ssh_key() -> SshKeyContent {
    let keypair = Ed25519Keypair::from_seed(&[7; 32]);
    let key = PrivateKey::new(keypair.into(), "synthetic test key").unwrap();
    let private_key = key.to_openssh(LineEnding::LF).unwrap().to_string();
    let mut content = SshKeyContent::import(private_key, String::new()).unwrap();
    content.user = "ada".to_owned();
    content.hosts = vec!["example.invalid".to_owned()];
    content.notes = "SSH notes".to_owned();
    content
}

fn sample_contents() -> Vec<(&'static str, SecretContent)> {
    vec![
        (
            "custom",
            SecretContent::Custom(CustomContent {
                records: vec![CustomRecord {
                    id: Uuid::from_u128(1).to_string(),
                    key: "Recovery code".to_owned(),
                    value: "  value with spaces\nand a newline  ".to_owned(),
                    multiline: true,
                    concealed: true,
                }],
                notes: "Custom notes".to_owned(),
            }),
        ),
        (
            "card",
            SecretContent::Card(CardContent {
                holder: "Ada Example".to_owned(),
                number: "4111-1111 1111-1111".to_owned(),
                expiry_month: 12,
                expiry_year: 2032,
                cvv: "123".to_owned(),
                pin: "2468".to_owned(),
                notes: "Card notes".to_owned(),
            }),
        ),
        ("totp", SecretContent::Totp(sample_totp())),
        (
            "identity",
            SecretContent::Identity(IdentityContent {
                first_name: "Ada".to_owned(),
                last_name: "Example".to_owned(),
                documents: vec![IdentityDocument {
                    id: Uuid::from_u128(2).to_string(),
                    document_type: "Passport".to_owned(),
                    number: "P123456".to_owned(),
                    country: "GB".to_owned(),
                    expires: "2032-12-31".to_owned(),
                }],
                emails: vec!["ada@example.invalid".to_owned()],
                phones: vec!["+1 555 0100".to_owned()],
                address: "1 Example Street".to_owned(),
                notes: "Identity notes".to_owned(),
            }),
        ),
        (
            "wifi",
            SecretContent::Wifi(WifiContent {
                ssid: "Example network".to_owned(),
                security: WifiSecurity::Wpa3,
                password: "example-network-password".to_owned(),
                hidden: true,
                notes: "Wi-Fi notes".to_owned(),
            }),
        ),
        (
            "license",
            SecretContent::License(LicenseContent {
                product: "Example Product".to_owned(),
                license_key: "AAAA-BBBB-CCCC-DDDD".to_owned(),
                holder: "Ada Example".to_owned(),
                email: "ada@example.invalid".to_owned(),
                version: "2.1".to_owned(),
                purchased: "2025-01-02".to_owned(),
                expires: "2030-01-02".to_owned(),
                notes: "License notes".to_owned(),
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

        for unsupported_version in [3, CONTENT_SCHEMA_VERSION + 1] {
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
                id: Uuid::from_u128(10).to_string(),
                key: "Repeated key".to_owned(),
                value: " first\nvalue ".to_owned(),
                multiline: true,
                concealed: true,
            },
            CustomRecord {
                id: Uuid::from_u128(11).to_string(),
                key: "Repeated key".to_owned(),
                value: "second value".to_owned(),
                multiline: false,
                concealed: false,
            },
        ],
        notes: String::new(),
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
    content.records[1].id = Uuid::from_u128(11).to_string();
    content.records[0].id = Uuid::nil().to_string();
    assert_rejected(&SecretContent::Custom(content.clone()), "nil custom ID");
    content.records[0].id = Uuid::from_u128(10).to_string();
    content.records[0].key.clear();
    assert_rejected(&SecretContent::Custom(content.clone()), "empty custom key");

    content.records[0].key = "k".repeat(512);
    content.records[0].key.push('k');
    assert_rejected(
        &SecretContent::Custom(content.clone()),
        "custom key longer than 512 bytes",
    );
    content.records[0].key.pop();
    content.records.resize_with(100, CustomRecord::default);
    for (index, record) in content.records.iter_mut().enumerate() {
        record.id = Uuid::from_u128(index as u128 + 20).to_string();
        if record.key.is_empty() {
            record.key = "field".to_owned();
        }
    }
    content.records[0].value = "v".repeat(256 * 1024);
    content.notes = "n".repeat(256 * 1024);
    assert!(encode_content(&SecretContent::Custom(content.clone())).is_ok());
    content.records[0].value.push('v');
    assert_rejected(
        &SecretContent::Custom(content.clone()),
        "custom value longer than 256 KiB",
    );
    content.records[0].value.pop();
    content.notes.push('n');
    assert_rejected(
        &SecretContent::Custom(content.clone()),
        "custom notes longer than 256 KiB",
    );
    content.notes.pop();
    let mut extra_record = CustomRecord::default();
    extra_record.id = Uuid::from_u128(500).to_string();
    extra_record.key = "extra".to_owned();
    content.records.push(extra_record);
    assert_rejected(
        &SecretContent::Custom(content),
        "more than 100 custom records",
    );
}

#[test]
fn card_validation_accepts_supported_boundaries_and_rejects_invalid_digits_and_dates() {
    let mut card = CardContent {
        holder: String::new(),
        number: "1234 5678-9012".to_owned(),
        expiry_month: 1,
        expiry_year: 1900,
        cvv: "123".to_owned(),
        pin: String::new(),
        notes: String::new(),
    };
    assert!(encode_content(&SecretContent::Card(card.clone())).is_ok());
    assert_eq!(card.normalized_number().as_str(), "123456789012");
    card.number = "1234567890123456789".to_owned();
    card.expiry_year = 9999;
    card.cvv = "1234".to_owned();
    assert!(encode_content(&SecretContent::Card(card.clone())).is_ok());
    card.number = "123456x890123".to_owned();
    assert_rejected(
        &SecretContent::Card(card.clone()),
        "non-digit card separator",
    );
    card.number = "123456789012".to_owned();
    card.cvv = "1234".to_owned();

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
    card.number = "12345678901".to_owned();
    assert_rejected(&SecretContent::Card(card.clone()), "short card number");
    card.number = "12345678901234567890".to_owned();
    assert_rejected(&SecretContent::Card(card.clone()), "long card number");
    card.number = "123456789012".to_owned();
    card.cvv = "12x".to_owned();
    assert_rejected(
        &SecretContent::Card(card.clone()),
        "non-numeric card security code",
    );
    card.cvv.clear();
    card.pin = "12x".to_owned();
    assert_rejected(&SecretContent::Card(card), "non-numeric card PIN");
}

#[test]
fn identity_document_and_contact_collections_enforce_stable_unique_ids_and_limits() {
    let mut identity = IdentityContent::default();
    identity.documents.push(IdentityDocument {
        id: Uuid::from_u128(30).to_string(),
        document_type: "Passport".to_owned(),
        number: "P123456".to_owned(),
        country: "GB".to_owned(),
        expires: "not-normalized date text".to_owned(),
    });
    identity.emails = vec!["ada@example.invalid".to_owned()];
    assert!(encode_content(&SecretContent::Identity(identity.clone())).is_ok());

    let duplicate = identity.documents[0].clone();
    identity.documents.push(duplicate);
    assert_rejected(
        &SecretContent::Identity(identity.clone()),
        "duplicate identity document ID",
    );
    identity.documents.pop();
    identity.documents[0].id = Uuid::nil().to_string();
    assert_rejected(
        &SecretContent::Identity(identity.clone()),
        "nil identity document ID",
    );
    identity.documents[0].id = Uuid::from_u128(30).to_string();
    identity.documents = (0..100)
        .map(|index| {
            let mut document = IdentityDocument::default();
            document.id = Uuid::from_u128(index + 100).to_string();
            document
        })
        .collect();
    assert!(encode_content(&SecretContent::Identity(identity.clone())).is_ok());
    identity.emails = vec!["person@example.invalid".to_owned(); 100];
    assert!(encode_content(&SecretContent::Identity(identity.clone())).is_ok());
    identity.phones = vec!["+1 555 0100".to_owned(); 101];
    assert_rejected(
        &SecretContent::Identity(identity.clone()),
        "more than 100 identity phones",
    );
    identity.phones.clear();
    let mut extra_document = IdentityDocument::default();
    extra_document.id = Uuid::from_u128(300).to_string();
    identity.documents.push(extra_document);
    assert_rejected(
        &SecretContent::Identity(identity.clone()),
        "more than 100 documents",
    );
    identity.documents.clear();
    identity.emails = vec!["person@example.invalid".to_owned(); 101];
    assert_rejected(&SecretContent::Identity(identity), "more than 100 emails");
}

#[test]
fn wifi_credentials_enforce_security_specific_passwords_and_escape_qr_fields() {
    let mut wifi = WifiContent {
        ssid: "office;\"wifi".to_owned(),
        security: WifiSecurity::Wpa,
        password: "password;with,delimiters".to_owned(),
        hidden: true,
        notes: String::new(),
    };
    assert!(encode_content(&SecretContent::Wifi(wifi.clone())).is_ok());
    assert_eq!(
        kryptopass_core::wifi_qr_payload(&wifi).unwrap().as_str(),
        "WIFI:T:WPA;S:office\\;\\\"wifi;P:password\\;with\\,delimiters;H:true;;"
    );
    wifi.ssid = "office:network\\".to_owned();
    wifi.password = "password:with\\slash".to_owned();
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
    wifi.password = "unexpected".to_owned();
    assert_rejected(&SecretContent::Wifi(wifi.clone()), "open Wi-Fi password");
    wifi.password.clear();
    wifi.ssid = "".to_owned();
    assert_rejected(&SecretContent::Wifi(wifi.clone()), "empty Wi-Fi SSID");
    wifi.ssid = "n".repeat(33);
    assert_rejected(&SecretContent::Wifi(wifi.clone()), "overlong Wi-Fi SSID");
    wifi.ssid = "s".repeat(32);
    wifi.security = WifiSecurity::Wpa;
    wifi.password = "f".repeat(64);
    assert!(encode_content(&SecretContent::Wifi(wifi.clone())).is_ok());
    wifi.password = format!("{}g", "f".repeat(63));
    assert_rejected(
        &SecretContent::Wifi(wifi.clone()),
        "non-hex 64-byte WPA key",
    );

    wifi.ssid = "WEP".to_owned();
    wifi.security = WifiSecurity::Wep;
    wifi.password = "abcde".to_owned();
    assert!(encode_content(&SecretContent::Wifi(wifi.clone())).is_ok());
    wifi.password = "abc".to_owned();
    assert_rejected(&SecretContent::Wifi(wifi.clone()), "short WEP password");

    wifi.security = WifiSecurity::Wpa3;
    wifi.password = "a".repeat(8);
    assert!(encode_content(&SecretContent::Wifi(wifi.clone())).is_ok());
    assert!(kryptopass_core::wifi_qr_payload(&wifi).is_err());
    wifi.security = WifiSecurity::Wpa;
    assert!(kryptopass_core::wifi_qr_payload(&wifi).is_ok());
    wifi.password = "a".repeat(7);
    assert_rejected(&SecretContent::Wifi(wifi), "short WPA3 password");
}

#[test]
fn license_required_fields_and_size_limits_are_enforced() {
    let mut license = LicenseContent::default();
    license.product = "Product".to_owned();
    license.license_key = "license-key".to_owned();
    assert!(encode_content(&SecretContent::License(license.clone())).is_ok());
    license.product.clear();
    assert_rejected(
        &SecretContent::License(license.clone()),
        "empty license product",
    );
    license.product = "Product".to_owned();
    license.license_key.clear();
    assert_rejected(
        &SecretContent::License(license.clone()),
        "empty license key",
    );
    license.license_key = "license-key".to_owned();
    license.product = "p".repeat(16 * 1024);
    license.notes = "n".repeat(256 * 1024);
    assert!(encode_content(&SecretContent::License(license.clone())).is_ok());
    license.notes.push('n');
    assert_rejected(
        &SecretContent::License(license.clone()),
        "license notes longer than 256 KiB",
    );
    license.notes.pop();
    license.product = "p".repeat(16 * 1024 + 1);
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
        SshKeyContent::import(ssh.private_key.clone(), "unexpected passphrase".to_owned()).is_err()
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
    ssh.algorithm = "ssh-rsa".to_owned();
    assert_rejected(
        &SecretContent::SshKey(ssh.clone()),
        "mismatched SSH algorithm",
    );
    ssh = sample_ssh_key();
    ssh.hosts = vec!["host.example.invalid".to_owned(); 100];
    assert!(encode_content(&SecretContent::SshKey(ssh.clone())).is_ok());
    ssh.hosts.push("too-many.example.invalid".to_owned());
    assert_rejected(
        &SecretContent::SshKey(ssh.clone()),
        "more than 100 SSH hosts",
    );
    ssh.hosts.clear();
    ssh.private_key = "x".repeat(512 * 1024 + 1);
    assert_rejected(&SecretContent::SshKey(ssh), "oversized SSH private key");
}
