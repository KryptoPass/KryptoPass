//! History and event tracking limits.

use super::vocabulary::vocabulary;

vocabulary! {
    /// Something that happens to an item and may be recorded in its history.
    pub enum EventKind {
        /// The item was created.
        Create => "create",
        /// The item was used, for example a login filled in a site.
        Use => "use",
        /// A secret was replaced by a new value.
        Rotate => "rotate",
        /// A field was edited.
        Edit => "edit",
        /// A field value was copied.
        Copy => "copy",
    }
}

/// How much history an item keeps. Recorded events carry no field values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Behavior {
    /// Maximum retained versions of fields marked with `history`.
    pub max_versions: Option<u8>,
    /// Kinds of event recorded in the item history.
    pub tracked_events: Vec<EventKind>,
}
