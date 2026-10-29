//! Typed model of item templates.
//!
//! A template describes how an item is rendered and which fields it stores. It
//! carries ids and message references only: translated text lives in a [`Catalog`] and
//! stored values are keyed by [`FieldId`], so changing a label or a language
//! never touches stored data.
//!
//! The `model` module holds the types and the `parse` module reads XML into
//! them. Parsing is the only way identifiers are built, which is why identifier
//! types have no public constructor.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod library;
mod model;
mod parse;
#[cfg(feature = "serde")]
mod serde_ids;

pub use library::{Library, LibraryError, referenced_messages};
pub use model::*;
pub use parse::{ParseError, ParseErrorKind, parse_catalog, parse_generators, parse_template};
