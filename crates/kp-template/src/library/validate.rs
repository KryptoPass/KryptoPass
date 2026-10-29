//! Checks that span documents: a template against catalogs and generators.
//!
//! The parser sees one document at a time, so these rules live here.

use super::error::LibraryError;
use crate::model::{
    Catalog, GeneratorDef, MessageId, MessageRef, MessageScope, Template, TemplateKind, Text,
};
use std::collections::BTreeSet;

/// Every catalog message a template needs in order to be shown.
///
/// User templates carry literal text, so they need none. Options of a `select`
/// are not included: which option values get translated is a convention of the
/// template's own implementation, not part of the format.
pub fn referenced_messages(template: &Template) -> BTreeSet<MessageRef> {
    let mut refs = BTreeSet::new();
    if let Text::Key(name) = &template.meta.name {
        refs.insert(name.clone());
    }
    for section in &template.sections {
        if template.kind == TemplateKind::Builtin {
            refs.insert(MessageRef {
                scope: MessageScope::Section,
                id: MessageId::from_validated(section.id.as_str().to_owned()),
            });
        }
        for field in &section.fields {
            if let Text::Key(label) = &field.label {
                refs.insert(label.clone());
            }
        }
    }
    refs
}

pub(super) fn check(
    templates: &[Template],
    catalogs: &[Catalog],
    generators: &[GeneratorDef],
) -> Result<(), LibraryError> {
    unique(
        templates.iter().map(|t| t.id.as_str()),
        LibraryError::DuplicateTemplate,
    )?;
    unique(
        catalogs.iter().map(|c| c.language.as_str()),
        LibraryError::DuplicateLanguage,
    )?;
    unique(
        generators.iter().map(|g| g.id.as_str()),
        LibraryError::DuplicateGenerator,
    )?;
    for template in templates {
        check_messages(template, catalogs)?;
        check_generators(template, generators)?;
    }
    Ok(())
}

fn unique<'a>(
    ids: impl Iterator<Item = &'a str>,
    error: fn(String) -> LibraryError,
) -> Result<(), LibraryError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(error(id.to_owned()));
        }
    }
    Ok(())
}

/// A library is the shipped set, so it has no translation gaps: every catalog
/// must hold every message every template refers to.
fn check_messages(template: &Template, catalogs: &[Catalog]) -> Result<(), LibraryError> {
    let needed = referenced_messages(template);
    for catalog in catalogs {
        if let Some(missing) = needed.iter().find(|m| !catalog.messages.contains_key(*m)) {
            return Err(LibraryError::MissingMessage {
                language: catalog.language.as_str().to_owned(),
                template: template.id.as_str().to_owned(),
                scope: missing.scope.name(),
                id: missing.id.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

fn check_generators(template: &Template, generators: &[GeneratorDef]) -> Result<(), LibraryError> {
    for field in template.sections.iter().flat_map(|s| &s.fields) {
        let Some(reference) = &field.generator else {
            continue;
        };
        let context = |generator: &str| {
            (
                template.id.as_str().to_owned(),
                field.id.as_str().to_owned(),
                generator.to_owned(),
            )
        };
        let Some(def) = generators.iter().find(|g| g.id == reference.id) else {
            let (template, field, generator) = context(reference.id.as_str());
            return Err(LibraryError::UnknownGenerator {
                template,
                field,
                generator,
            });
        };
        for over in &reference.overrides {
            let (template, field, param) = context(&over.name);
            let Some(known) = def.params.iter().find(|p| p.name == over.name) else {
                return Err(LibraryError::UnknownGeneratorParam {
                    template,
                    field,
                    param,
                });
            };
            if !(known.min..=known.max).contains(&over.value) {
                return Err(LibraryError::GeneratorValueOutOfRange {
                    template,
                    field,
                    param,
                });
            }
        }
    }
    Ok(())
}
