#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{ProjectionDecryptor, VaultSearchSource};
use crate::storage::SqliteStorage;
use kryptopass_core::{KeyFingerprint, RecipientSet, Secret, SecretId, SecretKind, Storage};
use kryptopass_search::{
    CatalogRecord, Index, Limits, Projection, RecordKey, SearchError, SearchSource,
};
use kryptopass_secure_memory::ProtectedBytes;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

const LOGIN_JSON: &[u8] = br#"{"schemaVersion":1,"kind":"login","fields":{"username":"currentuser","password":"passwordmarker","notes":"notesmarker","urls":[],"totp":null,"history":{"events":[],"versions":[]}}}"#;
const CIPHERTEXT: &[u8] = b"synthetic-ciphertext-without-openpgp";

struct TestVault {
    directory: PathBuf,
    path: PathBuf,
}

impl TestVault {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("kryptopass-search-test-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = directory.join("synthetic.sqlite3");
        drop(SqliteStorage::open(&path).unwrap());
        Self { directory, path }
    }

    fn insert(&self, secret: Secret) {
        SqliteStorage::open_existing(&self.path)
            .unwrap()
            .insert_secret(secret)
            .unwrap();
    }
}

impl Drop for TestVault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn primary() -> KeyFingerprint {
    KeyFingerprint::parse("0123456789ABCDEF0123456789ABCDEF01234567").unwrap()
}

fn other_recipient() -> KeyFingerprint {
    KeyFingerprint::parse("FEDCBA9876543210FEDCBA9876543210FEDCBA98").unwrap()
}

fn secret() -> Secret {
    Secret {
        id: SecretId::new(),
        folder_id: None,
        title: "Synthetic account".into(),
        kind: SecretKind::Login,
        icon: None,
        recipients: RecipientSet::new(vec![primary()]).unwrap(),
        ciphertext: CIPHERTEXT.to_vec(),
        favorite: false,
        trashed: false,
        revision: 0,
    }
}

fn key(secret: &Secret) -> RecordKey {
    RecordKey {
        id: *secret.id.0.as_bytes(),
        revision: secret.revision,
    }
}

struct FakeDecryptor<F> {
    calls: Arc<AtomicUsize>,
    action: F,
}

impl<F> ProjectionDecryptor for FakeDecryptor<F>
where
    F: Fn(&mut ProtectedBytes) -> Result<(), SearchError> + Send + 'static,
{
    fn decrypt(
        &self,
        ciphertext: &[u8],
        plaintext: &mut ProtectedBytes,
    ) -> Result<(), SearchError> {
        assert_eq!(ciphertext, CIPHERTEXT);
        assert!(
            plaintext.is_empty(),
            "a previous plaintext survived into a new decrypt"
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        (self.action)(plaintext)
    }
}

fn write_login(plaintext: &mut ProtectedBytes) -> Result<(), SearchError> {
    plaintext
        .try_extend_from_slice(LOGIN_JSON)
        .map_err(|_| SearchError::LimitExceeded)
}

fn load_error<D: ProjectionDecryptor>(
    source: &mut VaultSearchSource<D>,
    record: RecordKey,
) -> SearchError {
    match source.load(record) {
        Ok(_) => panic!("unauthorized or invalid projection unexpectedly returned"),
        Err(error) => error,
    }
}

fn clean<D>(source: &VaultSearchSource<D>) {
    if let Some(plaintext) = &source.plaintext {
        assert!(plaintext.is_empty());
        assert_eq!(plaintext.len(), 0);
    }
}

fn unavailable(error: SearchError) -> bool {
    if error == SearchError::ProtectionUnavailable {
        eprintln!("search source unavailable: required memory protection absent");
        true
    } else {
        false
    }
}

fn observe_projection(projection: Projection, record: RecordKey) {
    let mut index = Index::new(Limits {
        block_bytes: 4096,
        ..Limits::default()
    });
    index
        .reconcile(vec![CatalogRecord {
            key: record,
            public_fields: vec![],
            private_eligible: true,
        }])
        .unwrap();
    index.set_active(true);
    assert_eq!(index.next_pending(), Some(record));
    if let Err(error) = index.complete(record, Ok(projection)) {
        assert!(
            unavailable(error),
            "could not index the synthetic projection: {error:?}"
        );
        return;
    }
    assert_eq!(index.search("currentuser").unwrap(), vec![record.id]);
    for excluded in ["passwordmarker", "notesmarker", "synthetic", "username"] {
        assert!(index.search(excluded).unwrap().is_empty());
    }
    assert_eq!(index.coverage().indexed, 1);
}

#[test]
fn unauthorized_or_stale_records_are_rejected_before_decryption() {
    let vault = TestVault::new();
    let calls = Arc::new(AtomicUsize::new(0));
    for case in ["revision", "trashed", "kind", "recipient", "missing"] {
        let mut stored = secret();
        match case {
            "trashed" => stored.trashed = true,
            "kind" => stored.kind = SecretKind::SecureNote,
            "recipient" => stored.recipients = RecipientSet::new(vec![other_recipient()]).unwrap(),
            _ => {}
        }
        let mut record = key(&stored);
        if case == "revision" {
            record.revision += 1;
        }
        if case != "missing" {
            vault.insert(stored);
        }
        let decryptor = FakeDecryptor {
            calls: calls.clone(),
            action: write_login,
        };
        let mut source =
            VaultSearchSource::with_decryptor(vault.path.clone(), primary(), decryptor);
        let expected = if case == "recipient" {
            SearchError::Unavailable
        } else {
            SearchError::StaleRevision
        };
        assert_eq!(load_error(&mut source, record), expected, "{case}");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "decrypted {case}");
        assert!(source.plaintext.is_none(), "allocated plaintext for {case}");
    }
}

#[test]
fn authorized_revision_projects_its_username_and_reuses_an_empty_temporary() {
    let vault = TestVault::new();
    let mut stored = secret();
    stored.recipients = RecipientSet::new(vec![other_recipient(), primary()]).unwrap();
    let record = key(&stored);
    vault.insert(stored);
    let calls = Arc::new(AtomicUsize::new(0));
    let decryptor = FakeDecryptor {
        calls: calls.clone(),
        action: write_login,
    };
    let mut source = VaultSearchSource::with_decryptor(vault.path.clone(), primary(), decryptor);

    let first = match source.load(record) {
        Ok(projection) => projection,
        Err(error) if unavailable(error) => {
            clean(&source);
            return;
        }
        Err(error) => panic!("authorized login failed: {error:?}"),
    };
    clean(&source);
    let temporary = source.plaintext.as_ref().unwrap();
    let capacity = temporary.capacity();
    let address = temporary.as_slice().as_ptr();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    observe_projection(first, record);

    let second = match source.load(record) {
        Ok(projection) => projection,
        Err(error) if unavailable(error) => {
            clean(&source);
            return;
        }
        Err(error) => panic!("second authorized login failed: {error:?}"),
    };
    clean(&source);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let temporary = source.plaintext.as_ref().unwrap();
    assert_eq!(temporary.capacity(), capacity);
    assert_eq!(temporary.as_slice().as_ptr(), address);
    observe_projection(second, record);
}

#[test]
fn changed_or_deleted_records_are_discarded_after_decryption() {
    for case in ["revision", "deleted", "trashed", "recipients"] {
        let vault = TestVault::new();
        let stored = secret();
        let record = key(&stored);
        let id = stored.id;
        vault.insert(stored);
        let calls = Arc::new(AtomicUsize::new(0));
        let database = vault.path.clone();
        let decryptor = FakeDecryptor {
            calls: calls.clone(),
            action: move |plaintext: &mut ProtectedBytes| {
                write_login(plaintext)?;
                let mut storage = SqliteStorage::open_existing(&database).unwrap();
                match case {
                    "deleted" => storage.remove_secret(id).unwrap(),
                    "recipients" => storage
                        .replace_encryption(
                            id,
                            b"replacement-synthetic-ciphertext".to_vec(),
                            RecipientSet::new(vec![other_recipient()]).unwrap(),
                            record.revision,
                        )
                        .unwrap(),
                    _ => {
                        let mut changed = storage.find_secret(id).unwrap().unwrap();
                        if case == "trashed" {
                            changed.trashed = true;
                        } else {
                            changed.title = "Changed during synthetic decryption".into();
                        }
                        storage.replace_secret(changed, record.revision).unwrap();
                    }
                }
                Ok(())
            },
        };
        let mut source =
            VaultSearchSource::with_decryptor(vault.path.clone(), primary(), decryptor);
        let error = load_error(&mut source, record);
        clean(&source);
        if unavailable(error) {
            return;
        }
        assert_eq!(error, SearchError::StaleRevision, "{case}");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "{case}");
        assert!(source.plaintext.as_ref().unwrap().is_empty());
        let after = SqliteStorage::open_existing(&vault.path)
            .unwrap()
            .find_secret(id)
            .unwrap();
        if case == "deleted" {
            assert!(after.is_none());
        } else {
            assert_eq!(after.unwrap().revision, record.revision + 1);
        }
    }
}

#[test]
fn partial_decryption_errors_clear_the_temporary_before_reuse() {
    let vault = TestVault::new();
    let stored = secret();
    let record = key(&stored);
    vault.insert(stored);
    let calls = Arc::new(AtomicUsize::new(0));
    let decryptor = FakeDecryptor {
        calls: calls.clone(),
        action: |plaintext: &mut ProtectedBytes| {
            plaintext
                .try_extend_from_slice(b"partial-private-marker")
                .unwrap();
            Err(SearchError::Unavailable)
        },
    };
    let mut source = VaultSearchSource::with_decryptor(vault.path.clone(), primary(), decryptor);
    for expected_calls in 1..=2 {
        let error = load_error(&mut source, record);
        clean(&source);
        if unavailable(error) {
            return;
        }
        assert_eq!(error, SearchError::Unavailable);
        assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        assert!(source.plaintext.as_ref().unwrap().is_empty());
    }
}

#[test]
fn invalid_projected_json_clears_the_temporary_before_reuse() {
    let vault = TestVault::new();
    let stored = secret();
    let record = key(&stored);
    vault.insert(stored);
    let calls = Arc::new(AtomicUsize::new(0));
    let decryptor = FakeDecryptor {
        calls: calls.clone(),
        action: |plaintext: &mut ProtectedBytes| {
            plaintext
                .try_extend_from_slice(
                    br#"{"schemaVersion":1,"kind":"login","fields":{"username":"partial"}"#,
                )
                .unwrap();
            Ok(())
        },
    };
    let mut source = VaultSearchSource::with_decryptor(vault.path.clone(), primary(), decryptor);
    for expected_calls in 1..=2 {
        let error = load_error(&mut source, record);
        clean(&source);
        if unavailable(error) {
            return;
        }
        assert_eq!(error, SearchError::InvalidInput);
        assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        assert!(source.plaintext.as_ref().unwrap().is_empty());
    }
}

#[test]
fn missing_vault_is_not_created_or_decrypted() {
    let vault = TestVault::new();
    let path = vault.directory.join("absent.sqlite3");
    let calls = Arc::new(AtomicUsize::new(0));
    let decryptor = FakeDecryptor {
        calls: calls.clone(),
        action: write_login,
    };
    let mut source = VaultSearchSource::with_decryptor(path.clone(), primary(), decryptor);
    assert_eq!(
        load_error(&mut source, key(&secret())),
        SearchError::Unavailable
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(source.plaintext.is_none());
    assert!(!path.exists());
}
