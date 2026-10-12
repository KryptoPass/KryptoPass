#![allow(clippy::unwrap_used)]

use super::*;

const RECIPIENT: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

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

#[test]
fn backup_round_trip_preserves_links_and_uses_v3_when_unlinked() {
    let first = secret(SecretId::new(), "First");
    let second = secret(SecretId::new(), "Second");
    let third = secret(SecretId::new(), "Third");
    let link = pair(first.id, second.id);
    let bytes = serialize_backup_with_links(
        vec![],
        vec![first.clone(), second.clone(), third.clone()],
        vec![],
        vec![link],
    )
    .unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["version"], 4);
    assert_eq!(
        document["links"],
        serde_json::json!([{
            "first": link.first.0.to_string(),
            "second": link.second.0.to_string(),
        }])
    );

    let (folders, secrets, assets, links) = deserialize_backup_with_links(&bytes).unwrap();
    assert!(folders.is_empty());
    assert_eq!(secrets, vec![first.clone(), second.clone(), third.clone()]);
    assert!(assets.is_empty());
    assert_eq!(links, vec![link]);

    let unlinked =
        serialize_backup_with_links(vec![], vec![first, second, third], vec![], vec![]).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&unlinked).unwrap();
    assert_eq!(document["version"], 3);
    assert!(document.get("links").is_none());
}

#[test]
fn backup_reader_rejects_self_missing_occupied_and_legacy_links() {
    let first = secret(SecretId::new(), "First");
    let second = secret(SecretId::new(), "Second");
    let third = secret(SecretId::new(), "Third");
    let link = pair(first.id, second.id);
    let bytes = serialize_backup_with_links(
        vec![],
        vec![first.clone(), second.clone(), third.clone()],
        vec![],
        vec![link],
    )
    .unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    let mut self_link = document.clone();
    self_link["links"][0]["second"] = serde_json::json!(link.first.0.to_string());

    let mut missing_target = document.clone();
    missing_target["links"][0]["second"] = serde_json::json!(SecretId::new().0.to_string());

    let mut occupied_endpoint = document.clone();
    let occupied_link = pair(link.second, third.id);
    occupied_endpoint["links"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "first": occupied_link.first.0.to_string(),
            "second": occupied_link.second.0.to_string(),
        }));

    let mut legacy_links = document;
    legacy_links["version"] = serde_json::json!(3);

    for malformed in [self_link, missing_target, occupied_endpoint, legacy_links] {
        assert!(deserialize_backup_with_links(&serde_json::to_vec(&malformed).unwrap()).is_err());
    }
}
