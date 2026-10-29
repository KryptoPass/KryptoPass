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

use crate::model::{Catalog, GeneratorDef, MessageId, MessageRef, MessageScope, Template, Text};

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

    /// The text to show for `text` in `language`.
    ///
    /// Falls back to the base language (`pt` for `pt-BR`), then English, then
    /// any language, so a gap shows other text instead of nothing. `None` only
    /// when no language has it.
    pub fn text<'a>(&'a self, text: &'a Text, language: &str) -> Option<&'a str> {
        match text {
            Text::Key(key) => self.message(key, language),
            Text::Literal(literals) => fallback_order(language)
                .find_map(|wanted| {
                    literals
                        .iter()
                        .find(|literal| wanted.is_none_or(|tag| literal.language == tag))
                })
                .map(|literal| literal.text.as_str()),
        }
    }

    /// The translation of `key` in `language`, with the same fallbacks.
    pub fn message(&self, key: &MessageRef, language: &str) -> Option<&str> {
        fallback_order(language)
            .flat_map(|wanted| {
                self.catalogs.iter().filter(move |catalog| {
                    wanted.is_none_or(|tag| catalog.language.as_str() == tag)
                })
            })
            .find_map(|catalog| catalog.messages.get(key))
            .map(String::as_str)
    }

    /// The title of a section; its id when no catalog has one, as for user
    /// templates.
    pub fn section_title<'a>(&'a self, section: &'a str, language: &str) -> &'a str {
        self.scoped(MessageScope::Section, section, language)
    }

    /// The label of a `choice` option; the option id itself when untranslated,
    /// as for product names.
    pub fn option_label<'a>(&'a self, option: &'a str, language: &str) -> &'a str {
        self.scoped(MessageScope::Option, option, language)
    }

    fn scoped<'a>(&'a self, scope: MessageScope, id: &'a str, language: &str) -> &'a str {
        let key = MessageRef {
            scope,
            id: MessageId::from_validated(id.to_owned()),
        };
        self.message(&key, language).unwrap_or(id)
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

/// Languages to try for `language`: itself, its base, English, then any
/// (`None`).
fn fallback_order(language: &str) -> impl Iterator<Item = Option<&str>> {
    let base = language.split_once('-').map(|(base, _)| base);
    [Some(language), base, Some("en"), None]
        .into_iter()
        .enumerate()
        .filter_map(move |(index, tag)| match (index, tag) {
            (1 | 2, None) => None,
            (_, tag) => Some(tag),
        })
}
