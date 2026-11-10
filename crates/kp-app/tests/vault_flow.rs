//! The vault from the application layer down: real SQLite store, real GnuPG,
//! real signatures. Only the person typing the passphrase is scripted.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test fixtures")]

#[path = "../../kp-keyring/tests/support/mod.rs"]
mod support;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use kryptopass_app::{VaultSession, candidates, catalog_records, credentials, qr_payload};
use kryptopass_core::vault::{
    FolderContents, FolderEdit, IconChoice, ItemEdit, NewFolder, NewItem, Recipients,
    TemplateRegistry, VaultError,
};
use kryptopass_core::{
    DiffTarget, FieldValue, FolderAppearance, ItemContent, KeyFingerprint, RecipientSet, SecretId,
    SensitiveText, Side,
};
use kryptopass_identity::{DeviceId, DeviceIdentity};
use kryptopass_keyring::Fingerprint;
use kryptopass_store::SqliteStore;
use kryptopass_sync::{Batch, SyncStore};
use support::GnupgHome;
use tempfile::TempDir;

/// The store refuses a directory other accounts can read.
fn private_directory() -> TempDir {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    directory
}

struct Device {
    home: GnupgHome,
    key: Fingerprint,
    files: TempDir,
    identity: Arc<DeviceIdentity>,
    templates: Arc<TemplateRegistry>,
}

impl Device {
    fn new() -> Self {
        let home = GnupgHome::new();
        let key = home.create("Ada <ada@example.org>");
        Self::with(home, key)
    }

    fn with(home: GnupgHome, key: Fingerprint) -> Self {
        Self {
            home,
            key,
            files: private_directory(),
            identity: Arc::new(DeviceIdentity::generate().unwrap()),
            templates: Arc::new(TemplateRegistry::builtin().unwrap()),
        }
    }

    fn path(&self) -> PathBuf {
        self.files.path().join("vault.sqlite")
    }

    fn session(&self) -> VaultSession {
        VaultSession::open(
            self.path(),
            self.home.keyring(),
            Arc::clone(&self.templates),
            Arc::clone(&self.identity),
        )
        .unwrap()
    }

    fn store(&self) -> SqliteStore {
        SqliteStore::open_existing(self.path(), Arc::clone(&self.identity)).unwrap()
    }

    fn recipients(&self) -> RecipientSet {
        RecipientSet::new(vec![KeyFingerprint::parse(self.key.as_ref()).unwrap()]).unwrap()
    }

    fn content(&self, template: &str, fields: &[(&str, &str)]) -> ItemContent {
        let template = self.templates.template(template).unwrap();
        let mut content = ItemContent::new(template);
        for (name, value) in fields {
            let field = template.field(name).unwrap();
            let value = SensitiveText::from((*value).to_owned());
            let value = if matches!(field.multiplicity, kp_template::Multiplicity::Single) {
                FieldValue::One(value)
            } else {
                FieldValue::Many(vec![value])
            };
            content.set(field.id.clone(), value);
        }
        content
    }

    fn create(&self, session: &mut VaultSession, title: &str, fields: &[(&str, &str)]) -> SecretId {
        let template = if fields.iter().any(|(name, _)| *name == "body") {
            "note"
        } else {
            "login"
        };
        session
            .vault_mut()
            .create_item(NewItem {
                folder: None,
                title: title.to_owned(),
                content: self.content(template, fields),
                recipients: Recipients::Explicit(self.recipients()),
                icon: IconChoice::Automatic,
                link: None,
            })
            .unwrap()
            .id
    }
}

fn text(content: &ItemContent, field: &str) -> String {
    content
        .value(field)
        .and_then(FieldValue::first)
        .map_or_else(String::new, |value| value.as_str().to_owned())
}

/// Hands every object `from` holds to `to`, as a Sync session would.
fn exchange(from: &Device, to: &Device) {
    let source = from.store();
    let mut target = to.store();
    let index = source.index().unwrap();
    let mut pending: Vec<_> = index.versions.into_iter().collect();
    while !pending.is_empty() {
        let before = pending.len();
        let mut later = Vec::new();
        for meta in pending {
            let ready = meta
                .parents
                .iter()
                .all(|parent| target.version(parent).unwrap().is_some());
            if !ready {
                later.push(meta);
                continue;
            }
            let version = source.version(&meta.version_id).unwrap().unwrap();
            let blobs = version
                .blobs()
                .filter_map(|hash| source.blob(hash).unwrap())
                .collect();
            target
                .apply(
                    Batch {
                        deletions: Vec::new(),
                        versions: vec![version],
                        blobs,
                    },
                    &DeviceId::from_bytes([0x99; 32]),
                )
                .unwrap();
        }
        assert!(later.len() < before, "parents must arrive first");
        pending = later;
    }
    for mark in source.index().unwrap().deletions {
        let deletion = source.deletion(&mark.record_id).unwrap().unwrap();
        target
            .apply(
                Batch {
                    deletions: vec![deletion],
                    ..Batch::default()
                },
                &DeviceId::from_bytes([0x99; 32]),
            )
            .unwrap();
    }
}

/// A second device with the same key, as after sending it to a phone.
fn second_device(first: &Device) -> Device {
    let home = GnupgHome::new();
    let secret = first.files.path().join("key.asc");
    first
        .home
        .keyring()
        .backup_secret_to(&first.key, &secret)
        .unwrap();
    home.keyring().import_file(&secret).unwrap();
    home.keyring().mark_as_own(&first.key).unwrap();
    Device::with(home, first.key.clone())
}

#[test]
fn an_item_is_created_read_edited_and_keeps_its_earlier_versions() {
    let device = Device::new();
    let mut session = device.session();
    let id = device.create(
        &mut session,
        "Correo",
        &[("username", "ada"), ("password", "uno")],
    );

    let opened = session.vault().open_item(id).unwrap();
    assert_eq!(opened.meta.state.title, "Correo");
    assert_eq!(text(&opened.content, "password"), "uno");
    assert_eq!(opened.meta.revision, 0);

    let edited = session
        .vault_mut()
        .update_item(
            ItemEdit::new(id, 0)
                .title("Correo personal")
                .content(device.content("login", &[("username", "ada"), ("password", "dos")])),
        )
        .unwrap();
    assert_eq!(edited.revision, 1);
    assert!(!edited.conflicted);

    let history = session.vault().item_history(id).unwrap();
    assert_eq!(history.len(), 2);
    let oldest = history
        .iter()
        .find(|version| version.parents.is_empty())
        .unwrap();
    let old = session.vault().open_version(id, oldest.id).unwrap();
    assert_eq!(text(&old.content, "password"), "uno");
    assert_eq!(old.meta.state.title, "Correo");
}

#[test]
fn an_edit_of_an_older_revision_is_refused() {
    let device = Device::new();
    let mut session = device.session();
    let id = device.create(&mut session, "Correo", &[("password", "uno")]);
    session
        .vault_mut()
        .update_item(ItemEdit::new(id, 0).favorite(true))
        .unwrap();
    let stale = session
        .vault_mut()
        .update_item(ItemEdit::new(id, 0).title("Otro"));
    assert!(matches!(stale, Err(VaultError::StaleRevision)));
}

#[test]
fn title_and_content_never_reach_the_signed_objects_in_clear() {
    let device = Device::new();
    let mut session = device.session();
    let id = device.create(
        &mut session,
        "TituloSecreto",
        &[("password", "ClaveSecreta")],
    );
    let store = device.store();
    let version = store.version(id.0.as_bytes()).unwrap();
    assert!(version.is_none(), "a record id is not a version id");
    let meta = store.index().unwrap().versions.remove(0);
    let version = store.version(&meta.version_id).unwrap().unwrap();
    let contains = |haystack: &[u8], needle: &[u8]| {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    };
    assert!(!contains(&version.state, b"TituloSecreto"));
    let blob = store.blob(&version.content_hash.unwrap()).unwrap().unwrap();
    assert!(!contains(&blob, b"ClaveSecreta"));
}

#[test]
fn changing_only_the_state_does_not_rewrite_the_content() {
    let device = Device::new();
    let mut session = device.session();
    let id = device.create(&mut session, "Correo", &[("password", "uno")]);
    session
        .vault_mut()
        .update_item(ItemEdit::new(id, 0).favorite(true).title("Renombrado"))
        .unwrap();
    let store = device.store();
    let hashes: Vec<_> = store
        .index()
        .unwrap()
        .versions
        .into_iter()
        .map(|meta| meta.blobs)
        .collect();
    assert_eq!(hashes.len(), 2);
    assert_eq!(hashes[0], hashes[1]);
}

#[test]
fn deleting_for_good_leaves_only_a_mark_and_old_versions_cannot_return() {
    let device = Device::new();
    let mut session = device.session();
    let id = device.create(&mut session, "Correo", &[("password", "uno")]);
    let keep = device.create(&mut session, "Otro", &[("password", "dos")]);
    let copy = {
        let store = device.store();
        let meta = store
            .index()
            .unwrap()
            .versions
            .into_iter()
            .find(|meta| meta.record_id == *id.0.as_bytes())
            .unwrap();
        let version = store.version(&meta.version_id).unwrap().unwrap();
        let blob = store.blob(&version.content_hash.unwrap()).unwrap().unwrap();
        (version, blob)
    };
    session.vault_mut().remove_item(id, 0).unwrap();

    assert!(matches!(
        session.vault().open_item(id),
        Err(VaultError::ItemNotFound)
    ));
    assert!(session.vault().open_item(keep).is_ok());
    let mut store = device.store();
    assert!(store.deletion(id.0.as_bytes()).unwrap().is_some());
    assert!(store.blob(&copy.0.content_hash.unwrap()).unwrap().is_none());

    let outcome = store
        .apply(
            Batch {
                deletions: Vec::new(),
                versions: vec![copy.0],
                blobs: vec![copy.1],
            },
            &DeviceId::from_bytes([0x99; 32]),
        )
        .unwrap();
    assert_eq!(outcome.inserted, 0);
    assert_eq!(outcome.skipped, 1);
}

#[test]
fn folders_nest_rename_and_hand_their_contents_to_the_parent() {
    let device = Device::new();
    let mut session = device.session();
    session
        .vault_mut()
        .set_default_recipients(Some(device.recipients()))
        .unwrap();
    let appearance =
        || FolderAppearance::new("folder".to_owned(), "transparent".to_owned()).unwrap();
    let parent = session
        .vault_mut()
        .create_folder(NewFolder {
            name: "Trabajo".to_owned(),
            parent: None,
            appearance: appearance(),
            default_recipients: None,
        })
        .unwrap()
        .id;
    let child = session
        .vault_mut()
        .create_folder(NewFolder {
            name: "Clientes".to_owned(),
            parent: Some(parent),
            appearance: appearance(),
            default_recipients: None,
        })
        .unwrap()
        .id;
    let id = device.create(&mut session, "Correo", &[("password", "uno")]);
    session
        .vault_mut()
        .update_item(ItemEdit::new(id, 0).folder(Some(child)))
        .unwrap();
    session
        .vault_mut()
        .update_folder(FolderEdit::new(child).name("Cuentas"))
        .unwrap();
    assert!(matches!(
        session.vault_mut().move_folder(parent, Some(child), None),
        Err(VaultError::FolderCycle)
    ));

    session
        .vault_mut()
        .remove_folder(child, FolderContents::Reparent)
        .unwrap();
    let item = session.vault().open_item(id).unwrap();
    assert_eq!(item.meta.state.folder, Some(parent));
    assert_eq!(session.vault().list_folders().unwrap().len(), 1);
}

#[test]
fn another_device_with_the_key_reads_what_it_receives() {
    let first = Device::new();
    let second = second_device(&first);
    let mut session = first.session();
    let id = first.create(&mut session, "Correo", &[("password", "uno")]);

    let mut other = second.session();
    let trust_free = other.vault().list_items().unwrap();
    assert!(trust_free.is_empty());
    exchange(&first, &second);
    assert_eq!(other.vault_mut().reindex().unwrap(), 1);

    let item = other.vault().open_item(id).unwrap();
    assert_eq!(item.meta.state.title, "Correo");
    assert_eq!(text(&item.content, "password"), "uno");
}

#[test]
fn without_the_key_an_item_is_kept_and_listed_as_unreadable() {
    let first = Device::new();
    let stranger = Device::new();
    let mut session = first.session();
    let id = first.create(&mut session, "Correo", &[("password", "uno")]);
    let mut other = stranger.session();
    exchange(&first, &stranger);

    assert_eq!(other.vault_mut().reindex().unwrap(), 0);
    assert!(other.vault().list_items().unwrap().is_empty());
    let opaque = other.vault().opaque_items().unwrap();
    assert_eq!(opaque.len(), 1);
    assert_eq!(opaque[0].id, id);
    assert_eq!(
        stranger.store().object_count().unwrap(),
        1,
        "it still travels on"
    );
}

#[test]
fn diverging_edits_become_a_conflict_the_person_settles_field_by_field() {
    let first = Device::new();
    let second = second_device(&first);
    let mut session = first.session();
    let id = first.create(
        &mut session,
        "Correo",
        &[("username", "ada"), ("password", "uno")],
    );
    let mut other = second.session();
    exchange(&first, &second);
    other.vault_mut().reindex().unwrap();

    session
        .vault_mut()
        .update_item(
            ItemEdit::new(id, 0)
                .content(first.content("login", &[("username", "ada"), ("password", "aqui")])),
        )
        .unwrap();
    other
        .vault_mut()
        .update_item(
            ItemEdit::new(id, 0)
                .title("Renombrado")
                .content(first.content("login", &[("username", "ada"), ("password", "alla")])),
        )
        .unwrap();
    exchange(&second, &first);
    session.vault_mut().reindex().unwrap();

    let conflict = session
        .vault()
        .open_conflict(id)
        .unwrap()
        .expect("conflict");
    let password = conflict
        .differences
        .iter()
        .find(|difference| matches!(&difference.target, DiffTarget::Field(field) if field.as_str() == "password"))
        .expect("the password differs");
    assert!(
        password.suggested.is_none(),
        "a concealed conflict is never preselected"
    );

    let refused = session
        .vault_mut()
        .resolve_conflict(&conflict, &HashMap::new());
    assert!(matches!(refused, Err(VaultError::ConflictUnresolved)));

    let choices = HashMap::from([(password.target.clone(), Side::Theirs)]);
    let merged = session
        .vault_mut()
        .resolve_conflict(&conflict, &choices)
        .unwrap();
    assert!(!merged.conflicted);
    assert_eq!(
        merged.state.title, "Renombrado",
        "a one-sided change is taken"
    );
    let item = session.vault().open_item(id).unwrap();
    assert_eq!(text(&item.content, "password"), "alla");
    assert_eq!(session.vault().item_heads(id).unwrap().len(), 1);
    assert!(session.vault().open_conflict(id).unwrap().is_none());
}

#[test]
fn the_browser_only_sees_fillable_items_at_the_revision_it_was_offered() {
    let device = Device::new();
    let mut session = device.session();
    let login = device.create(
        &mut session,
        "Correo",
        &[
            ("username", "ada"),
            ("password", "uno"),
            ("urls", "https://example.org"),
        ],
    );
    device.create(&mut session, "Nota", &[("body", "texto")]);
    let trashed = device.create(
        &mut session,
        "Vieja",
        &[("password", "dos"), ("urls", "https://a.example")],
    );
    session
        .vault_mut()
        .update_item(ItemEdit::new(trashed, 0).trashed(true))
        .unwrap();

    let offered = candidates(session.vault()).unwrap();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].id, login);
    let values = credentials(session.vault(), &offered[0]).unwrap();
    assert_eq!(values.username.unwrap().as_str(), "ada");
    assert_eq!(values.password.unwrap().as_str(), "uno");
    assert_eq!(values.urls[0].as_str(), "https://example.org");

    session
        .vault_mut()
        .update_item(ItemEdit::new(login, 0).favorite(true))
        .unwrap();
    assert!(credentials(session.vault(), &offered[0]).is_err());
}

#[test]
fn search_lists_titles_and_only_loads_what_a_template_indexes() {
    let device = Device::new();
    let mut session = device.session();
    device.create(
        &mut session,
        "Correo",
        &[("username", "ada"), ("password", "uno")],
    );
    let items = session.vault().list_items().unwrap();
    let records = catalog_records(&items, session.templates());
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].public_fields, vec!["Correo".to_owned()]);
    assert!(records[0].private_eligible);
}

#[test]
fn a_wifi_item_offers_its_qr_and_a_login_does_not() {
    let device = Device::new();
    let wifi = device.content(
        "wifi",
        &[
            ("ssid", "Casa"),
            ("security", "wpa"),
            ("password", "clave-larga"),
        ],
    );
    let template = device.templates.template("wifi").unwrap();
    let payload = qr_payload(template, &wifi).unwrap().unwrap();
    assert!(payload.as_str().starts_with("WIFI:"));
    let login = device.templates.template("login").unwrap();
    assert!(
        qr_payload(login, &device.content("login", &[]))
            .unwrap()
            .is_none()
    );
}

#[test]
fn locking_forgets_the_passphrase_of_keys_set_to_forget() {
    let device = Device::new();
    let mut session = device.session();
    let key = KeyFingerprint::parse(device.key.as_ref()).unwrap();
    let mut settings = session.vault().settings_of(&key).unwrap();
    settings.forget_on_lock = true;
    session.vault_mut().set_key_settings(settings).unwrap();
    session.lock().unwrap();
    let session = device.session();
    assert!(session.vault().settings_of(&key).unwrap().forget_on_lock);
}
