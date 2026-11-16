//! Host composition of the importers and conversion into the current vault.

pub mod catalog;
pub(crate) mod presentation;

pub use kryptopass_app::PreparedImport;

use std::path::Path;

use anyhow::Result;
use kryptopass_core::RecipientSet;
use kryptopass_core::vault::{ImportBatch, ImportReceipt, Recipients};
use kryptopass_import::ImporterRegistry;
use kryptopass_keyring::Keyring;

pub fn registry() -> Result<ImporterRegistry> {
    Ok(kryptopass_app::importers(&Keyring::system())?)
}

pub fn review_export(
    path: &Path,
    importer_id: &str,
    domain_folders: bool,
) -> Result<PreparedImport> {
    let registry = registry()?;
    let entries = kryptopass_app::read_export(&registry, importer_id, path)?;
    let templates = crate::vault_access::templates()?;
    Ok(kryptopass_app::prepare_import_with_options(
        entries,
        templates,
        &crate::vault_access::language(),
        kryptopass_app::ImportOptions { domain_folders },
    )?)
}

/// Adds the reviewed batch to the active vault, at its root.
pub fn receive_import(batch: ImportBatch, recipients: RecipientSet) -> Result<ImportReceipt> {
    let mut session = crate::vault_access::open_session()?;
    Ok(session
        .vault_mut()
        .import(batch, Recipients::Explicit(recipients), None)?)
}
