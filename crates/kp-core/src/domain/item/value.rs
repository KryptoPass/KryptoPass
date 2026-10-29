//! The value of one template field.

use super::json_text::RawText;
use crate::SensitiveText;
use crate::util::unescape_json_string;

use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// One text value or an ordered list, matching the field's multiplicity.
///
/// Serialized as a JSON string or array. It is read from the raw JSON text
/// (see `json_text`) so no decoded copy is left in a parser buffer.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub enum FieldValue {
    One(SensitiveText),
    Many(Vec<SensitiveText>),
}

impl FieldValue {
    /// The texts held, one for a single value.
    pub fn texts(&self) -> &[SensitiveText] {
        match self {
            Self::One(text) => std::slice::from_ref(text),
            Self::Many(list) => list,
        }
    }

    /// The first text, which is the primary one of a list.
    pub fn first(&self) -> Option<&SensitiveText> {
        self.texts().first()
    }

    /// Whether any text is non-empty.
    pub fn has_content(&self) -> bool {
        match self {
            Self::One(text) => !text.is_empty(),
            Self::Many(list) => list.iter().any(|text| !text.is_empty()),
        }
    }
}

impl Serialize for FieldValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::One(text) => text.serialize(serializer),
            Self::Many(list) => list.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for FieldValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let unescape = |raw| unescape_json_string(raw).map_err(de::Error::custom);
        match RawText::read(deserializer)? {
            RawText::One(raw) => unescape(raw).map(Self::One),
            RawText::Many(list) => list
                .into_iter()
                .map(unescape)
                .collect::<Result<_, _>>()
                .map(Self::Many),
        }
    }
}
