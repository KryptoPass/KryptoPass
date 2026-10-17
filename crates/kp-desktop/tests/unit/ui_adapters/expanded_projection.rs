#![allow(clippy::unwrap_used)]

use super::*;
use kryptopass_core::{
    CardContent, CustomContent, CustomRecord, IdentityContent, LicenseContent, SecretContent,
    SshKeyContent, TotpContent, WifiContent, WifiSecurity, parse_totp_input,
};
use serde_json::Value;
use ssh_key::{LineEnding, PrivateKey, private::Ed25519Keypair};
use uuid::Uuid;

fn synthetic_ssh_key() -> SshKeyContent {
    let keypair = Ed25519Keypair::from_seed(&[7; 32]);
    let key = PrivateKey::new(keypair.into(), "synthetic test key").unwrap();
    SshKeyContent::import(
        key.to_openssh(LineEnding::LF).unwrap().to_string().into(),
        Default::default(),
    )
    .unwrap()
}

fn sample_contents() -> Vec<SecretContent> {
    let totp = parse_totp_input(
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&issuer=Example&period=60",
    )
    .unwrap()
    .unwrap();
    vec![
        SecretContent::Custom(CustomContent {
            records: vec![CustomRecord {
                id: Uuid::from_u128(1).to_string().into(),
                key: "Recovery code".into(),
                value: "custom-secret-marker".into(),
                multiline: false,
                concealed: true,
            }],
            notes: "Custom notes".into(),
        }),
        SecretContent::Card(CardContent {
            holder: "Ada Example".into(),
            number: "4111111111111111".into(),
            expiry_month: 12,
            expiry_year: 2032,
            cvv: "cvv-secret-marker".into(),
            pin: "pin-secret-marker".into(),
            notes: "Card notes".into(),
        }),
        SecretContent::Totp(TotpContent {
            config: totp,
            notes: "Authenticator notes".into(),
        }),
        SecretContent::Identity(IdentityContent {
            first_name: "Ada".into(),
            last_name: "Example".into(),
            documents: vec![kryptopass_core::IdentityDocument {
                id: Uuid::from_u128(2).to_string().into(),
                document_type: "Passport".into(),
                number: "document-secret-marker".into(),
                country: "GB".into(),
                expires: "2032-12-31".into(),
            }],
            emails: vec!["email-secret-marker".into()],
            phones: vec!["phone-secret-marker".into()],
            address: "address-secret-marker".into(),
            notes: "Identity notes".into(),
        }),
        SecretContent::Wifi(WifiContent {
            ssid: "Example network".into(),
            security: WifiSecurity::Wpa,
            password: "wifi-secret-marker".into(),
            hidden: false,
            notes: "Wi-Fi notes".into(),
        }),
        SecretContent::License(LicenseContent {
            product: "Example Product".into(),
            license_key: "license-secret-marker".into(),
            holder: "Ada Example".into(),
            email: "license-email-marker".into(),
            version: "1.0".into(),
            purchased: "2025-01-01".into(),
            expires: "2030-01-01".into(),
            notes: "License notes".into(),
        }),
        SecretContent::SshKey(synthetic_ssh_key()),
    ]
}

fn entry(content: SecretContent) -> OpenedEntry {
    OpenedEntry {
        id: SecretId::new(),
        folder: None,
        title: "Synthetic expanded item".into(),
        revision: 1,
        recipients: vec!["0123456789ABCDEF0123456789ABCDEF01234567".into()],
        icon: None,
        related_entry: None,
        content,
    }
}

fn projection(content: &SecretContent, revealed: &[&str], qr_source: Option<&str>) -> Value {
    let revealed = revealed.iter().map(|value| (*value).to_owned()).collect();
    serde_json::from_str(
        &content_projection_with_state(
            SecretId::new(),
            content,
            false,
            1_792_000_001,
            &revealed,
            qr_source,
        )
        .unwrap(),
    )
    .unwrap()
}

fn row<'a>(projection: &'a Value, id: &str) -> &'a Value {
    projection["content"]["fields"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("missing projected row {id}"))
}

#[test]
fn detail_projection_hides_concealed_values_until_the_matching_field_is_revealed() {
    let content = SecretContent::Card(CardContent {
        holder: "Ada Example".into(),
        number: "4111111111111111".into(),
        expiry_month: 12,
        expiry_year: 2032,
        cvv: "cvv-secret-marker".into(),
        pin: "pin-secret-marker".into(),
        notes: "Card notes".into(),
    });

    let hidden = projection(&content, &[], None);
    assert_eq!(row(&hidden, "holder")["value"], "Ada Example");
    for id in ["number", "cvv", "pin"] {
        assert_eq!(row(&hidden, id)["value"], "");
        assert_eq!(row(&hidden, id)["concealed"], true);
        assert_eq!(row(&hidden, id)["revealed"], false);
    }
    for marker in ["4111111111111111", "cvv-secret-marker", "pin-secret-marker"] {
        assert!(
            !hidden.to_string().contains(marker),
            "detail leaked {marker}"
        );
    }

    let only_number_revealed = projection(&content, &["number"], None);
    assert_eq!(
        row(&only_number_revealed, "number")["value"],
        "4111111111111111"
    );
    assert_eq!(row(&only_number_revealed, "number")["revealed"], true);
    assert_eq!(row(&only_number_revealed, "cvv")["value"], "");
    assert_eq!(row(&only_number_revealed, "pin")["value"], "");
}

#[test]
fn field_copy_sources_match_the_encrypted_values_and_keep_concealment_metadata() {
    let contents = sample_contents();
    let expected = [
        (
            0,
            "record:00000000-0000-0000-0000-000000000001",
            "custom-secret-marker",
            true,
        ),
        (1, "number", "4111111111111111", true),
        (1, "cvv", "cvv-secret-marker", true),
        (1, "holder", "Ada Example", false),
        (
            3,
            "document:00000000-0000-0000-0000-000000000002:number",
            "document-secret-marker",
            true,
        ),
        (3, "email:0", "email-secret-marker", true),
        (3, "phone:0", "phone-secret-marker", true),
        (4, "password", "wifi-secret-marker", true),
        (5, "licenseKey", "license-secret-marker", true),
    ];
    for (index, id, expected_value, concealed) in expected {
        let (value, actual_concealed) = content_field(&contents[index], id)
            .unwrap_or_else(|| panic!("missing copy source {id}"));
        assert_eq!(&*value, expected_value, "copy source {id}");
        assert_eq!(actual_concealed, concealed, "concealment for {id}");
    }

    let SecretContent::Totp(totp) = &contents[2] else {
        unreachable!();
    };
    assert!(content_field(&contents[2], "secret").is_none());
    assert_eq!(totp.config.secret, "JBSWY3DPEHPK3PXP");

    let SecretContent::SshKey(ssh) = &contents[6] else {
        unreachable!();
    };
    let (public_key, concealed) = content_field(&contents[6], "publicKey").unwrap();
    assert_eq!(public_key.as_ref(), ssh.public_key);
    assert!(!concealed);
    for id in ["privateKey", "private_key", "passphrase"] {
        assert!(
            content_field(&contents[6], id).is_none(),
            "copy source exposed {id}"
        );
    }
    assert!(!ssh.private_key.is_empty());
    assert!(ssh.passphrase.is_empty());
}

#[test]
fn editor_and_detail_projections_never_serialize_the_ssh_private_key_or_passphrase() {
    let mut ssh = synthetic_ssh_key();
    ssh.passphrase = Default::default();
    let private_marker = ssh.private_key.clone();
    let public_key = ssh.public_key.clone();
    let fingerprint = ssh.fingerprint.clone();
    let editor = serde_json::from_str::<Value>(
        &editor_projection(&entry(SecretContent::SshKey(ssh.clone()))).unwrap(),
    )
    .unwrap();
    let detail = projection(&SecretContent::SshKey(ssh), &[], None);

    for serialized in [editor.to_string(), detail.to_string()] {
        assert!(!serialized.contains(private_marker.as_str()));
        assert!(!serialized.contains("privateKey"));
        assert!(!serialized.contains("passphrase"));
    }
    assert!(editor.to_string().contains(public_key.as_str()));
    assert!(editor.to_string().contains(fingerprint.as_str()));
    assert!(detail.to_string().contains(public_key.as_str()));
}

#[test]
fn qr_source_is_projected_only_when_rust_supplies_the_generated_image() {
    let wifi = SecretContent::Wifi(WifiContent {
        ssid: "Example network".into(),
        security: WifiSecurity::Wpa,
        password: "wifi-secret-marker".into(),
        hidden: false,
        notes: Default::default(),
    });
    let without_qr = projection(&wifi, &[], None);
    assert_eq!(without_qr["content"]["fields"]["hasQr"], true);
    assert!(without_qr["content"]["fields"]["qrSource"].is_null());
    assert!(!without_qr.to_string().contains("wifi-secret-marker"));

    let source = "data:image/png;base64,c3ludGhldGlj";
    let with_qr = projection(&wifi, &[], Some(source));
    assert_eq!(with_qr["content"]["fields"]["qrSource"], source);
    assert!(with_qr.to_string().contains(source));
}
