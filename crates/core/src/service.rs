//! Application facade for folder and secret use cases.

use anyhow::{Context, Result, ensure};
use zeroize::Zeroizing;

use crate::domain::{Folder, FolderId, RecipientSet, Secret, SecretId, SecretKind};
use crate::ports::{OpenPgp, Storage};

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
        ensure!(!name.trim().is_empty(), "folder name cannot be empty");

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
            default_recipients,
        })?;

        Ok(id)
    }

    pub fn create_secret(&mut self, request: CreateSecret) -> Result<SecretId> {
        ensure!(
            !request.title.trim().is_empty(),
            "secret title cannot be empty"
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
            title: request.title,
            kind: request.kind,
            recipients,
            ciphertext,
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

    pub fn move_secret(&mut self, id: SecretId, destination: Option<FolderId>) -> Result<()> {
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

        self.crypto.validate_recipients(&recipients)?;

        let plaintext = self.crypto.decrypt(&secret.ciphertext)?;
        let ciphertext = self.crypto.encrypt(plaintext.as_slice(), &recipients)?;

        self.storage.replace_encryption(id, ciphertext, recipients)
    }
}
