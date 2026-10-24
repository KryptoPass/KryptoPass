#![allow(clippy::unwrap_used)]

use std::cell::Cell;
use std::rc::Rc;

use anyhow::Result;
use kryptopass_core::SensitiveBytes;
use kryptopass_core::{
    Folder, FolderId, KeyFingerprint, LoginContent, LoginHistoryEventKind, NoteContent, OpenPgp,
    RecipientSelection, RecipientSet, Secret, SecretContent, SecretId, SecretKind, SecretMetadata,
    SecretsService, Storage, encode_content,
};

#[derive(Default)]
struct Calls {
    encryptions: Cell<usize>,
    replacements: Cell<usize>,
}

struct SingleSecretStorage {
    secret: Secret,
    calls: Rc<Calls>,
}

impl Storage for SingleSecretStorage {
    fn list_folders(&self) -> Result<Vec<Folder>> {
        unreachable!("folder operations are outside this scenario")
    }

    fn list_secrets(&self) -> Result<Vec<Secret>> {
        Ok(vec![self.secret.clone()])
    }

    fn list_secret_metadata(&self) -> Result<Vec<SecretMetadata>> {
        unreachable!("metadata listing is outside this scenario")
    }

    fn find_folder(&self, _: FolderId) -> Result<Option<Folder>> {
        unreachable!("folder operations are outside this scenario")
    }

    fn find_secret(&self, id: SecretId) -> Result<Option<Secret>> {
        Ok((self.secret.id == id).then(|| self.secret.clone()))
    }

    fn insert_folder(&mut self, _: Folder) -> Result<()> {
        unreachable!("folder operations are outside this scenario")
    }

    fn insert_secret(&mut self, _: Secret) -> Result<()> {
        unreachable!("creation is outside this edit scenario")
    }

    fn replace_secret(&mut self, mut secret: Secret, expected_revision: u64) -> Result<()> {
        self.calls
            .replacements
            .set(self.calls.replacements.get() + 1);
        anyhow::ensure!(
            self.secret.id == secret.id && self.secret.revision == expected_revision,
            "stale secret revision"
        );
        secret.revision = expected_revision + 1;
        self.secret = secret;
        Ok(())
    }

    fn replace_folder(&mut self, _: Folder) -> Result<()> {
        unreachable!("folder operations are outside this scenario")
    }

    fn move_folder(&mut self, _: FolderId, _: Option<FolderId>, _: Option<FolderId>) -> Result<()> {
        unreachable!("folder operations are outside this scenario")
    }

    fn remove_secret(&mut self, _: SecretId) -> Result<()> {
        unreachable!("removal is outside this scenario")
    }

    fn remove_folder(&mut self, _: FolderId) -> Result<()> {
        unreachable!("folder operations are outside this scenario")
    }

    fn move_secret(&mut self, _: SecretId, _: Option<FolderId>) -> Result<()> {
        unreachable!("moving is outside this scenario")
    }

    fn replace_encryption(
        &mut self,
        _: SecretId,
        _: Vec<u8>,
        _: RecipientSet,
        _: u64,
    ) -> Result<()> {
        unreachable!("recipient rotation is outside this scenario")
    }
}

struct ReversibleTestCrypto(Rc<Calls>);

fn transform(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(|byte| byte ^ 0xa5).collect()
}

impl OpenPgp for ReversibleTestCrypto {
    fn validate_recipients(&self, _: &RecipientSet) -> Result<()> {
        Ok(())
    }

    fn encrypt(&self, plaintext: &[u8], _: &RecipientSet) -> Result<Vec<u8>> {
        self.0.encryptions.set(self.0.encryptions.get() + 1);
        Ok(transform(plaintext))
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<SensitiveBytes> {
        Ok(transform(ciphertext).into())
    }
}

struct Fixture {
    service: SecretsService<SingleSecretStorage, ReversibleTestCrypto>,
    calls: Rc<Calls>,
}

impl Fixture {
    fn new(content: SecretContent) -> Self {
        let calls = Rc::new(Calls::default());
        let plaintext = encode_content(&content).unwrap();
        let secret = Secret {
            icon: None,
            id: SecretId::new(),
            folder_id: None,
            title: "Original title".into(),
            kind: content.kind(),
            recipients: RecipientSet::new(vec![
                KeyFingerprint::parse("0123456789ABCDEF0123456789ABCDEF01234567").unwrap(),
            ])
            .unwrap(),
            ciphertext: transform(plaintext.as_slice()),
            favorite: true,
            trashed: false,
            revision: 7,
        };
        let storage = SingleSecretStorage {
            secret,
            calls: Rc::clone(&calls),
        };
        Self {
            service: SecretsService::new(storage, ReversibleTestCrypto(Rc::clone(&calls))),
            calls,
        }
    }

    fn stored(&self) -> Secret {
        self.service.list_secrets().unwrap().remove(0)
    }

    fn save_edit(&mut self, content: SecretContent) -> Result<SecretId> {
        let original = self.stored();
        self.service.save_content(
            Some(original.id),
            original.folder_id,
            "Edited title".into(),
            RecipientSelection::Explicit(original.recipients),
            content,
            Some(original.revision),
        )
    }

    fn assert_rejected_kind_change(&mut self, replacement: SecretContent) {
        let original = self.stored();
        let original_content = self.service.read_content(original.id).unwrap();
        let original_plaintext = encode_content(&original_content).unwrap();

        assert!(self.save_edit(replacement).is_err());

        assert_eq!(self.calls.encryptions.get(), 0);
        assert_eq!(self.calls.replacements.get(), 0);
        assert_eq!(self.stored(), original);
        let current_content = self.service.read_content(original.id).unwrap();
        let current_plaintext = encode_content(&current_content).unwrap();
        assert_eq!(current_plaintext.as_slice(), original_plaintext.as_slice());
    }
}

fn sample_login() -> LoginContent {
    let previous = LoginContent {
        username: "user@example.org".into(),
        password: "previous password".into(),
        urls: vec!["https://example.org/login".into()],
        notes: "Recovery information".into(),
        totp: None,
        history: Box::default(),
    };
    let mut current = previous.clone();
    current.password = "current password".into();
    current
        .history
        .record_event(LoginHistoryEventKind::Create, 5);
    current
        .history
        .record_version(&previous, "Historical title", 20)
        .unwrap();
    current
}

fn sample_note() -> NoteContent {
    NoteContent {
        value: "Private note content".into(),
        notes: "Additional private context".into(),
    }
}

#[test]
fn login_cannot_be_replaced_by_note_or_lose_its_history() {
    let mut fixture = Fixture::new(SecretContent::Login(sample_login()));

    fixture.assert_rejected_kind_change(SecretContent::Note(sample_note()));

    let content = fixture.service.read_content(fixture.stored().id).unwrap();
    let SecretContent::Login(login) = &content else {
        panic!("expected original login content");
    };
    assert_eq!(login.history.version_metadata(), vec![20]);
    assert_eq!(login.history.version_title(0), Some("Historical title"));
    assert_eq!(
        login.history.version_content(0).unwrap().password,
        "previous password"
    );
    assert_eq!(login.history.events().len(), 1);
    assert_eq!(login.history.events()[0].timestamp, 5);
    assert!(login.history.events()[0].kind == LoginHistoryEventKind::Create);
}

#[test]
fn note_cannot_be_replaced_by_login() {
    let mut fixture = Fixture::new(SecretContent::Note(sample_note()));

    fixture.assert_rejected_kind_change(SecretContent::Login(sample_login()));
}

#[test]
fn login_edit_keeps_its_type_and_retains_previous_history() {
    let mut fixture = Fixture::new(SecretContent::Login(sample_login()));
    let original = fixture.stored();
    let mut updated = sample_login();
    updated.username = "updated@example.org".into();
    updated.password = "updated password".into();
    updated.history = Box::default();

    assert_eq!(
        fixture.save_edit(SecretContent::Login(updated)).unwrap(),
        original.id
    );

    let stored = fixture.stored();
    assert_eq!(stored.kind, SecretKind::Login);
    assert_eq!(stored.title, "Edited title");
    assert_eq!(stored.revision, original.revision + 1);
    assert_ne!(stored.ciphertext, original.ciphertext);
    assert!(stored.favorite);
    assert_eq!(fixture.calls.encryptions.get(), 1);
    assert_eq!(fixture.calls.replacements.get(), 1);
    let content = fixture.service.read_content(original.id).unwrap();
    let SecretContent::Login(login) = &content else {
        panic!("expected edited login content");
    };
    assert_eq!(login.username, "updated@example.org");
    assert_eq!(login.password, "updated password");
    assert_eq!(login.history.version_metadata().len(), 2);
    assert_eq!(login.history.version_title(0), Some("Original title"));
    assert_eq!(
        login.history.version_content(0).unwrap().password,
        "current password"
    );
    assert_eq!(login.history.version_title(1), Some("Historical title"));
    assert_eq!(
        login.history.version_content(1).unwrap().password,
        "previous password"
    );
    assert!(
        login
            .history
            .events()
            .iter()
            .any(|event| { event.timestamp == 5 && event.kind == LoginHistoryEventKind::Create })
    );
}

#[test]
fn note_edit_keeps_its_type_and_updates_its_content() {
    let mut fixture = Fixture::new(SecretContent::Note(sample_note()));
    let original = fixture.stored();

    assert_eq!(
        fixture
            .save_edit(SecretContent::Note(NoteContent {
                value: "Updated private note".into(),
                notes: "Updated context".into(),
            }))
            .unwrap(),
        original.id
    );

    let stored = fixture.stored();
    assert_eq!(stored.kind, SecretKind::SecureNote);
    assert_eq!(stored.title, "Edited title");
    assert_eq!(stored.revision, original.revision + 1);
    assert_ne!(stored.ciphertext, original.ciphertext);
    assert!(stored.favorite);
    assert_eq!(fixture.calls.encryptions.get(), 1);
    assert_eq!(fixture.calls.replacements.get(), 1);
    let content = fixture.service.read_content(original.id).unwrap();
    let SecretContent::Note(note) = &content else {
        panic!("expected edited note content");
    };
    assert_eq!(note.value, "Updated private note");
    assert_eq!(note.notes, "Updated context");
}
