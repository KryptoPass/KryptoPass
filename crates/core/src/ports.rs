//! Infrastructure capabilities required by the application service.

use anyhow::Result;
use zeroize::Zeroizing;

use crate::domain::{Folder, FolderId, RecipientSet, Secret, SecretId, SecretMetadata};

/// Persistence boundary for the local secret store.
pub trait Storage {
    fn insert_secret_with_icon(
        &mut self,
        secret: Secret,
        asset: Option<crate::EntryIconAsset>,
    ) -> Result<()> {
        anyhow::ensure!(asset.is_none(), "storage does not support image resources");
        self.insert_secret(secret)
    }

    fn replace_secret_with_icon(
        &mut self,
        secret: Secret,
        expected_revision: u64,
        asset: Option<crate::EntryIconAsset>,
    ) -> Result<()> {
        anyhow::ensure!(asset.is_none(), "storage does not support image resources");
        self.replace_secret(secret, expected_revision)
    }

    fn list_folders(&self) -> Result<Vec<Folder>>;

    fn list_secrets(&self) -> Result<Vec<Secret>>;

    /// Reads list metadata without loading encrypted payload bytes.
    fn list_secret_metadata(&self) -> Result<Vec<SecretMetadata>>;

    fn find_folder(&self, id: FolderId) -> Result<Option<Folder>>;

    fn find_secret(&self, id: SecretId) -> Result<Option<Secret>>;

    fn insert_folder(&mut self, folder: Folder) -> Result<()>;

    fn insert_secret(&mut self, secret: Secret) -> Result<()>;

    /// Replaces one secret only when its currently stored revision matches
    /// `expected_revision`, then persists the next revision atomically.
    fn replace_secret(&mut self, secret: Secret, expected_revision: u64) -> Result<()>;

    fn replace_folder(&mut self, folder: Folder) -> Result<()>;

    /// Moves a folder before a destination sibling, or appends when `before` is
    /// absent. Must reject cycles and invalid anchors without changing the store.
    fn move_folder(
        &mut self,
        id: FolderId,
        destination: Option<FolderId>,
        before: Option<FolderId>,
    ) -> Result<()>;

    fn remove_secret(&mut self, id: SecretId) -> Result<()>;

    /// Removes the folder and atomically reparents its children and secrets to
    /// its former parent (or to the root when it had no parent). Reparented
    /// secrets must receive a new revision so open edits become stale.
    fn remove_folder(&mut self, id: FolderId) -> Result<()>;

    fn move_secret(&mut self, id: SecretId, destination: Option<FolderId>) -> Result<()>;

    /// Must atomically replace ciphertext and recipient metadata only when the
    /// stored revision matches `expected_revision`, then increment the revision.
    fn replace_encryption(
        &mut self,
        id: SecretId,
        ciphertext: Vec<u8>,
        recipients: RecipientSet,
        expected_revision: u64,
    ) -> Result<()>;
}

/// OpenPGP capability boundary for recipient validation, encryption and decryption.
pub trait OpenPgp {
    fn validate_recipients(&self, recipients: &RecipientSet) -> Result<()>;

    fn encrypt(&self, plaintext: &[u8], recipients: &RecipientSet) -> Result<Vec<u8>>;

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>>;
}

/// Adds one encrypted batch atomically without replacing existing records.
/// Must reject ID collisions, missing parents, and cycles; preserve sibling
/// order by appending new folders. Any failure leaves the existing store intact.
pub trait ImportStorage: Storage {
    fn append_import(&mut self, folders: &[Folder], secrets: &[Secret]) -> Result<()>;
}
