//! Shared content vectors: the exact JSON envelopes each secret kind encodes to.
//!
//! `fixtures/content-vectors-v1.json` is the contract with Android. Desktop proves here that
//! every vector decodes and re-encodes to the same JSON tree; the Android app does the same
//! with its own codec, so a field added or renamed on one side fails on the other.
//! Regenerate with `KP_WRITE_CONTENT_VECTORS=1 cargo test -p kp-core --test content_vectors`.
#![allow(clippy::unwrap_used)]

use kryptopass_core::{
    CardContent, CustomContent, CustomRecord, IdentityContent, IdentityDocument, LicenseContent,
    LoginContent, NoteContent, SecretContent, SshKeyContent, TotpAlgorithm, TotpConfig,
    TotpContent, WifiContent, WifiSecurity, decode_content, encode_content,
};
use serde_json::{Value, json};
use ssh_key::{LineEnding, PrivateKey, private::Ed25519Keypair};
use uuid::Uuid;

const FIXTURE: &str = "tests/fixtures/content-vectors-v1.json";

fn totp() -> TotpConfig {
    TotpConfig {
        secret: "JBSWY3DPEHPK3PXP".into(),
        algorithm: TotpAlgorithm::Sha256,
        digits: 8,
        period: 60,
        issuer: Some("Example".into()),
        account: Some("ada@example.invalid".into()),
        original_label: None,
        extensions: vec![],
    }
}

fn ssh_key() -> SshKeyContent {
    let keypair = Ed25519Keypair::from_seed(&[7; 32]);
    let key = PrivateKey::new(keypair.into(), "synthetic test key").unwrap();
    let private_key = key.to_openssh(LineEnding::LF).unwrap().to_string();
    let mut content = SshKeyContent::import(private_key.into(), Default::default()).unwrap();
    content.user = "ada".into();
    content.hosts = vec!["example.invalid".into()];
    content.notes = "SSH notes".into();
    content
}

fn samples() -> Vec<(&'static str, SecretContent)> {
    vec![
        (
            "login-minimal",
            SecretContent::Login(LoginContent {
                username: "ada".into(),
                password: "pa55 word".into(),
                notes: String::new().into(),
                urls: vec![],
                totp: None,
                history: Box::default(),
            }),
        ),
        (
            "login-complete",
            SecretContent::Login(LoginContent {
                username: "ada@example.invalid".into(),
                password: "  spaces and ünïcödé 🔑  ".into(),
                notes: "line one\nline two".into(),
                urls: vec![
                    "https://example.invalid/login".into(),
                    "https://example.invalid/account".into(),
                ],
                totp: Some(totp()),
                history: Box::default(),
            }),
        ),
        (
            "note",
            SecretContent::Note(NoteContent {
                value: "# Title\n\nA **markdown** body\nwith lines".into(),
                notes: "Extra notes".into(),
            }),
        ),
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
        (
            "totp",
            SecretContent::Totp(TotpContent {
                config: totp(),
                notes: "Authenticator notes".into(),
            }),
        ),
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
        ("ssh-key", SecretContent::SshKey(ssh_key())),
    ]
}

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)
}

#[test]
fn write_vectors_when_asked() {
    if std::env::var_os("KP_WRITE_CONTENT_VECTORS").is_none() {
        return;
    }
    let vectors: Vec<Value> = samples()
        .into_iter()
        .map(|(name, content)| {
            let bytes = encode_content(&content).unwrap();
            let envelope: Value = serde_json::from_slice(bytes.as_slice()).unwrap();
            json!({ "name": name, "kind": envelope["kind"], "envelope": envelope })
        })
        .collect();
    let document = json!({ "version": 1, "vectors": vectors });
    std::fs::write(
        fixture_path(),
        serde_json::to_string_pretty(&document).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
fn every_vector_decodes_and_reencodes_to_the_same_json() {
    let document: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture_path()).unwrap()).unwrap();
    assert_eq!(document["version"], 1);
    let vectors = document["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), samples().len(), "vector set is out of date");
    for vector in vectors {
        let name = vector["name"].as_str().unwrap();
        let bytes = serde_json::to_vec(&vector["envelope"]).unwrap();
        let decoded = decode_content(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let reencoded = encode_content(&decoded).unwrap();
        let actual: Value = serde_json::from_slice(reencoded.as_slice()).unwrap();
        assert_eq!(actual, vector["envelope"], "{name} changed on round trip");
    }
}

#[test]
fn vectors_match_what_the_code_encodes_today() {
    let document: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture_path()).unwrap()).unwrap();
    let vectors = document["vectors"].as_array().unwrap();
    for (name, content) in samples() {
        let current: Value =
            serde_json::from_slice(encode_content(&content).unwrap().as_slice()).unwrap();
        let stored = vectors
            .iter()
            .find(|vector| vector["name"] == name)
            .unwrap_or_else(|| panic!("{name}: missing vector; regenerate"));
        assert_eq!(
            stored["envelope"], current,
            "{name}: the encoding changed; review it and regenerate the vectors"
        );
    }
}
