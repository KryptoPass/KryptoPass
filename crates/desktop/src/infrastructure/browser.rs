//! Vault adapter for temporary account previews and explicitly approved browser fills.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, ensure};
use kryptopass_browser::{Candidate, CredentialProvider, Login};
use kryptopass_core::{SecretContent, SecretId, SecretKind, SecretsService};
use zeroize::Zeroizing;

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
        let content = self.read_current(candidate, still_authorized)?;
        let SecretContent::Login(login) = &content else {
            anyhow::bail!("not a login");
        };
        let mut urls: Vec<_> = login
            .urls
            .iter()
            .map(|url| Zeroizing::new(url.clone()))
            .collect();
        if urls.is_empty() && !login.url.is_empty() {
            urls.push(Zeroizing::new(login.url.clone()));
        }
        Ok(Login {
            username: Zeroizing::new(login.username.clone()),
            password: Zeroizing::new(login.password.clone()),
            urls,
        })
    }

    fn username(
        &self,
        candidate: &Candidate,
        still_current: &dyn Fn() -> bool,
    ) -> Result<Option<Zeroizing<String>>> {
        let content = self.read_current(candidate, still_current)?;
        let SecretContent::Login(login) = &content else {
            anyhow::bail!("not a login");
        };
        Ok(Some(Zeroizing::new(login.username.clone())))
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
