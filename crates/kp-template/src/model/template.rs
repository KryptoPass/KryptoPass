//! The template root and its metadata.

use super::action::Action;
use super::behavior::Behavior;
use super::check::Check;
use super::field::Field;
use super::id::{FieldId, IconName, TemplateId};
use super::migration::Migration;
use super::section::Section;
use super::text::Text;
use super::vocabulary::vocabulary;

vocabulary! {
    /// Where a template comes from, which decides how strictly it is treated.
    pub enum TemplateKind {
        /// Shipped inside the application; labels are message keys.
        Builtin => "builtin",
        /// Defined by the user; untrusted input with literal labels.
        User => "user",
    }
}

/// Prefix of every user template id, so it can never shadow a built-in one.
pub(crate) const USER_ID_PREFIX: &str = "user.";

/// Name of the item's own title, which [`Meta::search_title`] may refer to even
/// though it is not one of the template fields.
pub(crate) const ITEM_TITLE: &str = "title";

/// Presentation data that is not a field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    /// Name of the item type, shown when choosing a template.
    pub name: Text,
    /// Icon shown next to the name and in lists.
    pub icon: IconName,
    /// Field whose value titles the item in search results. `title` refers to
    /// the item's own title, which is not one of the template fields.
    pub search_title: FieldId,
    /// Field shown under the title in lists, such as the username of a login.
    pub summary: Option<FieldId>,
}

/// A complete item type: how it looks and which fields it stores.
///
/// The serialized item records the template id and version together with the
/// values keyed by field id, so the template can be changed without rewriting
/// stored data as long as ids stay stable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    /// Stable identifier, recorded in every item created from this template.
    pub id: TemplateId,
    /// Bumped only for incompatible changes. Adding an optional field keeps it;
    /// removing a field or changing its type does not.
    pub version: u32,
    /// Origin of the template.
    pub kind: TemplateKind,
    /// Name, icon and list presentation.
    pub meta: Meta,
    /// Fields grouped in display order.
    pub sections: Vec<Section>,
    /// Actions offered on items of this type.
    pub actions: Vec<Action>,
    /// Rules across fields, enforced when an item is validated.
    pub checks: Vec<Check>,
    /// History and tracking limits; absent when the template keeps no history.
    pub behavior: Option<Behavior>,
    /// Upgrades from older versions, by `from` version ascending.
    pub migrations: Vec<Migration>,
}

impl Template {
    /// Every field, section by section, in display order.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.sections.iter().flat_map(|section| &section.fields)
    }

    /// The field with this id.
    pub fn field(&self, id: &str) -> Option<&Field> {
        self.fields().find(|field| field.id.as_str() == id)
    }

    /// The migration from `version` to the next one.
    pub fn migration_from(&self, version: u32) -> Option<&Migration> {
        self.migrations
            .iter()
            .find(|migration| migration.from == version)
    }

    /// The section with this id.
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|section| section.id.as_str() == id)
    }
}
