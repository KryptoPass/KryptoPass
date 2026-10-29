//! The templates items can be created from, loaded once.

use kp_template::{Library, LibraryError, Template};

use super::error::VaultError;
use crate::domain::item::ItemContent;

/// Templates by id. Share it behind an `Arc`: the UI, search and the vault
/// read the same instance.
#[derive(Debug)]
pub struct TemplateRegistry {
    library: Library,
}

impl TemplateRegistry {
    pub fn new(library: Library) -> Self {
        Self { library }
    }

    /// The templates embedded in the binary.
    pub fn builtin() -> Result<Self, LibraryError> {
        Library::builtin().map(Self::new)
    }

    /// Templates, catalogs and generators, for presentation.
    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn template(&self, id: &str) -> Result<&Template, VaultError> {
        self.library.template(id).ok_or(VaultError::UnknownTemplate)
    }

    /// The template of content about to be written, which must be at the
    /// template's current version: older content is migrated when read.
    pub(super) fn current(&self, content: &ItemContent) -> Result<&Template, VaultError> {
        let template = self.template(content.template().as_str())?;
        if content.template_version() == template.version {
            Ok(template)
        } else {
            Err(VaultError::TemplateVersion)
        }
    }
}
