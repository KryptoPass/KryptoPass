//! Reading JSON strings straight into protected memory.
//!
//! serde_json decodes a string with escapes (`\"`, `\n`, `é`…) into an
//! internal scratch buffer that is never zeroized, and a visitor cannot reach
//! it. Asking for the raw JSON text instead makes the parser only scan the
//! bytes, so here the escapes are decoded directly into a [`SensitiveText`]
//! and no plaintext copy is left behind.
//!
//! This works only when the input is borrowed, which is how
//! [`decode_item`](super::decode_item) reads. Any other reader fails instead of
//! falling back to the leaking path.

use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_json::value::RawValue;

use crate::SensitiveText;
use crate::util::unescape_json_string;

/// A JSON string or array of strings, before decoding.
pub(super) enum RawText<'de> {
    One(&'de str),
    Many(Vec<&'de str>),
}

impl<'de> RawText<'de> {
    pub(super) fn read<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <&'de RawValue>::deserialize(deserializer)?.get();
        match raw.as_bytes().first() {
            Some(b'"') => Ok(Self::One(raw)),
            Some(b'[') => {
                let items: Vec<&'de RawValue> =
                    serde_json::from_str(raw).map_err(|_| de::Error::custom("malformed list"))?;
                Ok(Self::Many(items.into_iter().map(RawValue::get).collect()))
            }
            _ => Err(de::Error::custom("expected a string or a list of strings")),
        }
    }
}

/// `deserialize_with` for a [`SensitiveText`] field.
pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<SensitiveText, D::Error> {
    let raw = <&'de RawValue>::deserialize(deserializer)?;
    unescape_json_string(raw.get()).map_err(de::Error::custom)
}
