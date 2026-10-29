//! The templates, catalogs and generators that ship inside the binary.
//!
//! They are embedded with `include_str!`, so the binary does not depend on files
//! next to it at run time. Rust cannot embed a whole directory without another
//! dependency, so the names are listed here; a test compares these lists with
//! the directories so a file added to disk but not to the list fails the build.

use super::error::LibraryError;
use crate::model::{Catalog, GeneratorDef, Template};
use crate::parse::{parse_catalog, parse_generators, parse_template};

/// Embeds `templates/<dir>/<name>.xml` for each name, paired with the name.
macro_rules! embed {
    ($dir:literal: $($name:literal),+ $(,)?) => {
        &[$(($name, include_str!(concat!("../../templates/", $dir, "/", $name, ".xml")))),+]
    };
}

pub(super) const TEMPLATES: &[(&str, &str)] = embed!("builtin":
    "api-token", "bank-account", "card", "crypto-wallet", "custom", "database",
    "drivers-license", "email-account", "identity", "license", "login", "membership",
    "note", "passport", "password", "server", "ssh-key", "totp", "wifi",
);

pub(super) const CATALOGS: &[(&str, &str)] = embed!("i18n": "es", "en", "pt-BR", "fr");

const GENERATORS: (&str, &str) = ("generators", include_str!("../../templates/generators.xml"));

pub(super) struct Parsed {
    pub(super) templates: Vec<Template>,
    pub(super) catalogs: Vec<Catalog>,
    pub(super) generators: Vec<GeneratorDef>,
}

pub(super) fn parse_all() -> Result<Parsed, LibraryError> {
    let embedded = |file| move |source| LibraryError::Embedded { file, source };
    let templates = TEMPLATES
        .iter()
        .map(|(name, xml)| parse_template(xml).map_err(embedded(name)))
        .collect::<Result<_, _>>()?;
    let catalogs = CATALOGS
        .iter()
        .map(|(name, xml)| parse_catalog(xml).map_err(embedded(name)))
        .collect::<Result<_, _>>()?;
    let generators = parse_generators(GENERATORS.1).map_err(embedded(GENERATORS.0))?;
    Ok(Parsed {
        templates,
        catalogs,
        generators,
    })
}
