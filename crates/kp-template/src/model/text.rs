//! Display text: catalog keys or literals.

use super::message::MessageRef;

/// A literal label in one language, used by user templates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalizedLiteral {
    /// Language the text is written in, as given by `xml:lang`.
    pub language: String,
    /// The text to show.
    pub text: String,
}

/// Text shown to the person: a label, a section name or a template name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Text {
    /// Resolved through the active [`Catalog`](crate::Catalog). Built-in
    /// templates use this so their text is translated.
    Key(MessageRef),
    /// Written directly in the template, one entry per language. User templates
    /// use this because they ship without a catalog.
    Literal(Vec<LocalizedLiteral>),
}
