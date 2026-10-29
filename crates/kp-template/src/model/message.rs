//! References from a template to translated text.

use super::id::MessageId;
use super::vocabulary::vocabulary;

vocabulary! {
    /// What a message translates. The name is the catalog group that holds it, so
    /// no prefix has to be parsed out of an id.
    #[derive(PartialOrd, Ord)]
    pub enum MessageScope {
        /// Name of a template, looked up by the template id.
        Template => "templates",
        /// Title of a section, looked up by the section id.
        Section => "sections",
        /// Label of a field.
        Label => "labels",
        /// Display name of one option of a field.
        Option => "options",
        /// Short help text shown with a field.
        Hint => "hints",
    }
}

/// A message in a catalog: the scope it belongs to and its id inside it.
///
/// A reference missing from a catalog is a gap in the translation, not a data
/// error in the template. A [`Library`](crate::Library) refuses to be built with
/// such gaps, so the shipped catalogs are always complete.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageRef {
    /// Group of the catalog the message is in.
    pub scope: MessageScope,
    /// Id of the message inside that group.
    pub id: MessageId,
}
