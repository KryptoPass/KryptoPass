//! Sections group fields and may accept user-added ones.

use super::field::Field;
use super::id::SectionId;

/// Limit on fields the person may add to a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomFields {
    /// Maximum number of user-added fields in the section.
    pub max: u16,
}

/// A titled group of fields, equivalent to an HTML `fieldset`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Identifier, also the id of the section title in the `Section` scope of the catalog.
    pub id: SectionId,
    /// Fields in display order.
    pub fields: Vec<Field>,
    /// Present when the person may add their own fields to this section.
    pub custom_fields: Option<CustomFields>,
}
