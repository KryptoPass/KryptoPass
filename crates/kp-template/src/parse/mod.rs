//! Reading template, catalog and generators documents into the model.
//!
//! Documents are untrusted input: DTDs are refused, size and node counts are
//! bounded, and every identifier and closed value is checked here, so the model
//! types can be trusted by the code that uses them. Failures never include a
//! stored value; they name an element, an attribute and a line.

mod catalog;
mod error;
mod generator;
mod ids;
mod reader;
mod template;
mod vocab;

pub use error::{ParseError, ParseErrorKind};

use crate::model::{Catalog, GeneratorDef, Template};

/// Reads one template document.
///
/// Checks that ids are unique, that `summary`, `search-title` and action fields
/// exist, and that type-specific attributes sit on the right field type.
/// Generator and message references are not resolved here, because they live
/// in other documents.
pub fn parse_template(xml: &str) -> Result<Template, ParseError> {
    template::parse(xml)
}

/// Reads one message catalog document.
pub fn parse_catalog(xml: &str) -> Result<Catalog, ParseError> {
    catalog::parse(xml)
}

/// Reads the generators document.
pub fn parse_generators(xml: &str) -> Result<Vec<GeneratorDef>, ParseError> {
    generator::parse(xml)
}
