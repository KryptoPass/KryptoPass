//! Vault adapter for temporary account previews and explicitly approved browser fills.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, ensure};
use kryptopass_app::Credentials;
use kryptopass_browser::{Candidate, CredentialProvider, Login};
use kryptopass_core::SecretId;
use kryptopass_secure_memory::SensitiveText;

use crate::storage::paths as app_paths;

struct VaultProvider(PathBuf);

pub fn configure(path: &std::path::Path) -> Result<()> {
    let directory = app_paths::browser_state_directory(crate::app_info::APP.desktop_file_id)?;
    kryptopass_browser::configure_vault(
        &directory,
        path,
        Arc::new(VaultProvider(path.canonicalize()?)),
    )
}

impl CredentialProvider for VaultProvider {
    fn candidates(&self) -> Result<Vec<Candidate>> {
        let session = crate::vault_access::open_existing_session(self.0.clone())?;
        Ok(kryptopass_app::candidates(session.vault())?
            .into_iter()
            .map(|entry| Candidate {
                id: entry.id.0.to_string(),
                name: entry.title,
                revision: entry.revision,
            })
            .collect())
    }

    fn login(&self, candidate: &Candidate, still_authorized: &dyn Fn() -> bool) -> Result<Login> {
        let credentials = self.read_current(candidate, still_authorized)?;
        Ok(Login {
            username: credentials.username.unwrap_or_default(),
            password: credentials.password.unwrap_or_default(),
            urls: credentials.urls,
        })
    }

    fn username(
        &self,
        candidate: &Candidate,
        still_current: &dyn Fn() -> bool,
    ) -> Result<Option<SensitiveText>> {
        let credentials = self.read_current(candidate, still_current)?;
        Ok(Some(credentials.username.unwrap_or_default()))
    }
}

impl VaultProvider {
    fn read_current(
        &self,
        candidate: &Candidate,
        still_authorized: &dyn Fn() -> bool,
    ) -> Result<Credentials> {
        ensure!(still_authorized(), "browser request cancelled");
        let id = SecretId(uuid::Uuid::parse_str(&candidate.id)?);
        let session = crate::vault_access::open_existing_session(self.0.clone())?;
        // `credentials` refuses a revision other than the one the browser was offered.
        let credentials = kryptopass_app::credentials(
            session.vault(),
            &kryptopass_app::Candidate {
                id,
                title: candidate.name.clone(),
                revision: candidate.revision,
            },
        )?;
        ensure!(still_authorized(), "browser request cancelled");
        Ok(credentials)
    }
}
