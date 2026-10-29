//! A validated set of templates, catalogs and generators.
//!
//! Parsing reads one document at a time; a [`Library`] is where documents are
//! checked against each other, so code holding one can rely on every reference
//! resolving.

mod builtin;
mod error;
mod validate;

pub use error::LibraryError;
pub use validate::referenced_messages;

use crate::model::{Catalog, GeneratorDef, Template};

/// Templates together with the catalogs and generators they refer to.
///
/// Built only through [`Library::new`] or [`Library::builtin`], both of which
/// run the cross-document checks, so a `Library` is always consistent.
#[derive(Clone, Debug)]
pub struct Library {
    templates: Vec<Template>,
    catalogs: Vec<Catalog>,
    generators: Vec<GeneratorDef>,
}

impl Library {
    /// Checks the parts against each other and keeps them.
    ///
    /// Refuses duplicate ids, a template that uses an undeclared generator or a
    /// parameter out of range, and any catalog that lacks a message a template
    /// refers to. Gaps are refused on purpose: a library is the shipped set, and
    /// an incomplete translation should fail at build time, not show a blank label.
    pub fn new(
        templates: Vec<Template>,
        catalogs: Vec<Catalog>,
        generators: Vec<GeneratorDef>,
    ) -> Result<Self, LibraryError> {
        validate::check(&templates, &catalogs, &generators)?;
        Ok(Self {
            templates,
            catalogs,
            generators,
        })
    }

    /// The templates, catalogs and generators embedded in the binary.
    ///
    /// Callers should build it once and keep it: each call parses every
    /// embedded document again.
    pub fn builtin() -> Result<Self, LibraryError> {
        let parsed = builtin::parse_all()?;
        Self::new(parsed.templates, parsed.catalogs, parsed.generators)
    }

    /// All templates, in the order they were provided.
    pub fn templates(&self) -> &[Template] {
        &self.templates
    }

    /// The template with this id.
    pub fn template(&self, id: &str) -> Option<&Template> {
        self.templates
            .iter()
            .find(|template| template.id.as_str() == id)
    }

    /// All catalogs, one per language.
    pub fn catalogs(&self) -> &[Catalog] {
        &self.catalogs
    }

    /// The catalog for exactly this language tag, without any fallback.
    pub fn catalog(&self, language: &str) -> Option<&Catalog> {
        self.catalogs
            .iter()
            .find(|catalog| catalog.language.as_str() == language)
    }

    /// All generators.
    pub fn generators(&self) -> &[GeneratorDef] {
        &self.generators
    }

    /// The generator with this id.
    pub fn generator(&self, id: &str) -> Option<&GeneratorDef> {
        self.generators
            .iter()
            .find(|generator| generator.id.as_str() == id)
    }
}
