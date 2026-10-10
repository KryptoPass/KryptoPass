#![allow(clippy::unwrap_used)]

use super::*;

fn store() -> SqliteStorage {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    initialize_schema(&mut connection).unwrap();
    SqliteStorage { connection }
}

fn recipients() -> RecipientSet {
    RecipientSet::new(vec![
        KeyFingerprint::parse("0123456789ABCDEF0123456789ABCDEF01234567").unwrap(),
        KeyFingerprint::parse("FEDCBA9876543210FEDCBA9876543210FEDCBA98").unwrap(),
    ])
    .unwrap()
}

fn folder(storage: &mut SqliteStorage, name: &str, parent_id: Option<FolderId>) -> FolderId {
    let id = FolderId::new();
    storage
        .insert_folder(Folder {
            id,
            parent_id,
            name: name.to_owned(),
            sort_position: 0,
            appearance: FolderAppearance::default(),
            default_recipients: Some(recipients()),
        })
        .unwrap();
    id
}

fn siblings(storage: &SqliteStorage, parent: Option<FolderId>) -> Vec<FolderId> {
    let mut folders: Vec<_> = storage
        .list_folders()
        .unwrap()
        .into_iter()
        .filter(|folder| folder.parent_id == parent)
        .collect();
    folders.sort_by_key(|folder| folder.sort_position);
    assert!(
        folders
            .windows(2)
            .all(|pair| pair[0].sort_position < pair[1].sort_position)
    );
    folders.into_iter().map(|folder| folder.id).collect()
}

#[test]
fn moving_folder_keeps_descendants_and_encrypted_secrets_intact() {
    let mut storage = store();
    let source = folder(&mut storage, "Source", None);
    let child = folder(&mut storage, "Child", Some(source));
    let grandchild = folder(&mut storage, "Grandchild", Some(child));
    let destination = folder(&mut storage, "Destination", None);
    let child_before = storage.find_folder(child).unwrap().unwrap();
    let grandchild_before = storage.find_folder(grandchild).unwrap().unwrap();
    let secret = Secret {
        id: SecretId::new(),
        folder_id: Some(grandchild),
        title: "Encrypted account".to_owned(),
        kind: SecretKind::Login,
        recipients: recipients(),
        ciphertext: vec![0xc3, 0x03, 0x01, 0x02, 0x03],
        favorite: true,
        trashed: false,
        revision: 0,
    };
    storage.insert_secret(secret.clone()).unwrap();

    storage
        .move_folder(source, Some(destination), None)
        .unwrap();

    assert_eq!(
        storage.find_folder(source).unwrap().unwrap().parent_id,
        Some(destination)
    );
    assert_eq!(storage.find_folder(child).unwrap().unwrap(), child_before);
    assert_eq!(
        storage.find_folder(grandchild).unwrap().unwrap(),
        grandchild_before
    );
    assert_eq!(storage.find_secret(secret.id).unwrap().unwrap(), secret);
    assert_eq!(siblings(&storage, None), vec![destination]);
    assert_eq!(siblings(&storage, Some(destination)), vec![source]);
}

#[test]
fn folder_order_supports_before_append_and_reparent_to_root() {
    let mut storage = store();
    let alpha = folder(&mut storage, "Alpha", None);
    let beta = folder(&mut storage, "Beta", None);
    let gamma = folder(&mut storage, "Gamma", None);

    storage.move_folder(gamma, None, Some(alpha)).unwrap();
    assert_eq!(siblings(&storage, None), vec![gamma, alpha, beta]);
    storage.move_folder(gamma, None, None).unwrap();
    assert_eq!(siblings(&storage, None), vec![alpha, beta, gamma]);

    let child = folder(&mut storage, "Child", Some(beta));
    storage.move_folder(alpha, Some(beta), Some(child)).unwrap();
    assert_eq!(siblings(&storage, Some(beta)), vec![alpha, child]);
    assert_eq!(siblings(&storage, None), vec![beta, gamma]);
    storage.move_folder(alpha, None, Some(gamma)).unwrap();
    assert_eq!(siblings(&storage, None), vec![beta, alpha, gamma]);
    assert_eq!(siblings(&storage, Some(beta)), vec![child]);
}

#[test]
#[cfg(unix)]
fn folder_tree_and_manual_order_survive_database_reopen() {
    use std::os::unix::fs::DirBuilderExt;

    struct PrivateDirectory(std::path::PathBuf);
    impl Drop for PrivateDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    let directory = PrivateDirectory(
        std::env::temp_dir().join(format!("kryptopass-folder-test-{}", Uuid::new_v4())),
    );
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory.0)
        .unwrap();
    let path = directory.0.join("vault.sqlite3");
    let mut storage = SqliteStorage::open(&path).unwrap();
    let alpha = folder(&mut storage, "Alpha", None);
    let beta = folder(&mut storage, "Beta", None);
    let child = folder(&mut storage, "Child", Some(alpha));
    let grandchild = folder(&mut storage, "Grandchild", Some(child));
    storage.move_folder(beta, None, Some(alpha)).unwrap();
    storage.move_folder(child, Some(beta), None).unwrap();
    let expected = storage.list_folders().unwrap();
    drop(storage);

    let storage = SqliteStorage::open_existing(&path).unwrap();
    assert_eq!(storage.list_folders().unwrap(), expected);
    assert_eq!(siblings(&storage, None), vec![beta, alpha]);
    assert_eq!(siblings(&storage, Some(beta)), vec![child]);
    assert_eq!(siblings(&storage, Some(child)), vec![grandchild]);
    drop(storage);
}

#[test]
fn cycles_missing_folders_and_invalid_anchors_leave_all_folders_unchanged() {
    let mut storage = store();
    let root = folder(&mut storage, "Root", None);
    let child = folder(&mut storage, "Child", Some(root));
    let grandchild = folder(&mut storage, "Grandchild", Some(child));
    let other = folder(&mut storage, "Other", None);
    let missing = FolderId::new();
    let before = storage.list_folders().unwrap();

    for (moving, parent, anchor) in [
        (root, Some(root), None),
        (root, Some(child), None),
        (root, Some(grandchild), None),
        (root, Some(missing), None),
        (missing, None, None),
        (other, None, Some(child)),
        (other, Some(root), Some(grandchild)),
        (other, None, Some(missing)),
        (other, None, Some(other)),
    ] {
        assert!(storage.move_folder(moving, parent, anchor).is_err());
        assert_eq!(storage.list_folders().unwrap(), before);
    }
}

#[test]
fn deleting_parent_promotes_children_in_place_without_losing_their_tree_or_secret() {
    let mut storage = store();
    let alpha = folder(&mut storage, "Alpha", None);
    let parent = folder(&mut storage, "Parent", None);
    let zebra = folder(&mut storage, "Zebra", None);
    let first = folder(&mut storage, "Alpha child", Some(parent));
    let second = folder(&mut storage, "Zebra child", Some(parent));
    let grandchild = folder(&mut storage, "Grandchild", Some(second));
    storage.move_folder(zebra, None, Some(alpha)).unwrap();
    storage.move_folder(parent, None, Some(alpha)).unwrap();
    storage
        .move_folder(second, Some(parent), Some(first))
        .unwrap();
    assert_eq!(siblings(&storage, None), vec![zebra, parent, alpha]);
    assert_eq!(siblings(&storage, Some(parent)), vec![second, first]);
    let secret = Secret {
        id: SecretId::new(),
        folder_id: Some(second),
        title: "Child secret".to_owned(),
        kind: SecretKind::Login,
        recipients: recipients(),
        ciphertext: vec![0xc3, 0x03, 0x01, 0x02, 0x03],
        favorite: false,
        trashed: false,
        revision: 0,
    };
    storage.insert_secret(secret.clone()).unwrap();

    storage.remove_folder(parent).unwrap();

    assert!(storage.find_folder(parent).unwrap().is_none());
    assert_eq!(siblings(&storage, None), vec![zebra, second, first, alpha]);
    assert_eq!(siblings(&storage, Some(second)), vec![grandchild]);
    assert_eq!(storage.find_secret(secret.id).unwrap().unwrap(), secret);
}

#[test]
fn schema_v1_migration_adds_appearance_and_order_without_losing_names() {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
         CREATE TABLE folders (
             id TEXT PRIMARY KEY NOT NULL,
             parent_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
             name TEXT NOT NULL CHECK(length(trim(name)) > 0),
             CHECK(parent_id IS NULL OR parent_id <> id)
         );
         CREATE TABLE folder_recipients (
             folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
             fingerprint TEXT NOT NULL, PRIMARY KEY(folder_id, fingerprint)
         );
         CREATE TABLE secrets (
             id TEXT PRIMARY KEY NOT NULL,
             folder_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
             title TEXT NOT NULL, kind TEXT NOT NULL,
             favorite INTEGER NOT NULL, trashed INTEGER NOT NULL,
             ciphertext BLOB NOT NULL, revision INTEGER NOT NULL
         );
         CREATE TABLE secret_recipients (
             secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
             fingerprint TEXT NOT NULL, PRIMARY KEY(secret_id, fingerprint)
         );
         PRAGMA user_version = 1;",
        )
        .unwrap();
    connection
        .pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    let zebra = FolderId::new();
    let alpha = FolderId::new();
    let child = FolderId::new();
    for (id, parent, name) in [
        (zebra, None, "Zebra original"),
        (alpha, None, "Alpha original"),
        (child, Some(zebra), "Original child"),
    ] {
        connection
            .execute(
                "INSERT INTO folders(id, parent_id, name) VALUES (?1, ?2, ?3)",
                params![id.0.to_string(), parent.map(|id| id.0.to_string()), name],
            )
            .unwrap();
    }

    initialize_schema(&mut connection).unwrap();
    assert_eq!(
        pragma_i64(&connection, "user_version").unwrap(),
        SCHEMA_VERSION
    );
    let storage = SqliteStorage { connection };
    assert_eq!(siblings(&storage, None), vec![alpha, zebra]);
    assert_eq!(siblings(&storage, Some(zebra)), vec![child]);
    for (id, name) in [
        (zebra, "Zebra original"),
        (alpha, "Alpha original"),
        (child, "Original child"),
    ] {
        let restored = storage.find_folder(id).unwrap().unwrap();
        assert_eq!(restored.name, name);
        assert_eq!(restored.appearance, FolderAppearance::default());
    }
}

#[test]
fn schema_v2_migration_preserves_alphabetical_order_per_parent() {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
         CREATE TABLE folders (
             id TEXT PRIMARY KEY NOT NULL,
             parent_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
             name TEXT NOT NULL CHECK(length(trim(name)) > 0),
             icon TEXT NOT NULL DEFAULT 'folder',
             color TEXT NOT NULL DEFAULT 'transparent',
             CHECK(parent_id IS NULL OR parent_id <> id)
         );
         CREATE TABLE folder_recipients (
             folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
             fingerprint TEXT NOT NULL, PRIMARY KEY(folder_id, fingerprint)
         );
         CREATE TABLE secrets (
             id TEXT PRIMARY KEY NOT NULL,
             folder_id TEXT REFERENCES folders(id) ON DELETE RESTRICT,
             title TEXT NOT NULL, kind TEXT NOT NULL,
             favorite INTEGER NOT NULL, trashed INTEGER NOT NULL,
             ciphertext BLOB NOT NULL, revision INTEGER NOT NULL
         );
         CREATE TABLE secret_recipients (
             secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
             fingerprint TEXT NOT NULL, PRIMARY KEY(secret_id, fingerprint)
         );
         PRAGMA user_version = 2;",
        )
        .unwrap();
    connection
        .pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    let zebra = FolderId::new();
    let alpha = FolderId::new();
    let child_z = FolderId::new();
    let child_a = FolderId::new();
    for (id, parent, name) in [
        (zebra, None, "Zebra"),
        (alpha, None, "Alpha"),
        (child_z, Some(zebra), "Zebra child"),
        (child_a, Some(zebra), "Alpha child"),
    ] {
        connection
            .execute(
                "INSERT INTO folders(id, parent_id, name) VALUES (?1, ?2, ?3)",
                params![id.0.to_string(), parent.map(|id| id.0.to_string()), name],
            )
            .unwrap();
    }

    initialize_schema(&mut connection).unwrap();
    assert_eq!(
        pragma_i64(&connection, "user_version").unwrap(),
        SCHEMA_VERSION
    );
    let storage = SqliteStorage { connection };
    assert_eq!(siblings(&storage, None), vec![alpha, zebra]);
    assert_eq!(siblings(&storage, Some(zebra)), vec![child_a, child_z]);
    assert_eq!(
        storage.find_folder(child_a).unwrap().unwrap().name,
        "Alpha child"
    );
}
