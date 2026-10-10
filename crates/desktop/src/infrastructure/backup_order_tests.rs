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
