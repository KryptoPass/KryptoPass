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

fn secret(title: &str, folder_id: Option<FolderId>, trashed: bool) -> Secret {
    Secret {
        id: SecretId::new(),
        folder_id,
        title: title.to_owned(),
        kind: SecretKind::Login,
        icon: None,
        recipients: recipients(),
        ciphertext: vec![0xc3, 0x03, 0x01, 0x02, 0x03],
        favorite: true,
        trashed,
        revision: 0,
    }
}

fn image_asset() -> EntryIconAsset {
    EntryIconAsset {
        id: Uuid::new_v4(),
        png: vec![
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 96, 0, 2,
            0, 0, 5, 0, 1, 122, 94, 171, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ],
    }
}

fn recipient_counts(storage: &SqliteStorage) -> (i64, i64) {
    storage
        .connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM folder_recipients),
                    (SELECT COUNT(*) FROM secret_recipients)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
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
        icon: None,
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
        icon: None,
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
fn deleting_folder_tree_removes_deep_contents_and_preserves_external_order_and_recipients() {
    for nested in [false, true] {
        let mut storage = store();
        let outer = folder(&mut storage, "Outer", None);
        let parent = nested.then_some(outer);
        let alpha = folder(&mut storage, "Alpha", parent);
        let target = folder(&mut storage, "Target", parent);
        let zebra = folder(&mut storage, "Zebra", parent);
        let child = folder(&mut storage, "Child", Some(target));
        let other_child = folder(&mut storage, "Other child", Some(target));
        let grandchild = folder(&mut storage, "Grandchild", Some(child));
        let great_grandchild = folder(&mut storage, "Great-grandchild", Some(grandchild));
        let external_child = folder(&mut storage, "External child", Some(alpha));
        let external_child_before = storage.find_folder(external_child).unwrap().unwrap();
        storage.move_folder(zebra, parent, Some(alpha)).unwrap();
        storage.move_folder(target, parent, Some(alpha)).unwrap();
        let expected_root_order: Vec<_> = siblings(&storage, None)
            .into_iter()
            .filter(|id| *id != target)
            .collect();
        let expected_parent_order: Vec<_> = siblings(&storage, parent)
            .into_iter()
            .filter(|id| *id != target)
            .collect();

        let removed = [
            secret("Direct", Some(target), false),
            secret("Trashed child", Some(child), true),
            secret("Other branch", Some(other_child), false),
            secret("Grandchild account", Some(grandchild), false),
            secret("Trashed deep account", Some(great_grandchild), true),
        ];
        let preserved = [
            secret("Root account", None, false),
            secret("External trashed account", Some(external_child), true),
        ];
        for entry in removed.iter().chain(preserved.iter()) {
            storage.insert_secret(entry.clone()).unwrap();
        }

        storage.remove_folder_tree(target).unwrap();

        for id in [target, child, other_child, grandchild, great_grandchild] {
            assert!(storage.find_folder(id).unwrap().is_none());
        }
        assert_eq!(storage.list_folders().unwrap().len(), 4);
        assert_eq!(siblings(&storage, None), expected_root_order);
        assert_eq!(siblings(&storage, parent), expected_parent_order);
        assert_eq!(siblings(&storage, Some(alpha)), vec![external_child]);
        assert_eq!(
            storage.find_folder(external_child).unwrap().unwrap(),
            external_child_before
        );
        for entry in removed {
            assert!(storage.find_secret(entry.id).unwrap().is_none());
        }
        assert_eq!(storage.list_secrets().unwrap().len(), preserved.len());
        for entry in preserved {
            assert_eq!(storage.find_secret(entry.id).unwrap().unwrap(), entry);
        }
        assert_eq!(recipient_counts(&storage), (8, 4));
    }
}

#[test]
fn deleting_folder_tree_collects_unused_icons_and_revises_only_surviving_link_endpoints() {
    let mut storage = store();
    let target = folder(&mut storage, "Target", None);
    let child = folder(&mut storage, "Child", Some(target));
    let outside = folder(&mut storage, "Outside", None);
    let shared_asset = image_asset();
    let removed_asset = image_asset();
    let outside_asset = image_asset();
    let mut survivor = secret("Surviving companion", Some(outside), false);
    survivor.icon = Some(EntryIcon::Image {
        asset_id: shared_asset.id,
    });
    storage
        .insert_secret_with_icon(survivor.clone(), Some(shared_asset.clone()))
        .unwrap();
    let mut linked_removed = secret("Removed companion", Some(child), false);
    linked_removed.icon = survivor.icon.clone();
    storage
        .insert_secret_with_link(linked_removed.clone(), None, Some(survivor.id))
        .unwrap();
    let mut exclusive_removed = secret("Exclusive icon", Some(target), true);
    exclusive_removed.icon = Some(EntryIcon::Image {
        asset_id: removed_asset.id,
    });
    storage
        .insert_secret_with_icon(exclusive_removed.clone(), Some(removed_asset.clone()))
        .unwrap();
    let mut first_outside = secret("Outside pair first", None, false);
    first_outside.icon = Some(EntryIcon::Image {
        asset_id: outside_asset.id,
    });
    storage
        .insert_secret_with_icon(first_outside.clone(), Some(outside_asset))
        .unwrap();
    let second_outside = secret("Outside pair second", Some(outside), false);
    storage
        .insert_secret_with_link(second_outside.clone(), None, Some(first_outside.id))
        .unwrap();

    let mut survivor_expected = storage.find_secret(survivor.id).unwrap().unwrap();
    survivor_expected.revision += 1;
    let first_outside_before = storage.find_secret(first_outside.id).unwrap().unwrap();
    let expected_links: Vec<_> = storage
        .list_secret_links()
        .unwrap()
        .into_iter()
        .filter(|link| link.first != linked_removed.id && link.second != linked_removed.id)
        .collect();
    assert_eq!(expected_links.len(), 1);
    let expected_assets: Vec<_> = storage
        .backup_snapshot()
        .unwrap()
        .2
        .into_iter()
        .filter(|asset| asset.id != removed_asset.id)
        .collect();

    storage.remove_folder_tree(target).unwrap();

    assert!(storage.find_secret(linked_removed.id).unwrap().is_none());
    assert!(storage.find_secret(exclusive_removed.id).unwrap().is_none());
    assert_eq!(
        storage.find_secret(survivor.id).unwrap().unwrap(),
        survivor_expected
    );
    assert_eq!(
        storage.find_secret(first_outside.id).unwrap().unwrap(),
        first_outside_before
    );
    assert_eq!(
        storage.find_secret(second_outside.id).unwrap().unwrap(),
        second_outside
    );
    assert_eq!(storage.list_secret_links().unwrap(), expected_links);
    assert_eq!(storage.backup_snapshot().unwrap().2, expected_assets);
    assert_eq!(recipient_counts(&storage), (2, 6));
}

#[test]
fn deleting_both_link_endpoints_in_folder_tree_needs_no_remaining_revision() {
    let mut storage = store();
    let target = folder(&mut storage, "Target", None);
    let child = folder(&mut storage, "Child", Some(target));
    let first = secret("First internal endpoint", Some(target), false);
    let second = secret("Second internal endpoint", Some(child), false);
    let outside = secret("Unrelated account", None, false);
    storage.insert_secret(first.clone()).unwrap();
    storage
        .insert_secret_with_link(second.clone(), None, Some(first.id))
        .unwrap();
    storage.insert_secret(outside.clone()).unwrap();
    storage
        .connection
        .execute(
            "UPDATE secrets SET revision = ?1 WHERE id IN (?2, ?3)",
            params![i64::MAX, first.id.0.to_string(), second.id.0.to_string()],
        )
        .unwrap();

    storage.remove_folder_tree(target).unwrap();

    assert!(storage.list_folders().unwrap().is_empty());
    assert_eq!(storage.list_secrets().unwrap(), vec![outside]);
    assert!(storage.list_secret_links().unwrap().is_empty());
    assert_eq!(recipient_counts(&storage), (0, 2));
}

#[test]
fn exhausted_surviving_link_revision_rolls_back_the_entire_folder_tree_deletion() {
    let mut storage = store();
    let alpha = folder(&mut storage, "Alpha", None);
    let target = folder(&mut storage, "Target", None);
    let zebra = folder(&mut storage, "Zebra", None);
    let child = folder(&mut storage, "Child", Some(target));
    let grandchild = folder(&mut storage, "Grandchild", Some(child));
    storage.move_folder(zebra, None, Some(alpha)).unwrap();
    storage.move_folder(target, None, Some(alpha)).unwrap();
    let mut ready_survivor = secret("Ready survivor", Some(alpha), false);
    ready_survivor.id = SecretId(Uuid::from_u128(1));
    let mut exhausted_survivor = secret("Exhausted survivor", Some(zebra), false);
    exhausted_survivor.id = SecretId(Uuid::from_u128(4));
    storage.insert_secret(ready_survivor.clone()).unwrap();
    storage.insert_secret(exhausted_survivor.clone()).unwrap();
    let asset = image_asset();
    let mut direct = secret("Direct account with icon", Some(target), false);
    direct.id = SecretId(Uuid::from_u128(2));
    direct.icon = Some(EntryIcon::Image { asset_id: asset.id });
    storage
        .insert_secret_with_link(direct, Some(asset), Some(ready_survivor.id))
        .unwrap();
    let mut deep = secret("Deep linked account", Some(grandchild), false);
    deep.id = SecretId(Uuid::from_u128(3));
    storage
        .insert_secret_with_link(deep, None, Some(exhausted_survivor.id))
        .unwrap();
    storage
        .insert_secret(secret("Trashed child account", Some(child), true))
        .unwrap();
    storage
        .connection
        .execute(
            "UPDATE secrets SET revision = ?1 WHERE id = ?2",
            params![i64::MAX, exhausted_survivor.id.0.to_string()],
        )
        .unwrap();
    let before = storage.backup_snapshot_with_links().unwrap();
    let recipients_before = recipient_counts(&storage);

    assert!(storage.remove_folder_tree(target).is_err());

    assert_eq!(storage.backup_snapshot_with_links().unwrap(), before);
    assert_eq!(recipient_counts(&storage), recipients_before);
    assert_eq!(siblings(&storage, None), vec![zebra, target, alpha]);
    assert!(storage.remove_folder_tree(FolderId::new()).is_err());
    assert_eq!(storage.backup_snapshot_with_links().unwrap(), before);
    assert_eq!(recipient_counts(&storage), recipients_before);
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
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
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
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
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
