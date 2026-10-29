//! Feeding the private search index from the vault.
//!
//! The list's public metadata (titles) goes in as is; private values are
//! loaded in the background, only for fields a template marks
//! `search="index"` and that are not concealed, and only with a passphrase
//! gpg-agent already caches.

use std::path::PathBuf;
use std::sync::Arc;

use kp_template::{Field, SearchMode, Template};
use kryptopass_core::SecretId;
use kryptopass_core::vault::{ItemMeta, TemplateRegistry, VaultError};
use kryptopass_keyring::Keyring;
use kryptopass_search::{CatalogRecord, Projection, RecordKey, SearchError, SearchSource};

use crate::session::{AppVault, background_vault};

/// Largest projection of one item, the index's per-field limit.
const MAX_PROJECTION_BYTES: usize = 64 * 1024;

/// The index catalog for the listed items: titles as public text, and which
/// items have private values worth loading.
pub fn catalog_records(items: &[ItemMeta], templates: &TemplateRegistry) -> Vec<CatalogRecord> {
    items
        .iter()
        .map(|meta| CatalogRecord {
            key: RecordKey {
                id: *meta.id.0.as_bytes(),
                revision: meta.revision,
            },
            public_fields: vec![meta.state.title.clone()],
            private_eligible: !meta.state.trashed
                && templates
                    .template(meta.state.template.as_str())
                    .is_ok_and(|template| indexed_fields(template).next().is_some()),
        })
        .collect()
}

fn indexed_fields(template: &Template) -> impl Iterator<Item = &Field> {
    template.fields().filter(|field| {
        field.search == SearchMode::Index && !field.field_type.is_concealed() && !field.conceal
    })
}

/// Loads projections for the search worker, on its own background
/// connection opened at first use.
pub struct VaultSearchSource {
    path: PathBuf,
    keyring: Keyring,
    templates: Arc<TemplateRegistry>,
    vault: Option<AppVault>,
}

impl VaultSearchSource {
    pub fn new(path: PathBuf, keyring: Keyring, templates: Arc<TemplateRegistry>) -> Self {
        Self {
            path,
            keyring,
            templates,
            vault: None,
        }
    }

    fn vault(&mut self) -> Result<&AppVault, SearchError> {
        if self.vault.is_none() {
            let vault = background_vault(&self.path, &self.keyring, Arc::clone(&self.templates))
                .map_err(|_| SearchError::Unavailable)?;
            self.vault = Some(vault);
        }
        self.vault.as_ref().ok_or(SearchError::Unavailable)
    }
}

impl SearchSource for VaultSearchSource {
    fn load(&mut self, record: RecordKey) -> Result<Projection, SearchError> {
        let templates = Arc::clone(&self.templates);
        let item = self
            .vault()?
            .open_item(SecretId(uuid::Uuid::from_bytes(record.id)))
            .map_err(|error| match error {
                VaultError::ItemNotFound => SearchError::StaleRevision,
                _ => SearchError::Unavailable,
            })?;
        if item.meta.revision != record.revision || item.meta.state.trashed {
            return Err(SearchError::StaleRevision);
        }
        let template = templates
            .template(item.meta.state.template.as_str())
            .map_err(|_| SearchError::Unavailable)?;
        let texts: Vec<&str> = indexed_fields(template)
            .filter_map(|field| item.content.value(field.id.as_str()))
            .flat_map(|value| value.texts())
            .map(|text| text.as_str())
            .filter(|text| !text.is_empty())
            .collect();
        // Streamed straight into protected memory, values separated by spaces.
        let characters = texts
            .iter()
            .enumerate()
            .flat_map(|(index, text)| (index > 0).then_some(' ').into_iter().chain(text.chars()))
            .map(Ok::<char, SearchError>);
        Projection::from_characters(characters, MAX_PROJECTION_BYTES)
    }
}
