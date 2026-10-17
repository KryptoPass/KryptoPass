#![allow(clippy::unwrap_used)]

use kryptopass_core::{
    CONTENT_SCHEMA_VERSION, LoginContent, LoginHistory, LoginHistoryEventKind,
    MAX_HISTORY_PASSWORD_BYTES, SecretContent, TotpAlgorithm, TotpConfig, decode_content,
    encode_content, format_totp_input, parse_totp_input,
};

fn sample_login() -> LoginContent {
    LoginContent {
        username: "old@example.org".into(),
        password: "old password".into(),
        url: "https://example.org/login".into(),
        notes: "Recovery information before the edit".into(),
        urls: vec![
            "https://example.org/login".into(),
            "https://example.org/account".into(),
        ],
        totp: Some(TotpConfig {
            secret: "JBSWY3DPEHPK3PXP".into(),
            algorithm: TotpAlgorithm::Sha256,
            digits: 8,
            period: 60,
            issuer: Some("Example".into()),
            account: Some("old@example.org".into()),
            original_label: None,
            extensions: vec![],
        }),
        history: Box::default(),
    }
}

fn decode_login(bytes: &[u8]) -> LoginContent {
    let content = decode_content(bytes).unwrap();
    let SecretContent::Login(login) = &content else {
        panic!("expected login content");
    };
    login.clone()
}

fn assert_snapshot_fields(actual: &LoginContent, expected: &LoginContent) {
    assert_eq!(actual.username, expected.username);
    assert_eq!(actual.password, expected.password);
    assert_eq!(actual.url, expected.url);
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
    assert!(actual.history.events().is_empty());
    assert!(actual.history.version_metadata().is_empty());
    assert!(actual.history.password_change_timestamps().next().is_none());
}

#[test]
fn current_schema_roundtrip_preserves_complete_login_without_nested_history() {
    let mut previous = sample_login();
    previous
        .history
        .record_event(LoginHistoryEventKind::Create, 1);
    previous
        .history
        .record_password("earlier password".into(), 2)
        .unwrap();
    let mut history = LoginHistory::default();
    history
        .record_version(&previous, "Previous title", 42)
        .unwrap();
    previous.username = "changed@example.org".into();
    previous.password = "changed password".into();
    previous.notes.clear();
    previous.set_urls(vec!["https://new.example.org".into()]);
    previous.totp = None;
    previous.history = Box::new(history);

    let bytes = encode_content(&SecretContent::Login(previous)).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(bytes.as_slice()).unwrap();
    assert_eq!(CONTENT_SCHEMA_VERSION, 4);
    assert_eq!(envelope["schemaVersion"], CONTENT_SCHEMA_VERSION);
    let current = decode_login(bytes.as_slice());

    assert_eq!(current.username, "changed@example.org");
    assert_eq!(current.history.version_metadata(), vec![(42, false)]);
    let snapshot = current.history.version_content(0).unwrap();
    assert_snapshot_fields(&snapshot, &sample_login());
    assert!(current.history.version_content(1).is_none());
}

#[test]
fn metadata_orders_complete_and_legacy_versions_together_by_timestamp() {
    let mut login = sample_login();
    let mut history = LoginHistory::default();
    login.password = "snapshot 10".into();
    history.record_version(&login, "Older", 10).unwrap();
    login.password = "snapshot 30".into();
    history.record_version(&login, "Newest", 30).unwrap();
    login.password = "snapshot 20".into();
    history.record_version(&login, "Middle", 20).unwrap();
    history
        .record_password("legacy newest".into(), 100)
        .unwrap();
    history.record_password("legacy older".into(), 25).unwrap();

    assert_eq!(
        history.version_metadata(),
        vec![
            (100, true),
            (30, false),
            (25, true),
            (20, false),
            (10, false)
        ]
    );
    assert_eq!(
        history.version_content(0).unwrap().password,
        "legacy newest"
    );
    assert_eq!(history.version_content(2).unwrap().password, "legacy older");
    assert_eq!(history.version_content(1).unwrap().password, "snapshot 30");
    assert_eq!(history.version_content(3).unwrap().password, "snapshot 20");
    assert_eq!(history.version_content(4).unwrap().password, "snapshot 10");
    assert_eq!(history.version_title(1), Some("Newest"));
    assert_eq!(history.version_title(0), None);
    assert!(history.version_content(5).is_none());
}

#[test]
fn version_retention_keeps_twenty_newest_complete_snapshots() {
    let mut login = sample_login();
    let mut history = LoginHistory::default();
    for timestamp in 1..=25 {
        login.username = format!("user-{timestamp}").into();
        history.record_version(&login, "Login", timestamp).unwrap();
    }

    assert_eq!(
        history.version_metadata(),
        (6..=25)
            .rev()
            .map(|timestamp| (timestamp, false))
            .collect::<Vec<_>>()
    );
    assert_eq!(history.version_content(0).unwrap().username, "user-25");
    assert_eq!(history.version_content(19).unwrap().username, "user-6");
    assert!(history.version_content(20).is_none());
}

#[test]
fn retained_complete_versions_and_legacy_passwords_share_the_size_budget() {
    let mut login = sample_login();
    login.notes = "n".repeat(150 * 1024).into();
    let mut history = LoginHistory::default();
    history
        .record_password("p".repeat(64 * 1024).into(), 1)
        .unwrap();
    for timestamp in 2..=4 {
        history.record_version(&login, "Login", timestamp).unwrap();
    }

    let metadata = history.version_metadata();
    assert!(!metadata.is_empty());
    assert_eq!(metadata[0], (4, false));
    let mut retained_bytes = 0;
    for index in 0..metadata.len() {
        let version = history.version_content(index).unwrap();
        retained_bytes += version.username.len()
            + version.password.len()
            + version.notes.len()
            + version.urls.iter().map(|value| value.len()).sum::<usize>();
    }
    assert!(retained_bytes <= MAX_HISTORY_PASSWORD_BYTES);
    login.history = Box::new(history);
    let bytes = encode_content(&SecretContent::Login(login)).unwrap();
    assert_eq!(
        decode_login(bytes.as_slice()).history.version_metadata(),
        metadata
    );
}

#[test]
fn schema_one_login_migrates_with_empty_versions_and_primary_url() {
    let login = decode_login(
        br#"{"schemaVersion":1,"kind":"login","fields":{"username":"legacy user","password":"legacy password","url":"https://legacy.example.org","notes":"legacy notes"}}"#,
    );

    assert_eq!(login.username, "legacy user");
    assert_eq!(login.password, "legacy password");
    assert_eq!(login.urls, vec!["https://legacy.example.org"]);
    assert_eq!(login.url, login.urls[0]);
    assert!(login.totp.is_none());
    assert!(login.history.version_metadata().is_empty());
}

#[test]
fn schema_two_password_history_remains_readable_as_password_only_versions() {
    let login = decode_login(
        br#"{"schemaVersion":2,"kind":"login","fields":{"username":"current user","password":"current password","url":"https://legacy.example.org","notes":"current notes","urls":["https://legacy.example.org"],"totp":null,"history":{"events":[{"timestamp":30,"kind":"passwordRotated"}],"previousPasswords":[{"password":"previous password","changedAt":30},{"password":"oldest password","changedAt":10}]}}}"#,
    );

    assert_eq!(login.history.events().len(), 1);
    assert_eq!(login.history.events()[0].timestamp, 30);
    assert_eq!(
        login.history.version_metadata(),
        vec![(30, true), (10, true)]
    );
    let previous = login.history.version_content(0).unwrap();
    assert_eq!(previous.password, "previous password");
    assert!(previous.username.is_empty());
    assert!(previous.notes.is_empty());
    assert!(previous.urls.is_empty());
    assert!(previous.totp.is_none());
    assert!(previous.history.version_metadata().is_empty());
    assert_eq!(
        login.history.version_content(1).unwrap().password,
        "oldest password"
    );

    let bytes = encode_content(&SecretContent::Login(login)).unwrap();
    let migrated = decode_login(bytes.as_slice());
    assert_eq!(
        migrated.history.version_metadata(),
        vec![(30, true), (10, true)]
    );
    assert_eq!(
        migrated.history.version_content(0).unwrap().password,
        "previous password"
    );
}

fn snapshot_envelope() -> serde_json::Value {
    let mut login = sample_login();
    let mut history = LoginHistory::default();
    history
        .record_version(&login, "Previous title", 42)
        .unwrap();
    login.history = Box::new(history);
    let bytes = encode_content(&SecretContent::Login(login)).unwrap();
    serde_json::from_slice(bytes.as_slice()).unwrap()
}

#[test]
fn schema_three_rejects_malformed_or_recursive_snapshot_fields() {
    let valid = snapshot_envelope();
    let mut missing_username = valid.clone();
    missing_username["fields"]["history"]["versions"][0]
        .as_object_mut()
        .unwrap()
        .remove("username");
    let mut empty_url = valid.clone();
    empty_url["fields"]["history"]["versions"][0]["urls"] = serde_json::json!([""]);
    let mut invalid_totp = valid.clone();
    invalid_totp["fields"]["history"]["versions"][0]["totp"]["digits"] = serde_json::json!(0);
    let mut recursive_history = valid;
    recursive_history["fields"]["history"]["versions"][0]["history"] =
        serde_json::json!({"events": [], "previousPasswords": [], "versions": []});

    for malformed in [missing_username, empty_url, invalid_totp, recursive_history] {
        assert!(decode_content(&serde_json::to_vec(&malformed).unwrap()).is_err());
    }
}

#[test]
fn schema_three_rejects_history_exceeding_the_combined_size_budget() {
    let mut envelope = snapshot_envelope();
    let mut version = envelope["fields"]["history"]["versions"][0].clone();
    version["notes"] = serde_json::json!("n".repeat(150 * 1024));
    let versions = envelope["fields"]["history"]["versions"]
        .as_array_mut()
        .unwrap();
    versions.clear();
    for timestamp in (1..=3).rev() {
        version["changedAt"] = serde_json::json!(timestamp);
        versions.push(version.clone());
    }
    envelope["fields"]["history"]["previousPasswords"] =
        serde_json::json!([{"password": "p".repeat(64 * 1024), "changedAt": 1}]);

    let bytes = serde_json::to_vec(&envelope).unwrap();
    assert!(bytes.len() < kryptopass_core::MAX_CONTENT_BYTES);
    assert!(decode_content(&bytes).is_err());
}

#[test]
fn equal_timestamps_preserve_each_complete_and_legacy_version_mapping() {
    let mut history = LoginHistory::default();
    let mut login = sample_login();
    login.password = "snapshot first".into();
    history.record_version(&login, "First", 42).unwrap();
    login.password = "snapshot second".into();
    history.record_version(&login, "Second", 42).unwrap();
    history.record_password("legacy first".into(), 42).unwrap();
    history.record_password("legacy second".into(), 42).unwrap();

    assert_eq!(
        history.version_metadata(),
        vec![(42, false), (42, false), (42, true), (42, true)]
    );
    for (index, password) in [
        "snapshot first",
        "snapshot second",
        "legacy first",
        "legacy second",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(history.version_content(index).unwrap().password, *password);
    }
    assert_eq!(history.version_title(0), Some("First"));
    assert_eq!(history.version_title(1), Some("Second"));
    assert_eq!(history.version_title(2), None);
}

#[derive(Default)]
struct SingleSecretStorage(Option<kryptopass_core::Secret>);

impl kryptopass_core::Storage for SingleSecretStorage {
    fn list_folders(&self) -> anyhow::Result<Vec<kryptopass_core::Folder>> {
        unreachable!("folder operations are outside this service scenario")
    }
    fn list_secrets(&self) -> anyhow::Result<Vec<kryptopass_core::Secret>> {
        Ok(self.0.iter().cloned().collect())
    }
    fn list_secret_metadata(&self) -> anyhow::Result<Vec<kryptopass_core::SecretMetadata>> {
        unreachable!("metadata listing is outside this service scenario")
    }
    fn find_folder(
        &self,
        _: kryptopass_core::FolderId,
    ) -> anyhow::Result<Option<kryptopass_core::Folder>> {
        unreachable!("folder operations are outside this service scenario")
    }
    fn find_secret(
        &self,
        id: kryptopass_core::SecretId,
    ) -> anyhow::Result<Option<kryptopass_core::Secret>> {
        Ok(self.0.as_ref().filter(|secret| secret.id == id).cloned())
    }
    fn insert_folder(&mut self, _: kryptopass_core::Folder) -> anyhow::Result<()> {
        unreachable!("folder operations are outside this service scenario")
    }
    fn insert_secret(&mut self, secret: kryptopass_core::Secret) -> anyhow::Result<()> {
        anyhow::ensure!(self.0.is_none(), "test store already contains a secret");
        self.0 = Some(secret);
        Ok(())
    }
    fn replace_secret(
        &mut self,
        mut secret: kryptopass_core::Secret,
        expected_revision: u64,
    ) -> anyhow::Result<()> {
        let previous = self.0.as_ref().unwrap();
        anyhow::ensure!(
            previous.id == secret.id && previous.revision == expected_revision,
            "stale secret revision"
        );
        secret.revision = expected_revision + 1;
        self.0 = Some(secret);
        Ok(())
    }
    fn replace_folder(&mut self, _: kryptopass_core::Folder) -> anyhow::Result<()> {
        unreachable!("folder operations are outside this service scenario")
    }
    fn move_folder(
        &mut self,
        _: kryptopass_core::FolderId,
        _: Option<kryptopass_core::FolderId>,
        _: Option<kryptopass_core::FolderId>,
    ) -> anyhow::Result<()> {
        unreachable!("folder operations are outside this service scenario")
    }
    fn remove_secret(&mut self, _: kryptopass_core::SecretId) -> anyhow::Result<()> {
        unreachable!("removal is outside this service scenario")
    }
    fn remove_folder(&mut self, _: kryptopass_core::FolderId) -> anyhow::Result<()> {
        unreachable!("folder operations are outside this service scenario")
    }
    fn move_secret(
        &mut self,
        _: kryptopass_core::SecretId,
        _: Option<kryptopass_core::FolderId>,
    ) -> anyhow::Result<()> {
        unreachable!("moving is outside this service scenario")
    }
    fn replace_encryption(
        &mut self,
        _: kryptopass_core::SecretId,
        _: Vec<u8>,
        _: kryptopass_core::RecipientSet,
        _: u64,
    ) -> anyhow::Result<()> {
        unreachable!("recipient rotation is outside this service scenario")
    }
}

struct ReversibleTestCrypto;

impl kryptopass_core::OpenPgp for ReversibleTestCrypto {
    fn validate_recipients(&self, _: &kryptopass_core::RecipientSet) -> anyhow::Result<()> {
        Ok(())
    }
    fn encrypt(
        &self,
        plaintext: &[u8],
        _: &kryptopass_core::RecipientSet,
    ) -> anyhow::Result<Vec<u8>> {
        Ok(plaintext.iter().map(|byte| byte ^ 0xa5).collect())
    }
    fn decrypt(&self, ciphertext: &[u8]) -> anyhow::Result<kryptopass_core::SensitiveBytes> {
        Ok(ciphertext
            .iter()
            .map(|byte| byte ^ 0xa5)
            .collect::<Vec<_>>()
            .into())
    }
}

#[test]
fn service_edits_snapshot_previous_login_and_copy_events_keep_versions_unchanged() {
    use kryptopass_core::{KeyFingerprint, RecipientSelection, RecipientSet, SecretsService};

    let recipients = RecipientSet::new(vec![
        KeyFingerprint::parse("0123456789ABCDEF0123456789ABCDEF01234567").unwrap(),
    ])
    .unwrap();
    let mut service = SecretsService::new(SingleSecretStorage::default(), ReversibleTestCrypto);
    let previous = sample_login();
    let id = service
        .save_content(
            None,
            None,
            "Previous title".into(),
            RecipientSelection::Explicit(recipients.clone()),
            SecretContent::Login(previous.clone()),
            None,
        )
        .unwrap();
    let mut updated = sample_login();
    updated.username = "updated@example.org".into();
    updated.password = "new password".into();
    updated.notes = "Updated recovery information".into();
    updated.set_urls(vec!["https://updated.example.org".into()]);
    updated.totp.as_mut().unwrap().period = 30;
    let expected = updated.clone();
    service
        .save_content(
            Some(id),
            None,
            "Updated title".into(),
            RecipientSelection::Explicit(recipients),
            SecretContent::Login(updated),
            Some(0),
        )
        .unwrap();

    let current = service.read_content(id).unwrap();
    let SecretContent::Login(login) = &current else {
        panic!("expected login content")
    };
    assert_eq!(login.username, expected.username);
    assert_eq!(login.password, expected.password);
    assert_eq!(login.notes, expected.notes);
    assert_eq!(login.urls, expected.urls);
    assert_eq!(login.totp.as_ref().unwrap().period, 30);
    let versions = login.history.version_metadata();
    assert_eq!(versions.len(), 1);
    assert!(!versions[0].1);
    assert_eq!(login.history.version_title(0), Some("Previous title"));
    assert_snapshot_fields(&login.history.version_content(0).unwrap(), &previous);
    assert!(login.history.password_change_timestamps().next().is_none());
    let stored = service.list_secrets().unwrap();
    assert_eq!(stored[0].title, "Updated title");
    assert_eq!(stored[0].revision, 1);

    service
        .record_login_event(
            id,
            1,
            current,
            LoginHistoryEventKind::CopyPassword,
            u64::MAX,
        )
        .unwrap();
    let copied = service.read_content(id).unwrap();
    let SecretContent::Login(login) = &copied else {
        panic!("expected login content")
    };
    assert_eq!(login.history.version_metadata(), versions);
    assert_eq!(login.history.version_title(0), Some("Previous title"));
    assert_snapshot_fields(&login.history.version_content(0).unwrap(), &previous);
    assert_eq!(login.username, expected.username);
    assert_eq!(login.password, expected.password);
    assert!(login.history.events()[0].kind == LoginHistoryEventKind::CopyPassword);
    assert_eq!(login.history.events()[0].timestamp, u64::MAX);
    assert_eq!(service.list_secrets().unwrap()[0].revision, 2);
}

#[test]
fn equivalent_totp_uri_with_retained_canonical_label_does_not_create_a_content_version() {
    use kryptopass_core::{KeyFingerprint, RecipientSelection, RecipientSet, SecretsService};

    let recipients = RecipientSet::new(vec![
        KeyFingerprint::parse("0123456789ABCDEF0123456789ABCDEF01234567").unwrap(),
    ])
    .unwrap();
    let mut service = SecretsService::new(SingleSecretStorage::default(), ReversibleTestCrypto);
    let existing = sample_login();
    let id = service
        .save_content(
            None,
            None,
            "Authenticator".into(),
            RecipientSelection::Explicit(recipients.clone()),
            SecretContent::Login(existing.clone()),
            None,
        )
        .unwrap();

    let uri = format_totp_input(existing.totp.as_ref().unwrap()).unwrap();
    let parsed = parse_totp_input(uri.as_str()).unwrap().unwrap();
    assert!(parsed.original_label.is_some());
    let mut updated = existing;
    updated.totp = Some(parsed);
    service
        .save_content(
            Some(id),
            None,
            "Authenticator".into(),
            RecipientSelection::Explicit(recipients),
            SecretContent::Login(updated),
            Some(0),
        )
        .unwrap();

    let content = service.read_content(id).unwrap();
    let SecretContent::Login(login) = &content else {
        panic!("expected login content");
    };
    assert!(login.history.version_metadata().is_empty());
    assert_eq!(login.history.events().len(), 1);
    assert!(login.history.events()[0].kind == LoginHistoryEventKind::Create);
    assert!(login.totp.as_ref().unwrap().original_label.is_some());
}
