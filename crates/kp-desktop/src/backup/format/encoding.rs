//! Bounded serde containers, base64 payloads, and sensitive output buffering.

use std::fmt;
use std::io::Write;

use kryptopass_secure_memory::SensitiveBytes;
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroize;

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Serialize)]
#[serde(transparent)]
pub(super) struct BoundedVec<T, const LIMIT: usize>(pub(super) Vec<T>);

impl<T, const LIMIT: usize> BoundedVec<T, LIMIT> {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T, const LIMIT: usize> Default for BoundedVec<T, LIMIT> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<'de, T, const LIMIT: usize> Deserialize<'de> for BoundedVec<T, LIMIT>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BoundedVisitor<T, const LIMIT: usize>(std::marker::PhantomData<T>);

        impl<'de, T, const LIMIT: usize> Visitor<'de> for BoundedVisitor<T, LIMIT>
        where
            T: Deserialize<'de>,
        {
            type Value = BoundedVec<T, LIMIT>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "an array with at most {LIMIT} items")
            }

            fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let capacity = sequence.size_hint().unwrap_or(0).min(LIMIT);
                let mut values = Vec::with_capacity(capacity);
                while let Some(value) = sequence.next_element()? {
                    if values.len() == LIMIT {
                        return Err(de::Error::invalid_length(values.len() + 1, &self));
                    }
                    values.push(value);
                }
                Ok(BoundedVec(values))
            }
        }

        deserializer.deserialize_seq(BoundedVisitor::<T, LIMIT>(std::marker::PhantomData))
    }
}

pub(super) struct Base64Bytes(pub(super) Vec<u8>);

impl Serialize for Base64Bytes {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64_encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Base64Bytes {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Base64Visitor;

        impl Visitor<'_> for Base64Visitor {
            type Value = Base64Bytes;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded base64 ciphertext string")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                base64_decode(value).map(Base64Bytes).map_err(E::custom)
            }

            fn visit_string<E>(self, mut value: String) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                let decoded = base64_decode(&value).map(Base64Bytes).map_err(E::custom);
                value.zeroize();
                decoded
            }
        }

        deserializer.deserialize_str(Base64Visitor)
    }
}

pub(super) struct IconBytes(pub(super) Vec<u8>);

impl Serialize for IconBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64_encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for IconBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct IconVisitor;
        impl Visitor<'_> for IconVisitor {
            type Value = IconBytes;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded base64 icon string")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<IconBytes, E> {
                if value.len() > kryptopass_core::MAX_ENTRY_ICON_BYTES.div_ceil(3) * 4 {
                    return Err(E::custom("icon exceeds maximum size"));
                }
                let bytes = base64_decode(value).map_err(E::custom)?;
                if bytes.len() > kryptopass_core::MAX_ENTRY_ICON_BYTES {
                    return Err(E::custom("icon exceeds maximum size"));
                }
                Ok(IconBytes(bytes))
            }
        }
        deserializer.deserialize_str(IconVisitor)
    }
}

pub(super) struct BoundedBackupWriter {
    bytes: SensitiveBytes,
    maximum: usize,
}

impl BoundedBackupWriter {
    pub(super) fn new(maximum: usize) -> Self {
        Self {
            bytes: SensitiveBytes::default(),
            maximum,
        }
    }

    pub(super) fn into_bytes(mut self) -> SensitiveBytes {
        std::mem::take(&mut self.bytes)
    }
}

impl Write for BoundedBackupWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let length = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("backup size overflow"))?;
        if length > self.maximum {
            return Err(std::io::Error::other("backup exceeds maximum size"));
        }
        self.bytes.try_extend_from_slice(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(BASE64[(a >> 2) as usize] as char);
        output.push(BASE64[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            BASE64[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            BASE64[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

pub(in crate::backup) fn base64_decode(value: &str) -> std::result::Result<Vec<u8>, &'static str> {
    ensure_base64_limit(value)?;
    if value.is_empty() || value.len() % 4 != 0 {
        return Err("backup ciphertext encoding is invalid");
    }
    let padding = value
        .as_bytes()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    if padding > 2 {
        return Err("backup ciphertext encoding is invalid");
    }
    let decoded_len = value.len() / 4 * 3 - padding;
    if decoded_len == 0 || decoded_len > crate::infrastructure::MAX_CIPHERTEXT_BYTES {
        return Err("backup ciphertext size is invalid");
    }
    let chunks = value.as_bytes().chunks_exact(4);
    let mut output = Vec::with_capacity(decoded_len);
    for (index, chunk) in chunks.enumerate() {
        let last = index + 1 == value.len() / 4;
        let a = base64_value(chunk[0]).ok_or("backup ciphertext encoding is invalid")?;
        let b = base64_value(chunk[1]).ok_or("backup ciphertext encoding is invalid")?;
        output.push((a << 2) | (b >> 4));
        if chunk[2] == b'=' {
            if !last || chunk[3] != b'=' || b & 0x0f != 0 {
                return Err("backup ciphertext encoding is invalid");
            }
        } else {
            let c = base64_value(chunk[2]).ok_or("backup ciphertext encoding is invalid")?;
            output.push((b << 4) | (c >> 2));
            if chunk[3] == b'=' {
                if !last || c & 0x03 != 0 {
                    return Err("backup ciphertext encoding is invalid");
                }
            } else {
                let d = base64_value(chunk[3]).ok_or("backup ciphertext encoding is invalid")?;
                output.push((c << 6) | d);
            }
        }
    }
    if output.len() != decoded_len {
        return Err("backup ciphertext encoding is invalid");
    }
    Ok(output)
}

fn ensure_base64_limit(value: &str) -> std::result::Result<(), &'static str> {
    if value.len() > crate::infrastructure::MAX_CIPHERTEXT_BYTES.div_ceil(3) * 4 {
        Err("backup ciphertext size is invalid")
    } else {
        Ok(())
    }
}

fn base64_value(byte: u8) -> Option<u8> {
    BASE64
        .iter()
        .position(|candidate| *candidate == byte)
        .map(|i| i as u8)
}
