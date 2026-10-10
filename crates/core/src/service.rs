//! Application facade for folder and secret use cases.

use anyhow::{Context, Result, ensure};
use zeroize::Zeroizing;

use crate::domain::{
    Folder, FolderAppearance, FolderId, RecipientSet, Secret, SecretId, SecretKind, SecretMetadata,
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
    pub plaintext: Zeroizing<Vec<u8>>,
}

/// Coordinates domain rules, persistence, and OpenPGP without knowing either adapter.
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

    pub fn read_secret(&self, id: SecretId) -> Result<Zeroizing<Vec<u8>>> {
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

        let plaintext = encode_content(&content)?;
        let ciphertext = self.crypto.encrypt(plaintext.as_slice(), &recipients)?;
        let secret_id = id.unwrap_or_default();
        let secret = Secret {
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

        if existing.is_some() {
            let expected_revision =
                expected_revision.context("an edit requires its expected revision")?;
            self.storage.replace_secret(secret, expected_revision)?;
        } else {
            self.storage.insert_secret(secret)?;
        }

        Ok(secret_id)
    }

    pub fn read_content(&self, id: SecretId) -> Result<SecretContent> {
        let secret = self
            .storage
            .find_secret(id)?
            .context("secret does not exist")?;
        let plaintext = self.crypto.decrypt(&secret.ciphertext)?;
        let content = decode_content(plaintext.as_slice())?;
        ensure!(
            content.kind() == secret.kind,
            "secret metadata and content kinds do not match"
        );
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
