//! Identifiers as plain strings in serialized data, validated when read.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

use crate::model::{FieldId, SectionId, TemplateId};

macro_rules! string_serde {
    ($($name:ident),+ $(,)?) => {$(
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                Self::parse(&raw).map_err(|_| de::Error::custom("invalid identifier"))
            }
        }
    )+};
}

string_serde!(TemplateId, FieldId, SectionId);
