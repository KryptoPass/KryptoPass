//! Vault adapter for temporary account previews and explicitly approved browser fills.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, ensure};
use kryptopass_browser::{Candidate, CredentialProvider, Login};
use kryptopass_core::{SecretContent, SecretId, SecretKind, SecretsService};
use kryptopass_secure_memory::SensitiveText;

use super::{GnuPgOpenPgp, SqliteStorage, app_paths};

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
        let service = SecretsService::new(SqliteStorage::open_existing(&self.0)?, GnuPgOpenPgp);
        Ok(service
            .list_secret_metadata()?
            .into_iter()
            .filter(|entry| entry.kind == SecretKind::Login && !entry.trashed)
            .map(|entry| Candidate {
                id: entry.id.0.to_string(),
                name: entry.title,
                revision: entry.revision,
            })
            .collect())
    }

    fn login(&self, candidate: &Candidate, still_authorized: &dyn Fn() -> bool) -> Result<Login> {
        let mut content = self.read_current(candidate, still_authorized)?;
        let SecretContent::Login(login) = &mut content else {
            anyhow::bail!("not a login");
        };
        let mut urls = std::mem::take(&mut login.urls);
        if urls.is_empty() && !login.url.is_empty() {
            urls.push(std::mem::take(&mut login.url));
        }
        Ok(Login {
            username: std::mem::take(&mut login.username),
            password: std::mem::take(&mut login.password),
            urls,
        })
    }

    fn username(
        &self,
        candidate: &Candidate,
        still_current: &dyn Fn() -> bool,
    ) -> Result<Option<SensitiveText>> {
        let mut content = self.read_current(candidate, still_current)?;
        let SecretContent::Login(login) = &mut content else {
            anyhow::bail!("not a login");
        };
        Ok(Some(std::mem::take(&mut login.username)))
    }
}

impl VaultProvider {
    fn read_current(
        &self,
        candidate: &Candidate,
        still_authorized: &dyn Fn() -> bool,
    ) -> Result<SecretContent> {
        ensure!(still_authorized(), "browser request cancelled");
        let id = SecretId(uuid::Uuid::parse_str(&candidate.id)?);
        let service = SecretsService::new(SqliteStorage::open_existing(&self.0)?, GnuPgOpenPgp);
        let valid = || -> Result<()> {
            ensure!(still_authorized(), "browser request cancelled");
            ensure!(
                service
                    .list_secret_metadata()?
                    .iter()
                    .any(|entry| entry.id == id
                        && entry.revision == candidate.revision
                        && !entry.trashed
                        && entry.kind == SecretKind::Login),
                "browser entry changed"
            );
            Ok(())
        };
        valid()?;
        let content = service.read_content(id)?;
        valid()?;
        Ok(content)
    }
}
