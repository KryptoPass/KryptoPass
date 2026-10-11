#![allow(clippy::unwrap_used)]

use super::*;

fn sample_folders() -> Vec<Folder> {
    let root = FolderId::new();
    let child = FolderId::new();
    vec![
        Folder {
            id: root,
            parent_id: None,
            sort_position: 42,
            name: "Root".to_owned(),
            appearance: FolderAppearance::default(),
            default_recipients: None,
        },
        Folder {
            id: child,
            parent_id: Some(root),
            sort_position: 17,
            name: "Child".to_owned(),
            appearance: FolderAppearance::default(),
            default_recipients: None,
        },
        Folder {
            id: FolderId::new(),
            parent_id: Some(child),
            sort_position: 8,
            name: "Grandchild".to_owned(),
            appearance: FolderAppearance::default(),
            default_recipients: None,
        },
    ]
}

#[test]
fn backup_v2_roundtrip_preserves_folder_positions_and_hierarchy() {
    let folders = sample_folders();
    let bytes = serialize_backup(folders.clone(), vec![]).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["version"], 2);
    let (restored, secrets) = deserialize_backup(&bytes).unwrap();
    assert_eq!(restored, folders);
    assert!(secrets.is_empty());
}

#[test]
fn legacy_backup_v1_without_positions_defaults_to_zero() {
    let mut folders = sample_folders();
    let bytes = serialize_backup(folders.clone(), vec![]).unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    document["version"] = serde_json::json!(1);
    for folder in document["folders"].as_array_mut().unwrap() {
        folder.as_object_mut().unwrap().remove("sort_position");
    }
    for folder in &mut folders {
        folder.sort_position = 0;
    }
    let bytes = serde_json::to_vec(&document).unwrap();
    let (restored, secrets) = deserialize_backup(&bytes).unwrap();
    assert_eq!(restored, folders);
    assert!(secrets.is_empty());
}

#[test]
fn backup_rejects_negative_folder_positions_on_read_and_write() {
    let mut folders = sample_folders();
    let bytes = serialize_backup(folders.clone(), vec![]).unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    document["folders"][0]["sort_position"] = serde_json::json!(-1);
    assert!(deserialize_backup(&serde_json::to_vec(&document).unwrap()).is_err());
    folders[0].sort_position = -1;
    assert!(serialize_backup(folders, vec![]).is_err());
}

struct RestoreTestDirectory(PathBuf);

impl RestoreTestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kryptopass-restore-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }
}

impl Drop for RestoreTestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn active_store_path() -> Option<PathBuf> {
    super::super::app_paths::active_vault_path(crate::app_info::APP.desktop_file_id).unwrap()
}

#[test]
fn automatic_restore_destinations_are_unique_private_and_do_not_activate_store() {
    let active_before = active_store_path();
    let first =
        super::super::app_paths::restored_vault_path(crate::app_info::APP.desktop_file_id).unwrap();
    let second =
        super::super::app_paths::restored_vault_path(crate::app_info::APP.desktop_file_id).unwrap();

    assert_ne!(first, second);
    assert!(!first.exists());
    assert!(!second.exists());
    assert_eq!(first.parent(), second.parent());
    assert!(first.is_absolute());
    let directory = first.parent().unwrap();
    assert!(directory.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o7777,
            0o700
        );
    }
    assert_eq!(active_store_path(), active_before);
}

#[test]
fn restore_missing_input_is_recoverable_and_preserves_existing_destination() {
    let directory = RestoreTestDirectory::new();
    let input = directory.0.join("missing-backup.gpg");
    let destination = directory.0.join("existing.sqlite3");
    let original = b"existing store must remain unchanged";
    fs::write(&destination, original).unwrap();
    let active_before = active_store_path();

    let error = restore_backup(&input, &destination, &GnuPgOpenPgp).unwrap_err();

    assert_eq!(
        error.downcast_ref::<RestoreFailure>(),
        Some(&RestoreFailure::Input)
    );
    assert_eq!(fs::read(&destination).unwrap(), original);
    assert_eq!(active_store_path(), active_before);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn restore_existing_destination_is_recoverable_without_overwriting_store() {
    let directory = RestoreTestDirectory::new();
    let input = directory.0.join("backup.gpg");
    let destination = directory.0.join("existing.sqlite3");
    let original = b"existing store must remain unchanged";
    fs::write(&input, b"decryption must not be reached").unwrap();
    fs::write(&destination, original).unwrap();
    let active_before = active_store_path();

    let error = restore_backup(&input, &destination, &GnuPgOpenPgp).unwrap_err();

    assert_eq!(
        error.downcast_ref::<RestoreFailure>(),
        Some(&RestoreFailure::Destination)
    );
    assert_eq!(fs::read(&destination).unwrap(), original);
    assert_eq!(active_store_path(), active_before);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn restore_non_private_directory_is_recoverable_without_publishing_store() {
    use std::os::unix::fs::PermissionsExt;

    let directory = RestoreTestDirectory::new();
    let input = directory.0.join("backup.gpg");
    let destination = directory.0.join("restored.sqlite3");
    fs::write(&input, b"decryption must not be reached").unwrap();
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o755)).unwrap();
    let active_before = active_store_path();

    let error = restore_backup(&input, &destination, &GnuPgOpenPgp).unwrap_err();

    assert_eq!(
        error.downcast_ref::<RestoreFailure>(),
        Some(&RestoreFailure::Destination)
    );
    assert!(!destination.exists());
    assert_eq!(active_store_path(), active_before);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    assert_eq!(
        fs::metadata(&directory.0).unwrap().permissions().mode() & 0o777,
        0o755
    );
}
