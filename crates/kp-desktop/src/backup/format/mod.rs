//! Versioned backup documents and conversion to encrypted store records.

mod encoding;
mod validation;

use anyhow::{Context, Result, bail, ensure};
use kryptopass_core::{
    EntryIcon, EntryIconAsset, Folder, FolderAppearance, FolderId, KeyFingerprint, RecipientSet,
    Secret, SecretId, SecretKind, SecretLink,
};
use kryptopass_secure_memory::SensitiveBytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::EncryptedStoreSnapshot;

use super::{MAX_BACKUP_PLAINTEXT_BYTES, MAX_BACKUP_ROWS};
use encoding::{Base64Bytes, BoundedBackupWriter, BoundedVec, IconBytes};
use validation::{validate_domain_records, validate_icon_resources, validate_links};

#[cfg(test)]
pub(super) use encoding::base64_decode;

const BACKUP_FORMAT: &str = "org.kryptopass.backup";
const BACKUP_VERSION: u32 = 4;
const MAX_RECIPIENTS_PER_RECORD: usize = 64;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupDocument {
    format: String,
    version: u32,
    folders: BoundedVec<BackupFolder, MAX_BACKUP_ROWS>,
    secrets: BoundedVec<BackupSecret, MAX_BACKUP_ROWS>,
    #[serde(default)]
    assets: BoundedVec<BackupIconAsset, MAX_BACKUP_ROWS>,
    #[serde(default, skip_serializing_if = "BoundedVec::is_empty")]
    links: BoundedVec<BackupLink, MAX_BACKUP_ROWS>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupLink {
    first: String,
    second: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupIconAsset {
    id: Uuid,
    png: IconBytes,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupFolder {
    id: String,
    parent_id: Option<String>,
    #[serde(default)]
    sort_position: i64,
    name: String,
    icon: String,
    color: String,
    default_recipients: Option<BoundedVec<String, MAX_RECIPIENTS_PER_RECORD>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupSecret {
    id: String,
    folder_id: Option<String>,
    title: String,
    kind: String,
    recipients: BoundedVec<String, MAX_RECIPIENTS_PER_RECORD>,
    ciphertext: Base64Bytes,
    favorite: bool,
    trashed: bool,
    revision: u64,
    #[serde(default)]
    icon: Option<EntryIcon>,
}

#[cfg(test)]
pub(super) fn serialize_backup(
    folders: Vec<Folder>,
    secrets: Vec<Secret>,
) -> Result<SensitiveBytes> {
    serialize_backup_with_icons(folders, secrets, Vec::new())
}

#[cfg(test)]
pub(super) fn serialize_backup_with_icons(
    folders: Vec<Folder>,
    secrets: Vec<Secret>,
    assets: Vec<EntryIconAsset>,
) -> Result<SensitiveBytes> {
    serialize_backup_with_links(folders, secrets, assets, Vec::new())
}

pub(super) fn serialize_backup_with_links(
    folders: Vec<Folder>,
    secrets: Vec<Secret>,
    assets: Vec<EntryIconAsset>,
    links: Vec<SecretLink>,
) -> Result<SensitiveBytes> {
    validate_icon_resources(&secrets, &assets)?;
    validate_domain_records(&folders, &secrets)?;
    validate_links(&secrets, &links)?;
    let document = BackupDocument {
        format: BACKUP_FORMAT.to_owned(),
        version: if links.is_empty() { 3 } else { BACKUP_VERSION },
        links: BoundedVec(
            links
                .into_iter()
                .map(|link| BackupLink {
                    first: link.first.0.min(link.second.0).to_string(),
                    second: link.first.0.max(link.second.0).to_string(),
                })
                .collect(),
        ),
        assets: BoundedVec(
            assets
                .into_iter()
                .map(|asset| BackupIconAsset {
                    id: asset.id,
                    png: IconBytes(asset.png),
                })
                .collect(),
        ),
        folders: BoundedVec(
            folders
                .into_iter()
                .map(|folder| BackupFolder {
                    id: folder.id.0.to_string(),
                    parent_id: folder.parent_id.map(|id| id.0.to_string()),
                    sort_position: folder.sort_position,
                    name: folder.name,
                    icon: folder.appearance.icon().to_owned(),
                    color: folder.appearance.color().to_owned(),
                    default_recipients: folder.default_recipients.map(|recipients| {
                        BoundedVec(
                            recipients
                                .as_slice()
                                .iter()
                                .map(|fingerprint| fingerprint.as_str().to_owned())
                                .collect(),
                        )
                    }),
                })
                .collect(),
        ),
        secrets: BoundedVec(
            secrets
                .into_iter()
                .map(|secret| BackupSecret {
                    id: secret.id.0.to_string(),
                    folder_id: secret.folder_id.map(|id| id.0.to_string()),
                    title: secret.title,
                    kind: secret.kind.as_str().to_owned(),
                    recipients: BoundedVec(
                        secret
                            .recipients
                            .as_slice()
                            .iter()
                            .map(|fingerprint| fingerprint.as_str().to_owned())
                            .collect(),
                    ),
                    ciphertext: Base64Bytes(secret.ciphertext),
                    favorite: secret.favorite,
                    trashed: secret.trashed,
                    revision: secret.revision,
                    icon: secret.icon,
                })
                .collect(),
        ),
    };

    let mut writer = BoundedBackupWriter::new(MAX_BACKUP_PLAINTEXT_BYTES);
    serde_json::to_writer(&mut writer, &document).context("could not encode backup data")?;
    Ok(writer.into_bytes())
}

#[cfg(test)]
pub(super) fn deserialize_backup(bytes: &[u8]) -> Result<(Vec<Folder>, Vec<Secret>)> {
    let (folders, secrets, _) = deserialize_backup_with_icons(bytes)?;
    Ok((folders, secrets))
}

#[cfg(test)]
pub(super) fn deserialize_backup_with_icons(
    bytes: &[u8],
) -> Result<(Vec<Folder>, Vec<Secret>, Vec<EntryIconAsset>)> {
    let (folders, secrets, assets, _) = deserialize_backup_with_links(bytes)?;
    Ok((folders, secrets, assets))
}

pub(super) fn deserialize_backup_with_links(bytes: &[u8]) -> Result<EncryptedStoreSnapshot> {
    ensure!(
        bytes.len() <= MAX_BACKUP_PLAINTEXT_BYTES,
        "backup exceeds the maximum size"
    );
    let document: BackupDocument =
        serde_json::from_slice(bytes).context("backup document is invalid")?;
    ensure!(
        document.format == BACKUP_FORMAT && matches!(document.version, 1 | 2 | 3 | BACKUP_VERSION),
        "backup format or version is unsupported"
    );
    ensure!(
        document.version >= 4 || document.links.0.is_empty(),
        "links require backup version 4"
    );
    let links = document
        .links
        .0
        .into_iter()
        .map(|link| {
            let first = parse_secret_id(&link.first)?;
            let second = parse_secret_id(&link.second)?;
            Ok(SecretLink {
                first: SecretId(first.0.min(second.0)),
                second: SecretId(first.0.max(second.0)),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        document
            .folders
            .0
            .len()
            .saturating_add(document.secrets.0.len())
            <= MAX_BACKUP_ROWS,
        "backup contains too many records"
    );

    ensure!(
        document.version >= 3
            || (document.assets.0.is_empty()
                && document
                    .secrets
                    .0
                    .iter()
                    .all(|secret| secret.icon.is_none())),
        "icons require backup version 3"
    );
    let assets: Vec<_> = document
        .assets
        .0
        .into_iter()
        .map(|asset| EntryIconAsset {
            id: asset.id,
            png: asset.png.0,
        })
        .collect();
    let mut folders = Vec::with_capacity(document.folders.0.len());
    for folder in document.folders.0 {
        let id = parse_folder_id(&folder.id)?;
        let parent_id = folder
            .parent_id
            .as_deref()
            .map(parse_folder_id)
            .transpose()?;
        let appearance = FolderAppearance::new(folder.icon, folder.color)
            .map_err(|_| anyhow::anyhow!("backup folder appearance is invalid"))?;
        let default_recipients = folder
            .default_recipients
            .map(|set| parse_recipients(set.0, false))
            .transpose()?
            .flatten();
        folders.push(Folder {
            id,
            parent_id,
            sort_position: folder.sort_position,
            name: folder.name,
            appearance,
            default_recipients,
        });
    }

    let mut secrets = Vec::with_capacity(document.secrets.0.len());
    for secret in document.secrets.0 {
        let id = parse_secret_id(&secret.id)?;
        let folder_id = secret
            .folder_id
            .as_deref()
            .map(parse_folder_id)
            .transpose()?;
        let kind = SecretKind::parse(&secret.kind)
            .map_err(|_| anyhow::anyhow!("backup secret kind is unsupported"))?;
        let recipients = parse_recipients(secret.recipients.0, true)?
            .ok_or_else(|| anyhow::anyhow!("backup secret recipients are missing"))?;
        ensure!(
            !secret.ciphertext.0.is_empty()
                && secret.ciphertext.0.len() <= crate::infrastructure::MAX_CIPHERTEXT_BYTES,
            "backup secret ciphertext is invalid"
        );
        ensure!(
            secret.revision <= i64::MAX as u64,
            "backup secret revision is invalid"
        );
        secrets.push(Secret {
            id,
            folder_id,
            title: secret.title,
            kind,
            recipients,
            ciphertext: secret.ciphertext.0,
            favorite: secret.favorite,
            trashed: secret.trashed,
            revision: secret.revision,
            icon: secret.icon,
        });
    }

    validate_domain_records(&folders, &secrets)?;
    validate_icon_resources(&secrets, &assets)?;
    validate_links(&secrets, &links)?;
    Ok((folders, secrets, assets, links))
}

fn parse_recipients(values: Vec<String>, required: bool) -> Result<Option<RecipientSet>> {
    if values.is_empty() {
        ensure!(!required, "backup recipients are missing");
        bail!("backup recipient list must be null when empty");
    }
    let fingerprints = values
        .iter()
        .map(|value| {
            let fingerprint = KeyFingerprint::parse(value)
                .map_err(|_| anyhow::anyhow!("backup recipient fingerprint is invalid"))?;
            ensure!(
                fingerprint.as_str() == value,
                "backup recipient is not canonical"
            );
            Ok(fingerprint)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        fingerprints.windows(2).all(|pair| pair[0] < pair[1]),
        "backup recipients are duplicated or unsorted"
    );
    RecipientSet::new(fingerprints)
        .map(Some)
        .map_err(|_| anyhow::anyhow!("backup recipient metadata is invalid"))
}

fn parse_folder_id(value: &str) -> Result<FolderId> {
    let id = Uuid::parse_str(value).map_err(|_| anyhow::anyhow!("backup folder id is invalid"))?;
    ensure!(id.to_string() == value, "backup folder id is not canonical");
    Ok(FolderId(id))
}

fn parse_secret_id(value: &str) -> Result<SecretId> {
    let id = Uuid::parse_str(value).map_err(|_| anyhow::anyhow!("backup secret id is invalid"))?;
    ensure!(id.to_string() == value, "backup secret id is not canonical");
    Ok(SecretId(id))
}
