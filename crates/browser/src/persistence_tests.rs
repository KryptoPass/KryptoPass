#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use uuid::Uuid;

use super::persistence::{self, Store};

struct PrivateTempDir {
    path: PathBuf,
}

impl PrivateTempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kryptopass-browser-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("create isolated temporary directory");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .expect("make temporary directory private");
        Self { path }
    }

    fn store_path(&self) -> PathBuf {
        let directory = self.path.join("state");
        fs::create_dir(&directory).expect("create private store directory");
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .expect("set store directory mode");
        directory.join("browser.json")
    }
}

impl Drop for PrivateTempDir {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o700));
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn stored_identity(hash: &str) -> Store {
    Store::from_parts(hash.to_owned(), BTreeMap::new())
}

fn store_with_association(hash: &str) -> Store {
    let associations = BTreeMap::from([(
        "11111111-2222-4333-8444-555555555555".to_owned(),
        STANDARD.encode([0x5a; 32]),
    )]);
    Store::from_parts(hash.to_owned(), associations)
}

fn assert_store_eq(actual: &Store, expected: &Store) {
    assert_eq!(actual.hash, expected.hash);
    assert_eq!(actual.associations, expected.associations);
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path)
        .expect("inspect test path")
        .permissions()
        .mode()
        & 0o7777
}

#[test]
fn identity_and_associations_survive_a_reload() {
    let temporary = PrivateTempDir::new();
    let path = temporary.store_path();

    let initial = persistence::load(&path).expect("create first persistent identity");
    let with_association = store_with_association(&initial.hash);
    persistence::save(&path, &with_association).expect("persist browser association");

    let reloaded = persistence::load(&path).expect("reload browser store");
    assert_store_eq(&reloaded, &with_association);
    assert_eq!(mode(path.parent().expect("store parent")), 0o700);
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn malformed_store_is_rejected_without_replacing_its_contents() {
    let temporary = PrivateTempDir::new();
    let path = temporary.store_path();
    let malformed = b"{ definitely not browser store json";
    fs::write(&path, malformed).expect("write malformed store fixture");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .expect("make malformed fixture private");

    assert!(persistence::load(&path).is_err());
    assert!(
        persistence::save(
            &path,
            &stored_identity("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee")
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).expect("read malformed fixture"), malformed);
}

#[test]
fn unsafe_directory_and_file_permissions_are_rejected() {
    let temporary = PrivateTempDir::new();
    let path = temporary.store_path();
    let directory = path.parent().expect("store parent");

    fs::set_permissions(directory, fs::Permissions::from_mode(0o755))
        .expect("make directory fixture too permissive");
    assert!(persistence::load(&path).is_err());
    assert!(!path.exists());

    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .expect("restore private directory mode");
    persistence::save(
        &path,
        &stored_identity("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"),
    )
    .expect("write initial valid store");
    let previous = fs::read(&path).expect("read initial store");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
        .expect("make file fixture too permissive");

    assert!(persistence::load(&path).is_err());
    assert!(
        persistence::save(
            &path,
            &stored_identity("bbbbbbbb-cccc-4ddd-8eee-ffffffffffff"),
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).expect("read unsafe file fixture"), previous);
}

#[test]
fn symlink_store_is_rejected_for_load_and_save() {
    let temporary = PrivateTempDir::new();
    let path = temporary.store_path();
    let target = path.with_file_name("outside-store.json");
    let original = store_with_association("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee");
    let original_bytes = serde_json::to_vec(&original).expect("serialize target store");
    fs::write(&target, &original_bytes).expect("write target store");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).expect("make target private");
    symlink(&target, &path).expect("create store symlink");

    assert!(persistence::load(&path).is_err());
    assert!(
        persistence::save(
            &path,
            &stored_identity("bbbbbbbb-cccc-4ddd-8eee-ffffffffffff"),
        )
        .is_err()
    );
    assert_eq!(fs::read_link(&path).expect("inspect store symlink"), target);
    assert_eq!(
        fs::read(&target).expect("read target store"),
        original_bytes
    );
}

#[test]
fn invalid_save_leaves_the_previous_store_unchanged() {
    let temporary = PrivateTempDir::new();
    let path = temporary.store_path();
    let previous = store_with_association("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee");
    persistence::save(&path, &previous).expect("write previous valid store");
    let previous_bytes = fs::read(&path).expect("read previous store");
    let invalid = store_with_association("not-a-canonical-uuid");

    assert!(persistence::save(&path, &invalid).is_err());
    assert_eq!(
        fs::read(&path).expect("read store after failed save"),
        previous_bytes
    );
    let reloaded = persistence::load(&path).expect("load previous store after failed save");
    assert_store_eq(&reloaded, &previous);
}
