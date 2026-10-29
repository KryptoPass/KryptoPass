//! Failures when assembling a library of templates, catalogs and generators.

use crate::parse::ParseError;
use std::fmt;

/// Reason a set of templates, catalogs and generators was refused.
///
/// Fields hold ids and names only, never stored values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryError {
    /// An embedded file failed to parse. This is a bug in the shipped data.
    Embedded {
        /// Name of the embedded file, without extension.
        file: &'static str,
        /// What the parser reported.
        source: ParseError,
    },
    /// Two templates share an id.
    DuplicateTemplate(String),
    /// Two catalogs are for the same language.
    DuplicateLanguage(String),
    /// Two generators share an id.
    DuplicateGenerator(String),
    /// A template refers to a message a catalog does not have.
    MissingMessage {
        /// Language of the catalog with the gap.
        language: String,
        /// Template that refers to the message.
        template: String,
        /// Catalog group of the message.
        scope: &'static str,
        /// Id of the message.
        id: String,
    },
    /// A field uses a generator that is not declared.
    UnknownGenerator {
        /// Template holding the field.
        template: String,
        /// Field using the generator.
        field: String,
        /// Generator that does not exist.
        generator: String,
    },
    /// A field overrides a parameter the generator does not have.
    UnknownGeneratorParam {
        /// Template holding the field.
        template: String,
        /// Field with the override.
        field: String,
        /// Parameter that does not exist.
        param: String,
    },
    /// A field overrides a parameter with a value outside its range.
    GeneratorValueOutOfRange {
        /// Template holding the field.
        template: String,
        /// Field with the override.
        field: String,
        /// Parameter set out of range.
        param: String,
    },
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Embedded { file, source } => write!(f, "embedded `{file}`: {source}"),
            Self::DuplicateTemplate(id) => write!(f, "duplicate template `{id}`"),
            Self::DuplicateLanguage(tag) => write!(f, "duplicate catalog for `{tag}`"),
            Self::DuplicateGenerator(id) => write!(f, "duplicate generator `{id}`"),
            Self::MissingMessage {
                language,
                template,
                scope,
                id,
            } => {
                write!(
                    f,
                    "catalog `{language}` lacks {scope}/{id} used by `{template}`"
                )
            }
            Self::UnknownGenerator {
                template,
                field,
                generator,
            } => {
                write!(
                    f,
                    "`{template}.{field}` uses unknown generator `{generator}`"
                )
            }
            Self::UnknownGeneratorParam {
                template,
                field,
                param,
            } => {
                write!(
                    f,
                    "`{template}.{field}` overrides unknown parameter `{param}`"
                )
            }
            Self::GeneratorValueOutOfRange {
                template,
                field,
                param,
            } => {
                write!(f, "`{template}.{field}` sets `{param}` out of range")
            }
        }
    }
}

impl std::error::Error for LibraryError {}
