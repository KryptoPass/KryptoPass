#![allow(clippy::unwrap_used)]

use kryptopass_core::{EntryIcon, EntryIconAsset};
use uuid::Uuid;

fn transparent_png() -> Vec<u8> {
    vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 96, 0, 2, 0, 0, 5, 0,
        1, 122, 94, 171, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ]
}

fn asset(png: Vec<u8>) -> EntryIconAsset {
    EntryIconAsset {
        id: Uuid::new_v4(),
        png,
    }
}

#[test]
fn theme_icons_accept_names_and_reject_paths_or_external_sources() {
    for name in ["dialog-password", "document", "folder-blue"] {
        assert!(EntryIcon::Theme { name: name.into() }.validate().is_ok());
    }
    for name in [
        "",
        "../icon",
        "/usr/share/icons/icon.png",
        "file:///tmp/icon.png",
        "https://example.org/icon.png",
        "folder\\icon",
        "icon name",
    ] {
        assert!(EntryIcon::Theme { name: name.into() }.validate().is_err());
    }
    assert!(
        EntryIcon::Theme {
            name: "a".repeat(128)
        }
        .validate()
        .is_ok()
    );
    assert!(
        EntryIcon::Theme {
            name: "a".repeat(129)
        }
        .validate()
        .is_err()
    );
}

#[test]
fn serialized_icons_distinguish_theme_names_from_image_asset_ids() {
    let id = Uuid::new_v4();
    let theme = serde_json::json!({ "kind": "theme", "name": "dialog-password" });
    let image = serde_json::json!({ "kind": "image", "assetId": id.to_string() });
    assert_eq!(
        serde_json::to_value(EntryIcon::Theme {
            name: "dialog-password".into()
        })
        .unwrap(),
        theme
    );
    assert_eq!(
        serde_json::to_value(EntryIcon::Image { asset_id: id }).unwrap(),
        image
    );
    for value in [theme, image] {
        let icon: EntryIcon = serde_json::from_value(value).unwrap();
        assert!(icon.validate().is_ok());
    }
    assert!(
        serde_json::from_value::<EntryIcon>(serde_json::json!({
            "kind": "remote", "url": "https://example.org/icon.png"
        }))
        .is_err()
    );
    for value in [
        serde_json::json!({"kind": "image", "assetId": "not-a-uuid"}),
        serde_json::json!({"kind": "image", "asset_id": id.to_string()}),
        serde_json::json!({"kind": "theme", "name": "folder", "url": "https://example.org"}),
    ] {
        assert!(serde_json::from_value::<EntryIcon>(value).is_err());
    }
}

#[test]
fn image_references_and_assets_reject_nil_identifiers() {
    assert!(
        EntryIcon::Image {
            asset_id: Uuid::new_v4()
        }
        .validate()
        .is_ok()
    );
    assert!(
        EntryIcon::Image {
            asset_id: Uuid::nil()
        }
        .validate()
        .is_err()
    );
    assert!(
        EntryIconAsset {
            id: Uuid::nil(),
            png: transparent_png()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn png_assets_accept_transparency_and_reject_missing_or_non_png_content() {
    assert!(asset(transparent_png()).validate().is_ok());
    for bytes in [
        vec![],
        b"<svg></svg>".to_vec(),
        transparent_png()[..24].to_vec(),
    ] {
        assert!(asset(bytes).validate().is_err());
    }
    let mut invalid_signature = transparent_png();
    invalid_signature[0] = 0;
    assert!(asset(invalid_signature).validate().is_err());
}

#[test]
fn png_assets_reject_payloads_above_512_kibibytes() {
    let mut oversized = transparent_png();
    oversized.resize(512 * 1024 + 1, 0);
    assert!(asset(oversized).validate().is_err());
}

#[test]
fn png_assets_reject_zero_or_excessive_dimensions() {
    for (width, height) in [(0_u32, 1_u32), (1, 0), (257, 1), (1, 257), (u32::MAX, 1)] {
        let mut png = transparent_png();
        png[16..20].copy_from_slice(&width.to_be_bytes());
        png[20..24].copy_from_slice(&height.to_be_bytes());
        assert!(asset(png).validate().is_err());
    }
}

#[test]
fn normalized_png_assets_reject_unsupported_color_depth_and_animation() {
    for (offset, value) in [(24, 16), (25, 3), (26, 1), (27, 1), (28, 1)] {
        let mut png = transparent_png();
        png[offset] = value;
        assert!(asset(png).validate().is_err());
    }
    let mut animated = transparent_png();
    animated.splice(
        33..33,
        [
            0, 0, 0, 8, b'a', b'c', b'T', b'L', 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
    );
    assert!(asset(animated).validate().is_err());
    let mut missing_end = transparent_png();
    missing_end.truncate(missing_end.len() - 12);
    assert!(asset(missing_end).validate().is_err());
}
