#![allow(clippy::unwrap_used)]

use super::*;
use std::path::PathBuf;

const RECIPIENT: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("kryptopass-entry-links-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }

    fn store(&self, name: &str) -> SqliteStorage {
        SqliteStorage::open(self.0.join(name)).unwrap()
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn secret(id: SecretId, title: &str) -> Secret {
    Secret {
        icon: None,
        id,
        folder_id: None,
        title: title.to_owned(),
        kind: SecretKind::SecureNote,
        recipients: RecipientSet::new(vec![KeyFingerprint::parse(RECIPIENT).unwrap()]).unwrap(),
        ciphertext: vec![0xc3, 0x03, 0x01],
        favorite: false,
        trashed: false,
        revision: 0,
    }
}

fn pair(left: SecretId, right: SecretId) -> SecretLink {
    if left.0 < right.0 {
        SecretLink {
            first: left,
            second: right,
        }
    } else {
        SecretLink {
            first: right,
            second: left,
        }
    }
}

fn revision(entries: &[SecretMetadata], id: SecretId) -> u64 {
    entries
        .iter()
        .find(|entry| entry.id == id)
        .unwrap()
        .revision
}

#[test]
fn pair_changes_persist_as_one_link_and_update_survivor_revision_on_delete() {
    let directory = TestDirectory::new();
    let path = directory.0.join("store.sqlite3");
    let first = SecretId::new();
    let second = SecretId::new();
    let third = SecretId::new();
    let mut storage = SqliteStorage::open(&path).unwrap();

    storage.insert_secret(secret(first, "First")).unwrap();
    storage
        .insert_secret_with_link(secret(second, "Second"), None, Some(first))
        .unwrap();
    storage.insert_secret(secret(third, "Third")).unwrap();

    let (_, metadata, links) = storage.read_snapshot_with_links().unwrap();
    assert_eq!(links, vec![pair(first, second)]);
    assert_eq!(revision(&metadata, first), 1);
    assert_eq!(revision(&metadata, second), 0);

    let mut trashed_companion = storage.find_secret(first).unwrap().unwrap();
    trashed_companion.trashed = true;
    storage.replace_secret(trashed_companion, 1).unwrap();
    let (_, metadata, links) = storage.read_snapshot_with_links().unwrap();
    assert_eq!(links, vec![pair(first, second)]);
    assert_eq!(revision(&metadata, first), 2);

    let mut same_pair_save = storage.find_secret(second).unwrap().unwrap();
    same_pair_save.title = "Second saved".to_owned();
    storage
        .replace_secret_with_link(same_pair_save, 0, None, Some(first))
        .unwrap();
    let (_, metadata, links) = storage.read_snapshot_with_links().unwrap();
    assert_eq!(links, vec![pair(first, second)]);
    assert_eq!(revision(&metadata, first), 2);
    assert_eq!(revision(&metadata, second), 1);

    let mut edited_second = storage.find_secret(second).unwrap().unwrap();
    edited_second.title = "Second updated".to_owned();
    storage
        .replace_secret_with_link(edited_second, 1, None, Some(third))
        .unwrap();
    let (_, metadata, links) = storage.read_snapshot_with_links().unwrap();
    assert_eq!(links, vec![pair(second, third)]);
    assert_eq!(revision(&metadata, first), 3);
    assert_eq!(revision(&metadata, second), 2);
    assert_eq!(revision(&metadata, third), 1);

    drop(storage);
    let mut storage = SqliteStorage::open_existing(path).unwrap();
    let (_, metadata, links) = storage.read_snapshot_with_links().unwrap();
    assert_eq!(links, vec![pair(second, third)]);
    assert_eq!(revision(&metadata, second), 2);
    assert_eq!(revision(&metadata, third), 1);

    storage.remove_secret(second).unwrap();
    let (_, metadata, links) = storage.read_snapshot_with_links().unwrap();
    assert!(links.is_empty());
    assert_eq!(revision(&metadata, first), 3);
    assert_eq!(revision(&metadata, third), 2);
    assert!(storage.find_secret(second).unwrap().is_none());
}

#[test]
fn linked_writes_reject_self_missing_and_occupied_replacement_atomically() {
    let directory = TestDirectory::new();
    let mut storage = directory.store("store.sqlite3");
    let first = SecretId::new();
    let second = SecretId::new();
    let third = SecretId::new();
    storage.insert_secret(secret(first, "First")).unwrap();
    storage
        .insert_secret_with_link(secret(second, "Second"), None, Some(first))
        .unwrap();
    storage.insert_secret(secret(third, "Third")).unwrap();

    let third_before = storage.find_secret(third).unwrap().unwrap();
    let mut attempted_third = third_before.clone();
    attempted_third.title = "Should roll back".to_owned();
    attempted_third.ciphertext = vec![0xc3, 0x03, 0x99];
    assert!(
        storage
            .replace_secret_with_link(attempted_third, 0, None, Some(second))
            .is_err()
    );
    assert_eq!(storage.find_secret(third).unwrap().unwrap(), third_before);

    let self_id = SecretId::new();
    assert!(
        storage
            .insert_secret_with_link(secret(self_id, "Self linked"), None, Some(self_id))
            .is_err()
    );
    let missing_id = SecretId::new();
    assert!(
        storage
            .insert_secret_with_link(
                secret(missing_id, "Missing target"),
                None,
                Some(SecretId::new()),
            )
            .is_err()
    );

    let (_, metadata, links) = storage.read_snapshot_with_links().unwrap();
    assert_eq!(links, vec![pair(first, second)]);
    assert_eq!(metadata.len(), 3);
    assert_eq!(revision(&metadata, first), 1);
    assert_eq!(revision(&metadata, second), 0);
    assert_eq!(revision(&metadata, third), 0);
    for rejected_id in [self_id, missing_id] {
        assert!(storage.find_secret(rejected_id).unwrap().is_none());
    }
}

#[test]
fn additive_import_stores_companion_links_and_rejects_reused_endpoints() {
    use kryptopass_core::{ImportStorage, Storage};

    let directory = TestDirectory::new();
    let mut storage = directory.store("import-links.sqlite3");
    let login = secret(SecretId::new(), "Imported login");
    let companion = secret(SecretId::new(), "Imported login");
    let link = pair(login.id, companion.id);
    storage
        .append_import(&[], &[login, companion], &[link])
        .unwrap();
    assert_eq!(storage.list_secret_links().unwrap(), vec![link]);

    let first = secret(SecretId::new(), "First");
    let second = secret(SecretId::new(), "Second");
    let third = secret(SecretId::new(), "Third");
    let reused = [pair(first.id, second.id), pair(first.id, third.id)];
    assert!(
        storage
            .append_import(&[], &[first, second, third], &reused)
            .is_err()
    );
    assert_eq!(
        storage.list_secrets().unwrap().len(),
        2,
        "a rejected batch writes nothing"
    );
    assert_eq!(storage.list_secret_links().unwrap(), vec![link]);
}
