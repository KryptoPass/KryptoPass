//! Message catalogs: translations kept apart from templates.

use super::message::MessageRef;
use std::collections::BTreeMap;

/// BCP 47 language tag, for example `es` or `pt-BR`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LanguageTag(String);

impl LanguageTag {
    /// Wraps a tag the parser already validated.
    pub(crate) fn from_validated(value: String) -> Self {
        Self(value)
    }

    /// The tag as written in the catalog file.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The translations of every message for one language.
///
/// The catalog file groups messages by scope (`templates`, `sections`,
/// `labels`, `options`); here each message is keyed by its scope and id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    /// Language every message is written in.
    pub language: LanguageTag,
    /// Catalog format version.
    pub version: u32,
    /// Translated text by message reference.
    pub messages: BTreeMap<MessageRef, String>,
}
