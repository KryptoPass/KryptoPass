//! Application facade for folder and secret use cases.

use crate::SensitiveBytes;
use anyhow::{Context, Result, ensure};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

use crate::domain::content::LoginHistoryEventKind;
use crate::domain::{
    Folder, FolderAppearance, FolderId, RecipientSet, Secret, SecretId, SecretKind,
    SecretLinkError, SecretMetadata,
    content::{MAX_CONTENT_BYTES, SecretContent, decode_content, encode_content},
};
use crate::ports::{OpenPgp, Storage};

const MAX_TITLE_BYTES: usize = 512;
const MAX_FOLDER_NAME_BYTES: usize = 256;

/// How a new secret obtains its explicit recipient set.
pub enum RecipientSelection {
    Explicit(RecipientSet),
    FolderDefault,
}

/// Input for creating one encrypted secret.
pub struct CreateSecret {
    pub folder_id: Option<FolderId>,
    pub title: String,
    pub kind: SecretKind,
    pub recipients: RecipientSelection,
    pub plaintext: SensitiveBytes,
}

pub struct SaveContentRequest {
    pub id: Option<SecretId>,
    pub folder_id: Option<FolderId>,
    pub title: String,
    pub recipients: RecipientSelection,
    pub content: SecretContent,
    pub expected_revision: Option<u64>,
}

pub struct SecretsService<S, G> {
    storage: S,
    crypto: G,
}

impl<S: Storage, G: OpenPgp> SecretsService<S, G> {
    pub fn new(storage: S, crypto: G) -> Self {
        Self { storage, crypto }
    }

    pub fn create_folder(
        &mut self,
        name: String,
        parent_id: Option<FolderId>,
        default_recipients: Option<RecipientSet>,
    ) -> Result<FolderId> {
        self.create_folder_with_appearance(
            name,
            parent_id,
            default_recipients,
            FolderAppearance::default(),
        )
    }

    pub fn create_folder_with_appearance(
        &mut self,
        name: String,
        parent_id: Option<FolderId>,
        default_recipients: Option<RecipientSet>,
        appearance: FolderAppearance,
    ) -> Result<FolderId> {
        let name = validate_folder_name(&name)?;

        if let Some(parent) = parent_id {
            self.storage
                .find_folder(parent)?
                .context("parent folder does not exist")?;
        }

        let id = FolderId::new();

        self.storage.insert_folder(Folder {
            id,
            parent_id,
            sort_position: 0,
            name,
            appearance,
            default_recipients,
        })?;

        Ok(id)
    }

    /// Low-level byte API retained for callers that own their payload format.
    /// Desktop editors use `save_content` to enforce the versioned typed contract.
    pub fn create_secret(&mut self, request: CreateSecret) -> Result<SecretId> {
        let title = request.title.trim();
        ensure!(!title.is_empty(), "secret title cannot be empty");
        ensure!(
            title.len() <= MAX_TITLE_BYTES,
            "secret title exceeds the maximum size"
        );
        ensure!(
            request.plaintext.len() <= MAX_CONTENT_BYTES,
            "secret content exceeds the maximum size"
        );

        let folder = request
            .folder_id
            .map(|id| {
                self.storage
                    .find_folder(id)?
                    .context("destination folder does not exist")
            })
            .transpose()?;

        let recipients = match request.recipients {
            RecipientSelection::Explicit(recipients) => recipients,
            RecipientSelection::FolderDefault => folder
                .and_then(|folder| folder.default_recipients)
                .context("no default recipients; select keys explicitly")?,
        };

        self.crypto.validate_recipients(&recipients)?;

        let ciphertext = self
            .crypto
            .encrypt(request.plaintext.as_slice(), &recipients)?;

        let id = SecretId::new();

        self.storage.insert_secret(Secret {
            icon: None,
            id,
            folder_id: request.folder_id,
            title: title.to_owned(),
            kind: request.kind,
            recipients,
            ciphertext,
            favorite: false,
            trashed: false,
            revision: 0,
        })?;

        Ok(id)
    }

    pub fn read_secret(&self, id: SecretId) -> Result<SensitiveBytes> {
        let secret = self
            .storage
            .find_secret(id)?
            .context("secret does not exist")?;

        self.crypto.decrypt(&secret.ciphertext)
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        self.storage.list_folders()
    }

    pub fn list_secrets(&self) -> Result<Vec<Secret>> {
        self.storage.list_secrets()
    }

    pub fn list_secret_metadata(&self) -> Result<Vec<SecretMetadata>> {
        self.storage.list_secret_metadata()
    }

    /// Creates or replaces a secret after validating its metadata and encrypting
    /// the bounded, typed payload. Existing flags survive content edits.
    pub fn save_content(
        &mut self,
        id: Option<SecretId>,
        folder_id: Option<FolderId>,
        title: String,
        recipients: RecipientSelection,
        content: SecretContent,
        expected_revision: Option<u64>,
    ) -> Result<SecretId> {
        self.save_content_impl(
            SaveContentRequest {
                id,
                folder_id,
                title,
                recipients,
                content,
                expected_revision,
            },
            None,
            None,
        )
    }

    pub fn save_content_with_appearance(
        &mut self,
        request: SaveContentRequest,
        appearance: crate::EntryAppearanceUpdate,
    ) -> Result<SecretId> {
        appearance.validate()?;
        self.save_content_impl(request, Some(appearance), None)
    }

    /// Saves typed content and sets or clears the item's one companion link in
    /// the same storage transaction as the content update.
    pub fn save_content_with_link(
        &mut self,
        request: SaveContentRequest,
        appearance: Option<crate::EntryAppearanceUpdate>,
        related: Option<SecretId>,
    ) -> Result<SecretId> {
        if let Some(appearance) = &appearance {
            appearance.validate()?;
        }
        self.save_content_impl(request, appearance, Some(related))
    }

    fn save_content_impl(
        &mut self,
        request: SaveContentRequest,
        appearance: Option<crate::EntryAppearanceUpdate>,
        relationship: Option<Option<SecretId>>,
    ) -> Result<SecretId> {
        let SaveContentRequest {
            id,
            folder_id,
            title,
            recipients,
            mut content,
            expected_revision,
        } = request;
        let title = title.trim();
        ensure!(!title.is_empty(), "secret title cannot be empty");
        ensure!(
            title.len() <= MAX_TITLE_BYTES,
            "secret title exceeds the maximum size"
        );

        let existing = id
            .map(|secret_id| {
                self.storage
                    .find_secret(secret_id)?
                    .context("secret does not exist")
            })
            .transpose()?;
        match (existing.as_ref(), expected_revision) {
            (Some(secret), Some(expected)) => ensure!(
                secret.revision == expected,
                "secret changed since it was opened; reload before saving"
            ),
            (Some(_), None) => anyhow::bail!("an edit requires its expected revision"),
            (None, Some(_)) => anyhow::bail!("a new secret cannot have an expected revision"),
            (None, None) => {}
        }
        let secret_id = id.unwrap_or_default();
        if let Some(related) = relationship {
            let links = self.storage.list_secret_links()?;
            let current_target = links
                .iter()
                .find(|link| link.first == secret_id || link.second == secret_id)
                .map(|link| {
                    if link.first == secret_id {
                        link.second
                    } else {
                        link.first
                    }
                });
            if related != current_target {
                if let Some(target_id) = related {
                    if existing.as_ref().is_some_and(|secret| secret.trashed)
                        || target_id == secret_id
                    {
                        return Err(SecretLinkError::InvalidTarget.into());
                    }
                    let target = self
                        .storage
                        .list_secret_metadata()?
                        .into_iter()
                        .find(|secret| secret.id == target_id)
                        .ok_or(SecretLinkError::InvalidTarget)?;
                    if target.trashed {
                        return Err(SecretLinkError::InvalidTarget.into());
                    }
                    if let Some(link) = links
                        .iter()
                        .find(|link| link.first == target_id || link.second == target_id)
                    {
                        let current_companion = if link.first == target_id {
                            link.second
                        } else {
                            link.first
                        };
                        if current_companion != secret_id {
                            return Err(SecretLinkError::Occupied.into());
                        }
                    }
                }
            }
        }
        if let Some(secret) = &existing {
            ensure!(
                secret.kind == content.kind(),
                "an existing secret cannot change its content kind"
            );
        }
        let folder = folder_id
            .map(|folder_id| {
                self.storage
                    .find_folder(folder_id)?
                    .context("destination folder does not exist")
            })
            .transpose()?;

        let recipients = match recipients {
            RecipientSelection::Explicit(recipients) => recipients,
            RecipientSelection::FolderDefault => folder
                .and_then(|folder| folder.default_recipients)
                .context("no default recipients; select keys explicitly")?,
        };
        self.crypto.validate_recipients(&recipients)?;

        content.validate()?;
        let mut previous_content = if content.needs_previous_content() {
            existing
                .as_ref()
                .map(|secret| self.decrypt_content(secret))
                .transpose()?
        } else {
            None
        };
        let metadata_changed = existing.as_ref().is_some_and(|secret| {
            secret.title != title
                || secret.folder_id != folder_id
                || secret.recipients != recipients
        });
        content.record_save(
            previous_content.as_mut(),
            existing.as_ref().map_or("", |secret| secret.title.as_str()),
            metadata_changed,
            unix_seconds(),
        )?;

        let plaintext = encode_content(&content)?;
        let ciphertext = self.crypto.encrypt(plaintext.as_slice(), &recipients)?;
        let secret = Secret {
            icon: match appearance.as_ref().map(|update| update.icon.as_ref()) {
                Some(Some(
                    icon @ crate::EntryIcon::Theme { .. } | icon @ crate::EntryIcon::Image { .. },
                )) => Some(icon.clone()),
                Some(None | Some(crate::EntryIcon::Favicon { .. })) => {
                    crate::automatic_entry_icon(&content)
                }
                None => match existing.as_ref().and_then(|secret| secret.icon.as_ref()) {
                    Some(
                        icon @ crate::EntryIcon::Theme { .. }
                        | icon @ crate::EntryIcon::Image { .. },
                    ) => Some(icon.clone()),
                    _ => crate::automatic_entry_icon(&content),
                },
            },
            id: secret_id,
            folder_id,
            title: title.to_owned(),
            kind: content.kind(),
            recipients,
            ciphertext,
            favorite: existing.as_ref().is_some_and(|secret| secret.favorite),
            trashed: existing.as_ref().is_some_and(|secret| secret.trashed),
            revision: existing.as_ref().map_or(0, |secret| secret.revision),
        };

        let asset = appearance.and_then(|update| update.asset);
        if existing.is_some() {
            let expected_revision =
                expected_revision.context("an edit requires its expected revision")?;
            if let Some(related) = relationship {
                self.storage
                    .replace_secret_with_link(secret, expected_revision, asset, related)?;
            } else {
                self.storage
                    .replace_secret_with_icon(secret, expected_revision, asset)?;
            }
        } else if let Some(related) = relationship {
            self.storage
                .insert_secret_with_link(secret, asset, related)?;
        } else {
            self.storage.insert_secret_with_icon(secret, asset)?;
        }

        Ok(secret_id)
    }

    pub fn read_content(&self, id: SecretId) -> Result<SecretContent> {
        let secret = self
            .storage
            .find_secret(id)?
            .context("secret does not exist")?;
        self.decrypt_content(&secret)
    }

    fn decrypt_content(&self, secret: &Secret) -> Result<SecretContent> {
        let plaintext = self.crypto.decrypt(&secret.ciphertext)?;
        let content = decode_content(plaintext.as_slice())?;
        ensure!(
            content.kind() == secret.kind,
            "secret metadata and content kinds do not match"
        );
        Ok(content)
    }

    /// Persists a verified cached login payload with one copy event.
    ///
    /// The caller supplies the decrypted content snapshot and the revision it
    /// was read from, so recording a copy does not decrypt the old ciphertext
    /// or prompt for a private key after the clipboard operation.
    pub fn record_login_event(
        &mut self,
        id: SecretId,
        expected_revision: u64,
        mut content: SecretContent,
        kind: LoginHistoryEventKind,
        timestamp: u64,
    ) -> Result<SecretContent> {
        ensure!(
            matches!(
                kind,
                LoginHistoryEventKind::CopyUsername
                    | LoginHistoryEventKind::CopyPassword
                    | LoginHistoryEventKind::CopyTotp
                    | LoginHistoryEventKind::CopyNextTotp
            ),
            "only successful copy events can be recorded through this operation"
        );

        let mut secret = self
            .storage
            .find_secret(id)?
            .context("secret does not exist")?;
        ensure!(
            secret.revision == expected_revision,
            "secret changed after its cached content was opened"
        );
        ensure!(
            secret.kind == SecretKind::Login,
            "copy history can only be recorded for login entries"
        );
        let SecretContent::Login(login) = &mut content else {
            anyhow::bail!("copy history requires cached login content");
        };

        login.history.record_event(kind, timestamp);
        let plaintext = encode_content(&content)?;
        let ciphertext = self
            .crypto
            .encrypt(plaintext.as_slice(), &secret.recipients)?;
        secret.ciphertext.zeroize();
        secret.ciphertext = ciphertext;
        self.storage.replace_secret(secret, expected_revision)?;
        Ok(content)
    }

    pub fn update_flags(
        &mut self,
        id: SecretId,
        favorite: Option<bool>,
        trashed: Option<bool>,
    ) -> Result<()> {
        if favorite.is_none() && trashed.is_none() {
            return Ok(());
        }
        let mut secret = self
            .storage
            .find_secret(id)?
            .context("secret does not exist")?;
        if let Some(favorite) = favorite {
            secret.favorite = favorite;
        }
        if let Some(trashed) = trashed {
            secret.trashed = trashed;
        }
        let expected_revision = secret.revision;
        self.storage.replace_secret(secret, expected_revision)
    }

    pub fn rename_folder(&mut self, id: FolderId, name: String) -> Result<()> {
        let name = validate_folder_name(&name)?;

        let mut folder = self
            .storage
            .find_folder(id)?
            .context("folder does not exist")?;
        folder.name = name;
        self.storage.replace_folder(folder)
    }

    pub fn update_folder_details(
        &mut self,
        id: FolderId,
        name: String,
        appearance: FolderAppearance,
    ) -> Result<()> {
        let name = validate_folder_name(&name)?;
        let mut folder = self
            .storage
            .find_folder(id)?
            .context("folder does not exist")?;
        folder.name = name;
        folder.appearance = appearance;
        self.storage.replace_folder(folder)
    }

    /// Storage removes the folder and atomically moves its children to its
    /// former parent. This method validates the referenced folder hierarchy.
    pub fn remove_folder(&mut self, id: FolderId) -> Result<()> {
        let folder = self
            .storage
            .find_folder(id)?
            .context("folder does not exist")?;
        if let Some(parent_id) = folder.parent_id {
            self.storage
                .find_folder(parent_id)?
                .context("parent folder does not exist")?;
        }
        self.storage.remove_folder(id)
    }

    /// Permanently removes the folder, all descendants and their secrets in
    /// one storage transaction, including secrets already in the trash.
    pub fn remove_folder_tree(&mut self, id: FolderId) -> Result<()> {
        self.storage.remove_folder_tree(id)
    }

    /// Moves a whole subtree; storage validates hierarchy and ordering atomically.
    pub fn move_folder(
        &mut self,
        id: FolderId,
        destination: Option<FolderId>,
        before: Option<FolderId>,
    ) -> Result<()> {
        self.storage.move_folder(id, destination, before)
    }

    pub fn remove_secret(&mut self, id: SecretId) -> Result<()> {
        self.storage
            .find_secret(id)?
            .context("secret does not exist")?;
        self.storage.remove_secret(id)
    }

    pub fn move_secret(&mut self, id: SecretId, destination: Option<FolderId>) -> Result<()> {
        self.storage
            .find_secret(id)?
            .context("secret does not exist")?;
        if let Some(folder_id) = destination {
            self.storage
                .find_folder(folder_id)?
                .context("destination folder does not exist")?;
        }

        self.storage.move_secret(id, destination)
    }

    pub fn change_recipients(&mut self, id: SecretId, recipients: RecipientSet) -> Result<()> {
        let secret = self
            .storage
            .find_secret(id)?
            .context("secret does not exist")?;
        let expected_revision = secret.revision;

        self.crypto.validate_recipients(&recipients)?;

        let plaintext = self.crypto.decrypt(&secret.ciphertext)?;
        let ciphertext = self.crypto.encrypt(plaintext.as_slice(), &recipients)?;

        self.storage
            .replace_encryption(id, ciphertext, recipients, expected_revision)
    }
}

fn validate_folder_name(name: &str) -> Result<String> {
    let name = name.trim();
    ensure!(!name.is_empty(), "folder name cannot be empty");
    ensure!(
        name.len() <= MAX_FOLDER_NAME_BYTES,
        "folder name exceeds the maximum size"
    );
    Ok(name.to_owned())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl<S: crate::ports::ImportStorage, G: OpenPgp> crate::import::ImportReceiver
    for SecretsService<S, G>
{
    fn receive_import(
        &mut self,
        batch: crate::import::ImportBatch,
        options: crate::import::ImportOptions,
    ) -> Result<crate::import::ImportReceipt> {
        use crate::import::{ImportFolderPolicy, ImportReceipt};
        batch.validate()?;
        ensure!(
            options.recipients.as_slice().len() <= 64,
            "too many import recipients"
        );
        let destination = match options.folders {
            ImportFolderPolicy::PreserveHierarchy { parent } => parent,
            ImportFolderPolicy::PlaceAll { folder } => folder,
        };
        if let Some(id) = destination {
            self.storage
                .find_folder(id)?
                .context("import destination folder does not exist")?;
        }
        self.crypto.validate_recipients(&options.recipients)?;
        let ids: Vec<_> = batch.folders.iter().map(|_| FolderId::new()).collect();
        let folders = match options.folders {
            ImportFolderPolicy::PreserveHierarchy { parent } => batch
                .folders
                .iter()
                .enumerate()
                .map(|(index, folder)| Folder {
                    id: ids[index],
                    parent_id: folder.parent.map(|index| ids[index]).or(parent),
                    sort_position: index as i64,
                    name: folder.name.trim().to_owned(),
                    appearance: FolderAppearance::default(),
                    default_recipients: Some(options.recipients.clone()),
                })
                .collect(),
            ImportFolderPolicy::PlaceAll { .. } => Vec::new(),
        };
        let mut secrets = Vec::with_capacity(batch.items.len());
        for item in &batch.items {
            let plaintext = encode_content(&item.content)?;
            let ciphertext = self.crypto.encrypt(&plaintext, &options.recipients)?;
            ensure!(
                !ciphertext.is_empty(),
                "import encryption produced no ciphertext"
            );
            let folder_id = match options.folders {
                ImportFolderPolicy::PreserveHierarchy { .. } => Some(ids[item.folder]),
                ImportFolderPolicy::PlaceAll { folder } => folder,
            };
            secrets.push(Secret {
                icon: match &item.icon {
                    Some(icon) => {
                        icon.validate()?;
                        Some(icon.clone())
                    }
                    None => crate::automatic_entry_icon(&item.content),
                },
                id: SecretId::new(),
                folder_id,
                title: item.title.trim().to_owned(),
                kind: item.content.kind(),
                recipients: options.recipients.clone(),
                ciphertext,
                favorite: item.favorite,
                trashed: item.trashed,
                revision: 0,
            });
        }
        let links: Vec<_> = batch
            .links
            .iter()
            .map(|&(first, second)| {
                let (first, second) = (secrets[first].id, secrets[second].id);
                // Links are stored with endpoints in UUID order.
                if first.0 < second.0 {
                    crate::SecretLink { first, second }
                } else {
                    crate::SecretLink {
                        first: second,
                        second: first,
                    }
                }
            })
            .collect();
        self.storage.append_import(&folders, &secrets, &links)?;
        Ok(ImportReceipt {
            folders_added: folders.len(),
            items_added: secrets.len(),
        })
    }
}
