//! How content written by an older template version becomes current.
//!
//! A template's `version` is bumped only for incompatible changes. Each bump
//! comes with one [`Migration`] that turns content of the previous version
//! into content of the next one; content several versions behind goes through
//! each step in order.
//!
//! Steps never delete data. A field that leaves the template is retired into a
//! user-added field, so whatever a person stored survives any template change.

use super::id::{FieldId, SectionId};

/// The steps from version `from` to `from + 1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migration {
    /// Version this migration upgrades from.
    pub from: u32,
    /// Applied in order.
    pub steps: Vec<MigrationStep>,
}

/// One change to stored content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationStep {
    /// The value moves to a new field id, unchanged.
    Rename {
        /// Id in the older version.
        from: FieldId,
        /// Id in the newer version.
        to: FieldId,
    },
    /// A single value becomes the first entry of a list.
    ToList {
        /// Field that became a list.
        field: FieldId,
    },
    /// The field leaves the template; each of its values becomes a user-added
    /// field in `section`, keyed by the old field id.
    Retire {
        /// Field that no longer exists.
        field: FieldId,
        /// Section that receives the values; it must accept custom fields.
        section: SectionId,
        /// Whether the values are shown hidden. The older field type is gone,
        /// so the template says it; it defaults to hidden.
        concealed: bool,
        /// Whether the values keep line breaks.
        multiline: bool,
    },
}
