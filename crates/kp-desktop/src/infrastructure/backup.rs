//! Versioned, fully encrypted store backups and isolated restore publication.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use kryptopass_core::{
    EntryIcon, EntryIconAsset, Folder, FolderAppearance, FolderId, KeyFingerprint, OpenPgp,
    RecipientSet, Secret, SecretId, SecretKind, SecretLink,
};
use kryptopass_secure_memory::SensitiveBytes;
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;
use zeroize::Zeroize;

use super::GnuPgOpenPgp;
use super::sqlite_storage::{EncryptedStoreSnapshot, SqliteStorage};

const BACKUP_FORMAT: &str = "org.kryptopass.backup";
const BACKUP_VERSION: u32 = 4;
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const MAX_RECIPIENTS_PER_RECORD: usize = 64;
const MAX_SELECTED_RECIPIENTS: usize = 256;
const RESTORE_DATABASE_NAME: &str = "restored.sqlite3";

pub(super) const MAX_BACKUP_PLAINTEXT_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_BACKUP_CIPHERTEXT_BYTES: usize = 128 * 1024 * 1024 + 256 * 1024;
pub(super) const MAX_BACKUP_ROWS: usize = 10_000;
pub(super) const MAX_BACKUP_RECIPIENTS: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackupPublishOutcome {
    Durable,
    PublishedButDurabilityUnconfirmed,
}

/// Recovery categories exposed to the UI without decrypted data or GnuPG output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreFailure {
    Input,
    Destination,
    Decryption,
    InvalidBackup,
}

impl fmt::Display for RestoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "restore failed: {self:?}")
    }
}

impl std::error::Error for RestoreFailure {}

/// Writes a full store snapshot as one encrypted OpenPGP file.
pub fn export_backup(
    path: &Path,
    recipients: RecipientSet,
    crypto: &GnuPgOpenPgp,
) -> Result<BackupPublishOutcome> {
    ensure!(
        recipients.as_slice().len() <= MAX_SELECTED_RECIPIENTS,
        "too many backup recipients were selected"
    );
    crypto.validate_recipients(&recipients)?;

    let database_path = super::app_paths::database_path(crate::app_info::APP.desktop_file_id)?;
    let destination = prepare_destination(path, true)?;
    protect_store_target(&database_path, &destination)?;

    let storage = SqliteStorage::open_existing(database_path)?;
    let (folders, secrets, assets, links) = storage.backup_snapshot_with_links()?;
    let plaintext = serialize_backup_with_links(folders, secrets, assets, links)?;
    let ciphertext = crypto.encrypt_bounded(
        plaintext.as_slice(),
        &recipients,
        MAX_BACKUP_PLAINTEXT_BYTES,
        MAX_BACKUP_CIPHERTEXT_BYTES,
    )?;
    ensure!(
        !ciphertext.is_empty() && ciphertext.len() <= MAX_BACKUP_CIPHERTEXT_BYTES,
        "encrypted backup exceeds the maximum size"
    );
    write_backup_atomically(&destination, &ciphertext)
}

/// Decrypts and validates a backup, then publishes it as a new private SQLite store.
pub fn restore_backup(
    input: &Path,
    output: &Path,
    crypto: &GnuPgOpenPgp,
) -> Result<BackupPublishOutcome> {
    let database_path = super::app_paths::database_path(crate::app_info::APP.desktop_file_id)?;
    let input = prepare_existing_file(input).context(RestoreFailure::Input)?;
    let destination = prepare_destination(output, false).context(RestoreFailure::Destination)?;
    protect_store_target(&database_path, &destination).context(RestoreFailure::Destination)?;
    ensure!(
        !destination.exists(),
        "restore output already exists; choose a new vault file"
    );
    let parent = destination
        .parent()
        .context("restore destination has no parent")?;
    ensure_private_directory(parent).context(RestoreFailure::Destination)?;

    let ciphertext =
        read_bounded_file(&input, MAX_BACKUP_CIPHERTEXT_BYTES).context(RestoreFailure::Input)?;
    let plaintext = crypto
        .decrypt_bounded(
            &ciphertext,
            MAX_BACKUP_CIPHERTEXT_BYTES,
            MAX_BACKUP_PLAINTEXT_BYTES,
        )
        .context(RestoreFailure::Decryption)?;
    let (folders, secrets, assets, links) = deserialize_backup_with_links(plaintext.as_slice())
        .context(RestoreFailure::InvalidBackup)?;

    publish_restore_snapshot(&destination, &folders, &secrets, &assets, &links)
}

/// Publishes a restored snapshot into an isolated, new private store.
fn publish_restore_snapshot(
    output: &Path,
    folders: &[Folder],
    secrets: &[Secret],
    assets: &[EntryIconAsset],
    links: &[SecretLink],
) -> Result<BackupPublishOutcome> {
    let database_path = super::app_paths::database_path(crate::app_info::APP.desktop_file_id)?;
    let destination = prepare_destination(output, false)?;
    protect_store_target(&database_path, &destination)?;
    ensure!(!destination.exists(), "import destination already exists");
    let parent = destination
        .parent()
        .context("import destination has no parent")?;
    ensure_private_directory(parent)?;
    let stage = create_private_staging_directory(parent)?;
    let mut stage_guard = StagingDirectory(stage.clone());
    let staged_database = stage.join(RESTORE_DATABASE_NAME);
    let mut storage = SqliteStorage::open(&staged_database)?;
    storage.import_backup_snapshot_with_links(folders, secrets, assets, links)?;
    storage.finish_for_backup_publish()?;
    File::open(&staged_database)
        .and_then(|file| file.sync_all())
        .context("could not sync the restored vault")?;

    // A hard link publishes the fully closed database atomically and fails if
    // another process created the selected destination in the meantime.
    fs::hard_link(&staged_database, &destination)
        .context("could not publish the restored vault")?;
    let directory_synced = sync_directory(parent).is_ok();
    let database_removed = fs::remove_file(&staged_database).is_ok();
    let stage_removed = fs::remove_dir(&stage).is_ok();
    if database_removed && stage_removed {
        stage_guard.disarm();
    }
    Ok(if directory_synced && database_removed && stage_removed {
        BackupPublishOutcome::Durable
    } else {
        BackupPublishOutcome::PublishedButDurabilityUnconfirmed
    })
}

/// Parses the explicit recipient selection sent by the UI.
pub fn parse_selected_recipients(json: &str) -> Result<RecipientSet> {
    ensure!(
        json.len() <= 20 * 1024,
        "backup recipient selection is too large"
    );
    let values: Vec<String> = serde_json::from_str(json)
        .context("backup recipients must be a JSON array of fingerprints")?;
    ensure!(
        !values.is_empty() && values.len() <= MAX_SELECTED_RECIPIENTS,
        "select between one and 256 backup recipients"
    );
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
        fingerprints.iter().collect::<HashSet<_>>().len() == fingerprints.len(),
        "backup recipients must be unique"
    );
    RecipientSet::new(fingerprints)
        .map_err(|_| anyhow::anyhow!("backup recipient selection is invalid"))
}

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

#[derive(Serialize)]
#[serde(transparent)]
struct BoundedVec<T, const LIMIT: usize>(Vec<T>);

impl<T, const LIMIT: usize> BoundedVec<T, LIMIT> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T, const LIMIT: usize> Default for BoundedVec<T, LIMIT> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<'de, T, const LIMIT: usize> Deserialize<'de> for BoundedVec<T, LIMIT>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BoundedVisitor<T, const LIMIT: usize>(std::marker::PhantomData<T>);

        impl<'de, T, const LIMIT: usize> Visitor<'de> for BoundedVisitor<T, LIMIT>
        where
            T: Deserialize<'de>,
        {
            type Value = BoundedVec<T, LIMIT>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "an array with at most {LIMIT} items")
            }

            fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let capacity = sequence.size_hint().unwrap_or(0).min(LIMIT);
                let mut values = Vec::with_capacity(capacity);
                while let Some(value) = sequence.next_element()? {
                    if values.len() == LIMIT {
                        return Err(de::Error::invalid_length(values.len() + 1, &self));
                    }
                    values.push(value);
                }
                Ok(BoundedVec(values))
            }
        }

        deserializer.deserialize_seq(BoundedVisitor::<T, LIMIT>(std::marker::PhantomData))
    }
}

struct Base64Bytes(Vec<u8>);

impl Serialize for Base64Bytes {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64_encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Base64Bytes {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Base64Visitor;

        impl Visitor<'_> for Base64Visitor {
            type Value = Base64Bytes;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded base64 ciphertext string")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                base64_decode(value).map(Base64Bytes).map_err(E::custom)
            }

            fn visit_string<E>(self, mut value: String) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                let decoded = base64_decode(&value).map(Base64Bytes).map_err(E::custom);
                value.zeroize();
                decoded
            }
        }

        deserializer.deserialize_str(Base64Visitor)
    }
}

struct IconBytes(Vec<u8>);

impl Serialize for IconBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64_encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for IconBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct IconVisitor;
        impl Visitor<'_> for IconVisitor {
            type Value = IconBytes;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded base64 icon string")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<IconBytes, E> {
                if value.len() > kryptopass_core::MAX_ENTRY_ICON_BYTES.div_ceil(3) * 4 {
                    return Err(E::custom("icon exceeds maximum size"));
                }
                let bytes = base64_decode(value).map_err(E::custom)?;
                if bytes.len() > kryptopass_core::MAX_ENTRY_ICON_BYTES {
                    return Err(E::custom("icon exceeds maximum size"));
                }
                Ok(IconBytes(bytes))
            }
        }
        deserializer.deserialize_str(IconVisitor)
    }
}

#[cfg(test)]
fn serialize_backup(folders: Vec<Folder>, secrets: Vec<Secret>) -> Result<SensitiveBytes> {
    serialize_backup_with_icons(folders, secrets, Vec::new())
}

#[cfg(test)]
fn serialize_backup_with_icons(
    folders: Vec<Folder>,
    secrets: Vec<Secret>,
    assets: Vec<EntryIconAsset>,
) -> Result<SensitiveBytes> {
    serialize_backup_with_links(folders, secrets, assets, Vec::new())
}

fn serialize_backup_with_links(
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

struct BoundedBackupWriter {
    bytes: SensitiveBytes,
    maximum: usize,
}

impl BoundedBackupWriter {
    fn new(maximum: usize) -> Self {
        Self {
            bytes: SensitiveBytes::default(),
            maximum,
        }
    }

    fn into_bytes(mut self) -> SensitiveBytes {
        std::mem::take(&mut self.bytes)
    }
}

impl Write for BoundedBackupWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let length = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("backup size overflow"))?;
        if length > self.maximum {
            return Err(std::io::Error::other("backup exceeds maximum size"));
        }
        self.bytes.try_extend_from_slice(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
fn deserialize_backup(bytes: &[u8]) -> Result<(Vec<Folder>, Vec<Secret>)> {
    let (folders, secrets, _) = deserialize_backup_with_icons(bytes)?;
    Ok((folders, secrets))
}

#[cfg(test)]
fn deserialize_backup_with_icons(
    bytes: &[u8],
) -> Result<(Vec<Folder>, Vec<Secret>, Vec<EntryIconAsset>)> {
    let (folders, secrets, assets, _) = deserialize_backup_with_links(bytes)?;
    Ok((folders, secrets, assets))
}

fn deserialize_backup_with_links(bytes: &[u8]) -> Result<EncryptedStoreSnapshot> {
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
                && secret.ciphertext.0.len() <= super::MAX_CIPHERTEXT_BYTES,
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

fn validate_links(secrets: &[Secret], links: &[SecretLink]) -> Result<()> {
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

fn validate_domain_records(folders: &[Folder], secrets: &[Secret]) -> Result<()> {
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
            !secret.ciphertext.is_empty() && secret.ciphertext.len() <= super::MAX_CIPHERTEXT_BYTES,
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

fn base64_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(BASE64[(a >> 2) as usize] as char);
        output.push(BASE64[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            BASE64[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            BASE64[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn base64_decode(value: &str) -> std::result::Result<Vec<u8>, &'static str> {
    ensure_base64_limit(value)?;
    if value.is_empty() || value.len() % 4 != 0 {
        return Err("backup ciphertext encoding is invalid");
    }
    let padding = value
        .as_bytes()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    if padding > 2 {
        return Err("backup ciphertext encoding is invalid");
    }
    let decoded_len = value.len() / 4 * 3 - padding;
    if decoded_len == 0 || decoded_len > super::MAX_CIPHERTEXT_BYTES {
        return Err("backup ciphertext size is invalid");
    }
    let chunks = value.as_bytes().chunks_exact(4);
    let mut output = Vec::with_capacity(decoded_len);
    for (index, chunk) in chunks.enumerate() {
        let last = index + 1 == value.len() / 4;
        let a = base64_value(chunk[0]).ok_or("backup ciphertext encoding is invalid")?;
        let b = base64_value(chunk[1]).ok_or("backup ciphertext encoding is invalid")?;
        output.push((a << 2) | (b >> 4));
        if chunk[2] == b'=' {
            if !last || chunk[3] != b'=' || b & 0x0f != 0 {
                return Err("backup ciphertext encoding is invalid");
            }
        } else {
            let c = base64_value(chunk[2]).ok_or("backup ciphertext encoding is invalid")?;
            output.push((b << 4) | (c >> 2));
            if chunk[3] == b'=' {
                if !last || c & 0x03 != 0 {
                    return Err("backup ciphertext encoding is invalid");
                }
            } else {
                let d = base64_value(chunk[3]).ok_or("backup ciphertext encoding is invalid")?;
                output.push((c << 6) | d);
            }
        }
    }
    if output.len() != decoded_len {
        return Err("backup ciphertext encoding is invalid");
    }
    Ok(output)
}

fn ensure_base64_limit(value: &str) -> std::result::Result<(), &'static str> {
    if value.len() > super::MAX_CIPHERTEXT_BYTES.div_ceil(3) * 4 {
        Err("backup ciphertext size is invalid")
    } else {
        Ok(())
    }
}

fn base64_value(byte: u8) -> Option<u8> {
    BASE64
        .iter()
        .position(|candidate| *candidate == byte)
        .map(|i| i as u8)
}

fn prepare_destination(path: &Path, allow_existing_file: bool) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "backup path must be absolute");
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .context("backup path has no file name")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("/"));
    let parent = fs::canonicalize(parent).context("backup directory is unavailable")?;
    ensure!(
        open_real_directory(&parent)
            .context("backup parent is not a real directory")?
            .metadata()
            .context("could not inspect backup parent")?
            .is_dir(),
        "backup parent is not a directory"
    );
    let destination = parent.join(file_name);
    match fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            ensure!(
                allow_existing_file && metadata.is_file() && !metadata.file_type().is_symlink(),
                "backup destination already exists or is not a regular file"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => bail!("could not inspect backup destination"),
    }
    Ok(destination)
}

fn prepare_existing_file(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "backup path must be absolute");
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .context("backup path has no file name")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("/"));
    let parent = fs::canonicalize(parent).context("backup directory is unavailable")?;
    let file = parent.join(file_name);
    let metadata = fs::symlink_metadata(&file).context("backup file is unavailable")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "backup input is not a regular file"
    );
    Ok(file)
}

fn protect_store_target(database: &Path, destination: &Path) -> Result<()> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut candidate = database.as_os_str().to_os_string();
        candidate.push(suffix);
        ensure!(
            !same_path(Path::new(&candidate), destination),
            "backup path cannot replace the active store or its SQLite sidecars"
        );
    }

    if let Some(database_base) = sqlite_sidecar_base(destination) {
        ensure!(
            !file_has_sqlite_database_header(&database_base)?,
            "backup path cannot replace a SQLite database sidecar"
        );
    }

    if let Some(prefix) = read_file_prefix_if_present(destination)? {
        let sqlite_database = prefix.starts_with(b"SQLite format 3\0");
        let sqlite_wal = prefix
            .get(..4)
            .map(|magic| {
                magic == 0x377f_0682u32.to_be_bytes() || magic == 0x377f_0683u32.to_be_bytes()
            })
            .unwrap_or(false);
        let sqlite_journal = prefix.starts_with(&[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]);
        ensure!(
            !sqlite_database && !sqlite_wal && !sqlite_journal,
            "backup path cannot replace a SQLite database or journal"
        );
    }
    Ok(())
}

fn file_has_sqlite_database_header(path: &Path) -> Result<bool> {
    Ok(read_file_prefix_if_present(path)?
        .map(|prefix| prefix.starts_with(b"SQLite format 3\0"))
        .unwrap_or(false))
}

fn sqlite_sidecar_base(path: &Path) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let bytes = path.as_os_str().as_bytes();
        for suffix in [b"-wal".as_slice(), b"-shm", b"-journal"] {
            if let Some(base) = bytes.strip_suffix(suffix).filter(|base| !base.is_empty()) {
                return Some(PathBuf::from(OsString::from_vec(base.to_vec())));
            }
        }
        None
    }
    #[cfg(not(unix))]
    {
        let name = path.file_name()?.to_str()?;
        for suffix in ["-wal", "-shm", "-journal"] {
            if let Some(base) = name.strip_suffix(suffix).filter(|base| !base.is_empty()) {
                return Some(path.with_file_name(base));
            }
        }
        None
    }
}

fn read_file_prefix_if_present(path: &Path) -> Result<Option<Vec<u8>>> {
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("could not inspect backup destination"),
    };
    ensure!(
        before.is_file() && !before.file_type().is_symlink(),
        "backup destination is not a regular file"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .context("could not inspect backup destination")?;
    let opened = file
        .metadata()
        .context("could not inspect backup destination")?;
    ensure!(
        opened.is_file() && same_inode(&before, &opened),
        "backup destination changed while opening"
    );
    let mut prefix = Vec::with_capacity(16);
    file.take(16)
        .read_to_end(&mut prefix)
        .context("could not inspect backup destination")?;
    Ok(Some(prefix))
}

fn read_bounded_file(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let before = fs::symlink_metadata(path).context("backup file is unavailable")?;
    ensure!(
        before.is_file() && !before.file_type().is_symlink() && before.len() <= maximum as u64,
        "backup file exceeds the maximum size or is not regular"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    let file = options.open(path).context("could not open backup file")?;
    let opened = file.metadata().context("could not inspect backup file")?;
    ensure!(
        opened.is_file() && opened.len() <= maximum as u64 && same_inode(&before, &opened),
        "backup file changed while opening"
    );
    let mut bounded = file.take(maximum as u64 + 1);
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    bounded
        .read_to_end(&mut bytes)
        .context("could not read backup file")?;
    ensure!(
        bytes.len() <= maximum,
        "backup file exceeds the maximum size"
    );
    Ok(bytes)
}

fn write_backup_atomically(destination: &Path, bytes: &[u8]) -> Result<BackupPublishOutcome> {
    let parent = destination
        .parent()
        .context("backup destination has no parent")?;
    let temporary = parent.join(format!(".kryptopass-backup-{}.tmp", Uuid::new_v4()));
    let mut guard = TemporaryFile(temporary.clone());
    let mut file = create_private_file(&temporary)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .context("could not write encrypted backup")?;
    drop(file);
    fs::rename(&temporary, destination).context("could not publish encrypted backup")?;
    guard.disarm();
    Ok(if sync_directory(parent).is_ok() {
        BackupPublishOutcome::Durable
    } else {
        BackupPublishOutcome::PublishedButDurabilityUnconfirmed
    })
}

fn create_private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options
        .open(path)
        .context("could not create a private backup file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .context("could not secure backup file permissions")?;
    }
    Ok(file)
}

fn create_private_staging_directory(parent: &Path) -> Result<PathBuf> {
    for _ in 0..3 {
        let path = parent.join(format!(".kryptopass-restore-{}", Uuid::new_v4()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => bail!("could not create private restore staging directory"),
            }
        }
        #[cfg(not(unix))]
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => bail!("could not create private restore staging directory"),
        }
    }
    bail!("could not reserve a private restore staging directory")
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    let directory =
        open_real_directory(path).context("restore destination must be a private directory")?;
    let metadata = directory
        .metadata()
        .context("could not inspect restore destination")?;
    ensure!(metadata.is_dir(), "restore destination is not a directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        ensure!(
            metadata.uid() == unsafe { libc::geteuid() }
                && metadata.permissions().mode() & 0o7777 == 0o700,
            "restore destination must be owned by the current user with mode 0700"
        );
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    open_real_directory(path)
        .and_then(|directory| directory.sync_all())
        .context("could not sync backup directory")
}

fn open_real_directory(path: &Path) -> std::io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        File::open(path)
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
}

fn normalize_path(path: &Path) -> PathBuf {
    if let Ok(path) = fs::canonicalize(path) {
        return path;
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("/"));
    match (fs::canonicalize(parent), path.file_name()) {
        (Ok(parent), Some(file_name)) => parent.join(file_name),
        _ => path.to_path_buf(),
    }
}

fn same_inode(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        left.dev() == right.dev() && left.ino() == right.ino()
    }
    #[cfg(not(unix))]
    {
        left.file_type() == right.file_type() && left.len() == right.len()
    }
}

struct StagingDirectory(PathBuf);

impl StagingDirectory {
    fn disarm(&mut self) {
        self.0.clear();
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

struct TemporaryFile(PathBuf);

impl TemporaryFile {
    fn disarm(&mut self) {
        self.0.clear();
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/backup_order.rs"]
mod backup_order_tests;

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/backup_links.rs"]
mod backup_links_tests;

fn validate_icon_resources(secrets: &[Secret], assets: &[EntryIconAsset]) -> Result<()> {
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
            super::entry_icons::valid_normalized_image(&asset.png),
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

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/entry_icon.rs"]
mod entry_icon_tests;
