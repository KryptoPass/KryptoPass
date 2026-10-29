//! The payload types and the API to read and change them.

use super::json_text;
use super::value::FieldValue;
use crate::SensitiveText;

use kp_template::{FieldId, SectionId, Template, TemplateId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Envelope version of item payloads.
pub const ITEM_SCHEMA_VERSION: u32 = 1;

/// Plaintext of one item: the template that shapes it, the values of its
/// fields and the fields the person added.
///
/// The template and version are fixed at creation; only migration changes
/// the version. Values are reached by field id, and ids come from a template
/// or from [`FieldId::parse`], so a value is never stored under a malformed id.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ItemContent {
    #[zeroize(skip)]
    pub(super) template: TemplateId,
    pub(super) template_version: u32,
    pub(super) values: Vec<ItemValue>,
    pub(super) custom: Vec<CustomValue>,
}

/// Value stored under one template field id.
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct ItemValue {
    #[zeroize(skip)]
    pub(super) id: FieldId,
    pub(super) value: FieldValue,
}

/// A field the person added, kept in a section whose template allows it.
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomValue {
    #[zeroize(skip)]
    pub(super) id: Uuid,
    #[zeroize(skip)]
    pub section: SectionId,
    #[serde(deserialize_with = "json_text::deserialize")]
    pub key: SensitiveText,
    #[serde(deserialize_with = "json_text::deserialize")]
    pub value: SensitiveText,
    pub multiline: bool,
    pub concealed: bool,
}

impl ItemContent {
    /// Empty content for `template`, at its current version.
    pub fn new(template: &Template) -> Self {
        Self {
            template: template.id.clone(),
            template_version: template.version,
            values: Vec::new(),
            custom: Vec::new(),
        }
    }

    pub fn template(&self) -> &TemplateId {
        &self.template
    }

    /// Version of the template this content follows.
    pub fn template_version(&self) -> u32 {
        self.template_version
    }

    /// Values in stored order.
    pub fn values(&self) -> &[ItemValue] {
        &self.values
    }

    /// The value stored under `field`, if any.
    pub fn value(&self, field: &str) -> Option<&FieldValue> {
        self.position(field).map(|index| &self.values[index].value)
    }

    pub fn value_mut(&mut self, field: &str) -> Option<&mut FieldValue> {
        self.position(field)
            .map(|index| &mut self.values[index].value)
    }

    /// Stores `value` under `field`, replacing the old value, which is
    /// zeroized when dropped.
    pub fn set(&mut self, field: FieldId, value: FieldValue) {
        match self.position(field.as_str()) {
            Some(index) => self.values[index].value = value,
            None => self.values.push(ItemValue { id: field, value }),
        }
    }

    /// Removes the value of `field`; returns whether there was one.
    pub fn remove(&mut self, field: &str) -> bool {
        self.position(field)
            .map(|index| self.values.remove(index))
            .is_some()
    }

    /// Added fields in display order.
    pub fn custom(&self) -> &[CustomValue] {
        &self.custom
    }

    pub fn custom_mut(&mut self, id: Uuid) -> Option<&mut CustomValue> {
        self.custom.iter_mut().find(|custom| custom.id == id)
    }

    pub fn add_custom(&mut self, custom: CustomValue) {
        self.custom.push(custom);
    }

    /// Removes an added field; returns whether it existed.
    pub fn remove_custom(&mut self, id: Uuid) -> bool {
        let before = self.custom.len();
        self.custom.retain(|custom| custom.id != id);
        self.custom.len() != before
    }

    fn position(&self, field: &str) -> Option<usize> {
        self.values
            .iter()
            .position(|entry| entry.id.as_str() == field)
    }
}

impl ItemValue {
    pub fn id(&self) -> &FieldId {
        &self.id
    }

    pub fn value(&self) -> &FieldValue {
        &self.value
    }
}

impl CustomValue {
    /// A new added field with a fresh id, shown and single-line.
    pub fn new(section: SectionId, key: SensitiveText, value: SensitiveText) -> Self {
        Self {
            id: Uuid::new_v4(),
            section,
            key,
            value,
            multiline: false,
            concealed: false,
        }
    }

    /// Stable id, so the editor can address the field across reorders.
    pub fn id(&self) -> Uuid {
        self.id
    }
}
