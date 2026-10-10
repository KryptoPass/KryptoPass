//! Infrastructure capabilities required by the application service.

use anyhow::Result;
use zeroize::Zeroizing;

use crate::domain::{Folder, FolderId, RecipientSet, Secret, SecretId};

/// Persistence boundary for the local secret store.
pub trait Storage {
    fn find_folder(&self, id: FolderId) -> Result<Option<Folder>>;

    fn find_secret(&self, id: SecretId) -> Result<Option<Secret>>;

    fn insert_folder(&mut self, folder: Folder) -> Result<()>;

    fn insert_secret(&mut self, secret: Secret) -> Result<()>;

    fn move_secret(&mut self, id: SecretId, destination: Option<FolderId>) -> Result<()>;

    /// Must atomically replace ciphertext and its recipient metadata.
    fn replace_encryption(
        &mut self,
        id: SecretId,
        ciphertext: Vec<u8>,
        recipients: RecipientSet,
    ) -> Result<()>;
}

/// OpenPGP capability boundary. Implemented by the GnuPG adapter later.
pub trait OpenPgp {
    fn validate_recipients(&self, recipients: &RecipientSet) -> Result<()>;

    fn encrypt(&self, plaintext: &[u8], recipients: &RecipientSet) -> Result<Vec<u8>>;

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>>;
}
