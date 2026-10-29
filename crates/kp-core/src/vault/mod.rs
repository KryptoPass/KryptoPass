//! Vault: the use cases over items and folders, built on template-driven
//! content and two small ports, [`Store`] and [`Cipher`].
//!
//! Operations are grouped by file (`items`, `folders`, `sharing`, `settings`) as `impl`
//! blocks of [`Vault`]. Every public operation logs its outcome with ids,
//! codes and revisions only: never titles, values or key material.
//!
//! This module sits beside the older `service` and is not wired to the app
//! yet; it is meant to replace it.

mod cipher;
mod conflict;
mod error;
mod folders;
mod history;
mod icon;
mod item;
mod items;
mod record;
mod registry;
mod settings;
mod sharing;
mod store;

use std::sync::Arc;

use crate::domain::item::FormatRules;

pub use crate::domain::clock::Hlc;
pub use cipher::{Cipher, CipherError};
pub use conflict::Conflict;
pub use error::VaultError;
pub use folders::{FolderEdit, NewFolder};
pub use item::{IconChoice, Item, ItemEdit, NewItem, Recipients};
pub use record::{Author, ItemMeta, ItemRecord, ItemState, ItemVersion, VersionId, VersionInfo};
pub use registry::TemplateRegistry;
pub use settings::KeySettings;
pub use store::{Base, Change, FolderContents, LinkChange, Parents, PutItem, Store, StoreError};

/// An open vault: storage, the keys to read it and the templates it uses.
pub struct Vault<S, C> {
    store: S,
    cipher: C,
    templates: Arc<TemplateRegistry>,
    rules: Arc<FormatRules>,
}

impl<S: Store, C: Cipher> Vault<S, C> {
    pub fn new(store: S, cipher: C, templates: Arc<TemplateRegistry>) -> Self {
        Self {
            store,
            cipher,
            templates,
            rules: Arc::new(FormatRules::standard()),
        }
    }

    /// Validates content with `rules` instead of the standard ones.
    #[must_use]
    pub fn with_rules(mut self, rules: Arc<FormatRules>) -> Self {
        self.rules = rules;
        self
    }

    pub fn templates(&self) -> &Arc<TemplateRegistry> {
        &self.templates
    }

    /// Gives the ports back, for example to close the store on lock.
    pub fn into_parts(self) -> (S, C) {
        (self.store, self.cipher)
    }
}

/// Logs a failed operation: refusals at debug, faults at warn.
fn logged<T>(operation: &'static str, result: Result<T, VaultError>) -> Result<T, VaultError> {
    if let Err(error) = &result {
        if error.is_refusal() {
            tracing::debug!(operation, code = error.code(), "vault operation refused");
        } else {
            tracing::warn!(operation, code = error.code(), error = ?error, "vault operation failed");
        }
    }
    result
}

/// The revision after `base`.
fn next_revision(base: u64) -> Result<u64, VaultError> {
    base.checked_add(1).ok_or(VaultError::RevisionExhausted)
}
