#![allow(clippy::unwrap_used)]

use super::*;
use kryptopass_core::Storage;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kryptopass-icon-test-{}", Uuid::new_v4()));
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

fn account(icon: Option<EntryIcon>) -> Secret {
    Secret {
        id: SecretId::new(),
        folder_id: None,
        title: "Account with public appearance".to_owned(),
        kind: SecretKind::Login,
        icon,
        recipients: RecipientSet::new(vec![
            KeyFingerprint::parse("0123456789ABCDEF0123456789ABCDEF01234567").unwrap(),
        ])
        .unwrap(),
        ciphertext: vec![0xc3, 0x03, 0x01, 0x02, 0x03],
        favorite: false,
        trashed: false,
        revision: 0,
    }
}

#[test]
fn image_icon_and_resource_survive_reopen_and_backup_restore() {
    let directory = TestDirectory::new();
    let asset = image_asset();
    let secret = account(Some(EntryIcon::Image { asset_id: asset.id }));
    let mut storage = directory.store("source.sqlite3");
    storage
        .insert_secret_with_icon(secret.clone(), Some(asset.clone()))
        .unwrap();
    drop(storage);

    let storage = SqliteStorage::open_existing(directory.0.join("source.sqlite3")).unwrap();
    assert_eq!(storage.find_secret(secret.id).unwrap().unwrap(), secret);
    let (_, metadata) = storage.read_snapshot().unwrap();
    assert_eq!(metadata[0].icon, secret.icon);
    let (folders, secrets, assets) = storage.backup_snapshot().unwrap();
    assert_eq!(assets, vec![asset.clone()]);
    let bytes = serialize_backup_with_icons(folders, secrets, assets).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["version"], 1);
    let (folders, secrets, assets) = deserialize_backup_with_icons(&bytes).unwrap();
    assert_eq!(assets, vec![asset]);

    let mut restored = directory.store("restored.sqlite3");
    restored
        .import_backup_snapshot(&folders, &secrets, &assets)
        .unwrap();
    assert_eq!(restored.find_secret(secret.id).unwrap().unwrap(), secret);
    assert_eq!(restored.backup_snapshot().unwrap().2, assets);
}

#[test]
fn replacing_image_and_then_removing_entry_collects_unused_assets() {
    let directory = TestDirectory::new();
    let mut storage = directory.store("store.sqlite3");
    let original_asset = image_asset();
    let mut secret = account(Some(EntryIcon::Image {
        asset_id: original_asset.id,
    }));
    storage
        .insert_secret_with_icon(secret.clone(), Some(original_asset))
        .unwrap();
    let replacement = image_asset();
    secret.icon = Some(EntryIcon::Image {
        asset_id: replacement.id,
    });
    storage
        .replace_secret_with_icon(secret.clone(), 0, Some(replacement.clone()))
        .unwrap();
    assert_eq!(storage.backup_snapshot().unwrap().2, vec![replacement]);
    assert_eq!(storage.find_secret(secret.id).unwrap().unwrap().revision, 1);
    storage.remove_secret(secret.id).unwrap();
    let (_, secrets, assets) = storage.backup_snapshot().unwrap();
    assert!(secrets.is_empty());
    assert!(assets.is_empty());
}

#[test]
fn stale_revision_rolls_back_new_icon_resource_and_preserves_current_entry() {
    let directory = TestDirectory::new();
    let mut storage = directory.store("store.sqlite3");
    let asset = image_asset();
    let original = account(Some(EntryIcon::Image { asset_id: asset.id }));
    storage
        .insert_secret_with_icon(original.clone(), Some(asset.clone()))
        .unwrap();
    let mut latest = original.clone();
    latest.title = "Latest saved title".to_owned();
    storage.replace_secret(latest, 0).unwrap();
    let expected = storage.find_secret(original.id).unwrap().unwrap();
    let staged = image_asset();
    let mut stale = original;
    stale.icon = Some(EntryIcon::Image {
        asset_id: staged.id,
    });
    assert!(
        storage
            .replace_secret_with_icon(stale, 0, Some(staged))
            .is_err()
    );
    assert_eq!(storage.find_secret(expected.id).unwrap().unwrap(), expected);
    assert_eq!(storage.backup_snapshot().unwrap().2, vec![asset]);
}

#[test]
fn missing_resource_rejects_insert_and_restore_without_partial_records() {
    let directory = TestDirectory::new();
    let secret = account(Some(EntryIcon::Image {
        asset_id: Uuid::new_v4(),
    }));
    let mut storage = directory.store("store.sqlite3");
    assert!(storage.insert_secret(secret.clone()).is_err());
    assert!(storage.import_backup_snapshot(&[], &[secret], &[]).is_err());
    let (folders, secrets, assets) = storage.backup_snapshot().unwrap();
    assert!(folders.is_empty());
    assert!(secrets.is_empty());
    assert!(assets.is_empty());
}

#[test]
fn backup_rejects_missing_duplicate_or_unreferenced_image_resources() {
    let asset = image_asset();
    let secret = account(Some(EntryIcon::Image { asset_id: asset.id }));
    let bytes = serialize_backup_with_icons(vec![], vec![secret], vec![asset]).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut missing = document.clone();
    missing["assets"] = serde_json::json!([]);
    let mut duplicate = document.clone();
    let entry = duplicate["assets"][0].clone();
    duplicate["assets"].as_array_mut().unwrap().push(entry);
    let mut unreferenced = document.clone();
    unreferenced["secrets"][0]["icon"] = serde_json::Value::Null;
    let mut other_version = document;
    other_version["version"] = serde_json::json!(2);
    for malformed in [missing, duplicate, unreferenced, other_version] {
        assert!(deserialize_backup_with_icons(&serde_json::to_vec(&malformed).unwrap()).is_err());
    }
}

#[test]
fn default_and_theme_icons_roundtrip_without_image_resources() {
    let theme = account(Some(EntryIcon::Theme {
        name: "internet-web-browser".to_owned(),
    }));
    let default = account(None);
    let bytes =
        serialize_backup_with_icons(vec![], vec![theme.clone(), default.clone()], vec![]).unwrap();
    let (folders, secrets, assets) = deserialize_backup_with_icons(&bytes).unwrap();
    assert!(folders.is_empty());
    assert!(assets.is_empty());
    assert_eq!(secrets, vec![theme, default]);
}

fn png_dimensions(bytes: &[u8]) -> (u32, u32) {
    (
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    )
}

#[test]
fn preparing_transparent_png_keeps_dimensions_and_alpha_channel() {
    let directory = TestDirectory::new();
    let path = directory.0.join("transparent.png");
    fs::write(&path, image_asset().png).unwrap();

    let prepared = crate::infrastructure::entry_icons::prepare_image(&path).unwrap();

    assert_ne!(prepared.id, Uuid::nil());
    assert!(prepared.validate().is_ok());
    assert!(crate::infrastructure::entry_icons::valid_normalized_image(
        &prepared.png
    ));
    assert_eq!(png_dimensions(&prepared.png), (1, 1));
    assert_eq!(prepared.png[24], 8);
    assert_eq!(prepared.png[25], 6);
}

#[test]
fn preparing_rectangular_image_limits_size_without_stretching() {
    // A 512 × 128 fully transparent RGBA8 PNG, including valid CRCs and compressed pixels.
    let source = base64_decode(concat!(
        "iVBORw0KGgoAAAANSUhEUgAAAgAAAACACAYAAAB9V9ELAAABFUlEQVR4nO3BMQEA",
        "AADCoPVPbQ0PoAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAvgwAvAABd7Mz4AAAAABJRU5ErkJggg==",
    ))
    .unwrap();
    assert_eq!(png_dimensions(&source), (512, 128));
    let directory = TestDirectory::new();
    let path = directory.0.join("rectangular.png");
    fs::write(&path, source).unwrap();

    let prepared = crate::infrastructure::entry_icons::prepare_image(&path).unwrap();

    assert!(prepared.validate().is_ok());
    assert!(crate::infrastructure::entry_icons::valid_normalized_image(
        &prepared.png
    ));
    assert_eq!(png_dimensions(&prepared.png), (256, 64));
}

#[test]
fn preparing_non_image_file_rejects_content_despite_image_extension() {
    let directory = TestDirectory::new();
    let path = directory.0.join("not-an-image.png");
    fs::write(&path, b"not PNG, JPEG, or WebP image data").unwrap();

    assert!(crate::infrastructure::entry_icons::prepare_image(&path).is_err());
}
