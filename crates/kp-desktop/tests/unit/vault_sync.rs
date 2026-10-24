#![allow(clippy::expect_used)] // In-memory identity and ledger fixtures must be explicit.

use super::{SyncedVault, VaultSyncError};
use anyhow::bail;
use kryptopass_core::{
    KeyFingerprint, LoginContent, MAX_VAULT_OBJECT_BYTES, NoteContent, OpenPgp, RecipientSet,
    SecretContent, SecretId, SensitiveBytes, VaultObject, decode_vault_object, encode_vault_object,
};
use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_sync::{Id, Ledger, Operation, Revision, SyncError, TrustSource, fresh_id};
use std::collections::HashMap;

const VAULT_ID: Id = [0x51; 16];
const FINGERPRINT: &str = "0123456789ABCDEF0123456789ABCDEF01234567";
const FAKE_OPENPGP_PREFIX: &[u8] = b"synthetic-openpgp-v1:";
const FAKE_OPENPGP_MASK: u8 = 0xa5;

/// Deterministic reversible crypto used only with synthetic test payloads.
struct FakeOpenPgp;

impl OpenPgp for FakeOpenPgp {
    fn validate_recipients(&self, recipients: &RecipientSet) -> anyhow::Result<()> {
        if recipients.as_slice().is_empty() {
            bail!("synthetic encryption requires a recipient");
        }
        Ok(())
    }

    fn encrypt(&self, plaintext: &[u8], recipients: &RecipientSet) -> anyhow::Result<Vec<u8>> {
        self.validate_recipients(recipients)?;
        let mut ciphertext = FAKE_OPENPGP_PREFIX.to_vec();
        ciphertext.extend(plaintext.iter().map(|byte| *byte ^ FAKE_OPENPGP_MASK));
        Ok(ciphertext)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> anyhow::Result<SensitiveBytes> {
        let Some(encoded) = ciphertext.strip_prefix(FAKE_OPENPGP_PREFIX) else {
            bail!("not a synthetic OpenPGP payload");
        };
        Ok(SensitiveBytes::from(
            encoded
                .iter()
                .map(|byte| *byte ^ FAKE_OPENPGP_MASK)
                .collect::<Vec<_>>(),
        ))
    }
}

#[derive(Default)]
struct MemoryTrust(HashMap<DeviceId, PublicIdentity>);

impl MemoryTrust {
    fn add(&mut self, identity: &DeviceIdentity) {
        self.0
            .insert(identity.device_id(), identity.public_identity());
    }
}

impl TrustSource for MemoryTrust {
    fn lookup(
        &self,
        device_id: DeviceId,
    ) -> std::result::Result<Option<PublicIdentity>, SyncError> {
        Ok(self.0.get(&device_id).copied())
    }
}

struct Fixture {
    local: DeviceIdentity,
    peer: DeviceIdentity,
    trust: MemoryTrust,
    crypto: FakeOpenPgp,
    fingerprint: KeyFingerprint,
}

impl Fixture {
    fn new() -> Self {
        let local = DeviceIdentity::from_seed_material(&[0x31; 32], &[0x41; 32]);
        let peer = DeviceIdentity::from_seed_material(&[0x32; 32], &[0x42; 32]);
        let mut trust = MemoryTrust::default();
        trust.add(&local);
        trust.add(&peer);
        Self {
            local,
            peer,
            trust,
            crypto: FakeOpenPgp,
            fingerprint: KeyFingerprint::parse(FINGERPRINT).expect("synthetic fingerprint"),
        }
    }

    fn ledger(&self) -> Ledger {
        Ledger::in_memory(VAULT_ID).expect("isolated in-memory ledger")
    }

    fn vault<'a>(&'a self, ledger: &'a mut Ledger) -> SyncedVault<'a> {
        SyncedVault::new(
            ledger,
            self.fingerprint.clone(),
            &self.local,
            &self.trust,
            &self.crypto,
        )
        .expect("trusted local identity binds to the ledger")
    }
}

fn record_bytes(id: SecretId) -> Id {
    *id.0.as_bytes()
}

fn synthetic_login(username: &str, password: &str) -> SecretContent {
    SecretContent::Login(LoginContent {
        username: username.into(),
        password: password.into(),
        notes: Default::default(),
        urls: Vec::new(),
        totp: None,
        history: Box::default(),
    })
}

#[test]
fn save_requires_exact_current_heads_and_lists_conflicts_and_tombstones() {
    let fixture = Fixture::new();
    let mut ledger = fixture.ledger();
    let login_id = SecretId::new();
    let note_id = SecretId::new();

    let root = {
        let mut vault = fixture.vault(&mut ledger);
        vault
            .save(
                login_id,
                &[],
                "Synthetic login".to_owned(),
                false,
                false,
                synthetic_login("alice@example.invalid", "password-v1"),
            )
            .expect("create initial login revision")
    };
    let child = {
        let mut vault = fixture.vault(&mut ledger);
        let child = vault
            .save(
                login_id,
                &[root.revision_id],
                "Updated synthetic login".to_owned(),
                true,
                false,
                synthetic_login("alice@example.invalid", "password-v2"),
            )
            .expect("save using the exact current head");
        assert!(matches!(
            vault.save(
                login_id,
                &[root.revision_id],
                "Stale synthetic login".to_owned(),
                false,
                false,
                synthetic_login("alice@example.invalid", "stale-password"),
            ),
            Err(VaultSyncError::Conflict)
        ));
        child
    };
    let note_tombstone = {
        let mut vault = fixture.vault(&mut ledger);
        let note = vault
            .save(
                note_id,
                &[],
                "Disposable synthetic note".to_owned(),
                false,
                false,
                SecretContent::Note(NoteContent {
                    value: "synthetic note".into(),
                    notes: "synthetic metadata".into(),
                }),
            )
            .expect("create note revision");
        vault
            .delete(note_id, &[note.revision_id])
            .expect("append explicit tombstone")
    };
    assert_eq!(note_tombstone.operation, Operation::Delete);

    // Add a separately signed peer edit from the original root to form two
    // concurrent current heads. Both use synthetic encrypted content only.
    let branch_content = synthetic_login("peer@example.invalid", "peer-password");
    let branch_object = VaultObject {
        record_id: login_id,
        key_fingerprint: fixture.fingerprint.clone(),
        title: "Peer branch".to_owned(),
        favorite: false,
        trashed: false,
        content: branch_content,
    };
    let branch_plaintext = encode_vault_object(&branch_object).expect("valid branch object");
    let recipients =
        RecipientSet::new(vec![fixture.fingerprint.clone()]).expect("one synthetic recipient");
    let branch_ciphertext = fixture
        .crypto
        .encrypt(branch_plaintext.as_slice(), &recipients)
        .expect("synthetic peer encryption succeeds");
    let branch = Revision::sign(
        VAULT_ID,
        record_bytes(login_id),
        fresh_id().expect("fresh synthetic revision id"),
        vec![root.revision_id],
        Operation::Write,
        branch_ciphertext,
        &fixture.peer,
    )
    .expect("peer signs its branch");
    let branch_id = branch.revision_id;
    ledger
        .insert_batch(&[branch], &fixture.trust)
        .expect("trusted peer branch is accepted");

    let mut vault = fixture.vault(&mut ledger);
    assert!(matches!(
        vault.save(
            login_id,
            &[child.revision_id],
            "Cannot choose one conflict branch".to_owned(),
            false,
            false,
            synthetic_login("alice@example.invalid", "must-not-commit"),
        ),
        Err(VaultSyncError::Conflict)
    ));

    let projections = vault.list().expect("list every current revision head");
    let mut login_heads = projections
        .iter()
        .filter(|projection| projection.record_id == login_id)
        .map(|projection| projection.revision_id)
        .collect::<Vec<_>>();
    login_heads.sort_unstable();
    let mut expected_login_heads = vec![child.revision_id, branch_id];
    expected_login_heads.sort_unstable();
    assert_eq!(login_heads, expected_login_heads);

    let peer_projection = projections
        .iter()
        .find(|projection| projection.revision_id == branch_id)
        .expect("peer branch remains visible");
    let peer_object = peer_projection
        .object
        .as_ref()
        .expect("write head has content");
    assert_eq!(peer_object.title, "Peer branch");

    let tombstone = projections
        .iter()
        .find(|projection| projection.revision_id == note_tombstone.revision_id)
        .expect("delete head remains visible");
    assert_eq!(tombstone.operation, Operation::Delete);
    assert!(tombstone.object.is_none());

    let child_projection = projections
        .iter()
        .find(|projection| projection.revision_id == child.revision_id)
        .expect("local branch remains visible");
    assert_eq!(child_projection.parent_revision_ids, vec![root.revision_id]);
    assert_eq!(child_projection.operation, Operation::Write);
    let child_object = child_projection
        .object
        .as_ref()
        .expect("write head has content");
    assert_eq!(child_object.title, "Updated synthetic login");
    let SecretContent::Login(login) = &child_object.content else {
        panic!("saved revision should retain login content");
    };
    assert_eq!(login.password.as_str(), "password-v2");

    // Keep the codec in the path exercised by list() and ensure it bound the
    // branch to the expected record and synthetic key fingerprint.
    let decoded = decode_vault_object(
        encode_vault_object(peer_object)
            .expect("re-encode listed branch")
            .as_slice(),
        login_id,
        &fixture.fingerprint,
    )
    .expect("listed branch remains bound to its record and key");
    assert_eq!(decoded.title, "Peer branch");
}

#[test]
fn list_rejects_aggregate_plaintext_over_eight_mib_without_partial_projection() {
    const RECORD_COUNT: usize = 17;
    const FIELD_BYTES: usize = 256 * 1024;

    let fixture = Fixture::new();
    let mut ledger = fixture.ledger();
    {
        let mut vault = fixture.vault(&mut ledger);
        for _ in 0..RECORD_COUNT {
            vault
                .save(
                    SecretId::new(),
                    &[],
                    "Synthetic large note".to_owned(),
                    false,
                    false,
                    SecretContent::Note(NoteContent {
                        value: "v".repeat(FIELD_BYTES).into(),
                        notes: "n".repeat(FIELD_BYTES).into(),
                    }),
                )
                .expect("save a valid synthetic vault-object wrapper");
        }
    }

    let heads = ledger.manifest().expect("manifest of synthetic heads");
    assert_eq!(heads.len(), RECORD_COUNT);
    let mut total_plaintext_bytes = 0usize;
    for head in heads {
        let revision = ledger
            .get(head.revision_id)
            .expect("read synthetic revision")
            .expect("head revision exists");
        let plaintext = fixture
            .crypto
            .decrypt(&revision.ciphertext)
            .expect("decrypt synthetic vault object");
        assert!(plaintext.len() < MAX_VAULT_OBJECT_BYTES);
        total_plaintext_bytes += plaintext.len();
    }
    assert!(total_plaintext_bytes > 8 * 1024 * 1024);

    let vault = fixture.vault(&mut ledger);
    assert!(matches!(vault.list(), Err(VaultSyncError::LimitExceeded)));
}
