//! Record integrity, folder hierarchy, companion links, and icon budgets.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail, ensure};
use kryptopass_core::{
    EntryIcon, EntryIconAsset, Folder, FolderId, KeyFingerprint, Secret, SecretLink,
};

use super::super::{MAX_BACKUP_PLAINTEXT_BYTES, MAX_BACKUP_RECIPIENTS, MAX_BACKUP_ROWS};
use super::MAX_RECIPIENTS_PER_RECORD;

pub(super) fn validate_links(secrets: &[Secret], links: &[SecretLink]) -> Result<()> {
    ensure!(links.len() <= MAX_BACKUP_ROWS / 2, "too many linked pairs");
    let ids: HashSet<_> = secrets.iter().map(|secret| secret.id).collect();
    let mut paired = HashSet::new();
    for link in links {
        ensure!(link.first != link.second, "a secret cannot link to itself");
        ensure!(
            ids.contains(&link.first) && ids.contains(&link.second),
            "missing linked secret"
        );
        ensure!(
            paired.insert(link.first) && paired.insert(link.second),
            "a secret has more than one companion"
        );
    }
    Ok(())
}

pub(super) fn validate_domain_records(folders: &[Folder], secrets: &[Secret]) -> Result<()> {
    ensure!(
        folders.len().saturating_add(secrets.len()) <= MAX_BACKUP_ROWS,
        "backup contains too many records"
    );
    let folder_ids = folders
        .iter()
        .map(|folder| folder.id)
        .collect::<HashSet<_>>();
    ensure!(
        folder_ids.len() == folders.len(),
        "backup has duplicate folder ids"
    );
    let secret_ids = secrets
        .iter()
        .map(|secret| secret.id)
        .collect::<HashSet<_>>();
    ensure!(
        secret_ids.len() == secrets.len(),
        "backup has duplicate secret ids"
    );

    let mut parents = HashMap::with_capacity(folders.len());
    let mut recipient_count = 0usize;
    let mut raw_bytes = 0usize;
    for folder in folders {
        ensure!(folder.sort_position >= 0, "backup folder order is invalid");
        ensure!(
            !folder.name.trim().is_empty() && folder.name.len() <= 256,
            "backup folder name is invalid"
        );
        let parent = folder.parent_id;
        if let Some(parent_id) = parent {
            ensure!(
                folder_ids.contains(&parent_id),
                "backup folder parent is missing"
            );
        }
        parents.insert(folder.id, parent);
        raw_bytes = raw_bytes
            .saturating_add(folder.name.len())
            .saturating_add(folder.appearance.icon().len())
            .saturating_add(folder.appearance.color().len());
        if let Some(recipients) = &folder.default_recipients {
            ensure!(
                recipients.as_slice().len() <= MAX_RECIPIENTS_PER_RECORD,
                "backup folder has too many recipients"
            );
            recipient_count = recipient_count.saturating_add(recipients.as_slice().len());
            for fingerprint in recipients.as_slice() {
                let parsed = KeyFingerprint::parse(fingerprint.as_str())
                    .map_err(|_| anyhow::anyhow!("backup recipient fingerprint is invalid"))?;
                ensure!(parsed == *fingerprint, "backup recipient is not canonical");
                raw_bytes = raw_bytes.saturating_add(fingerprint.as_str().len());
            }
        }
    }
    validate_folder_cycles(&parents)?;

    for secret in secrets {
        ensure!(
            !secret.title.trim().is_empty() && secret.title.len() <= 512,
            "backup secret title is invalid"
        );
        ensure!(
            !secret.ciphertext.is_empty()
                && secret.ciphertext.len() <= crate::infrastructure::MAX_CIPHERTEXT_BYTES,
            "backup secret ciphertext is invalid"
        );
        ensure!(
            secret.revision <= i64::MAX as u64,
            "backup secret revision is invalid"
        );
        if let Some(folder_id) = secret.folder_id {
            ensure!(
                folder_ids.contains(&folder_id),
                "backup secret folder is missing"
            );
        }
        ensure!(
            !secret.recipients.as_slice().is_empty()
                && secret.recipients.as_slice().len() <= MAX_RECIPIENTS_PER_RECORD,
            "backup secret recipients are invalid"
        );
        recipient_count = recipient_count.saturating_add(secret.recipients.as_slice().len());
        raw_bytes = raw_bytes
            .saturating_add(secret.title.len())
            .saturating_add(secret.kind.as_str().len())
            .saturating_add(secret.ciphertext.len());
        for fingerprint in secret.recipients.as_slice() {
            let parsed = KeyFingerprint::parse(fingerprint.as_str())
                .map_err(|_| anyhow::anyhow!("backup recipient fingerprint is invalid"))?;
            ensure!(parsed == *fingerprint, "backup recipient is not canonical");
            raw_bytes = raw_bytes.saturating_add(fingerprint.as_str().len());
        }
    }
    ensure!(
        recipient_count <= MAX_BACKUP_RECIPIENTS,
        "backup contains too many recipients"
    );
    ensure!(
        raw_bytes <= MAX_BACKUP_PLAINTEXT_BYTES,
        "backup exceeds the maximum data size"
    );
    Ok(())
}

fn validate_folder_cycles(parents: &HashMap<FolderId, Option<FolderId>>) -> Result<()> {
    let mut state = HashMap::<FolderId, u8>::with_capacity(parents.len());
    for start in parents.keys().copied() {
        let mut current = Some(start);
        let mut path = Vec::new();
        while let Some(id) = current {
            match state.get(&id).copied().unwrap_or(0) {
                1 => bail!("backup folder hierarchy contains a cycle"),
                2 => break,
                _ => {}
            }
            state.insert(id, 1);
            path.push(id);
            current = parents.get(&id).copied().flatten();
        }
        for id in path {
            state.insert(id, 2);
        }
    }
    Ok(())
}

pub(super) fn validate_icon_resources(secrets: &[Secret], assets: &[EntryIconAsset]) -> Result<()> {
    ensure!(assets.len() <= MAX_BACKUP_ROWS, "too many icon resources");
    let mut ids = HashSet::new();
    let mut total = 0usize;
    for asset in assets {
        asset.validate()?;
        ensure!(ids.insert(asset.id), "duplicate icon resource");
        total = total.saturating_add(asset.png.len());
        ensure!(
            total <= MAX_BACKUP_PLAINTEXT_BYTES / 2,
            "icon resources exceed backup budget"
        );
        ensure!(
            crate::infrastructure::entry_icons::valid_normalized_image(&asset.png),
            "invalid icon image"
        );
    }
    let mut referenced = HashSet::new();
    for secret in secrets {
        if let Some(icon) = &secret.icon {
            icon.validate()?;
            if let EntryIcon::Image { asset_id } = icon {
                ensure!(ids.contains(asset_id), "missing icon resource");
                referenced.insert(*asset_id);
            }
        }
    }
    ensure!(ids == referenced, "unreferenced icon resource");
    Ok(())
}
