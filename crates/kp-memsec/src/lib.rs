//! Owned secret buffers with optional page protections and deterministic cleanup.
//!
//! Borrowed views and external consumers (Qt, parsers, formatters) can create copies
//! outside this allocator. Serde visitors cannot control a parser's escape scratch.
mod protected;
mod region;
pub use protected::ProtectedBytes;
use region::Region;
pub use region::{MemoryDiagnostics, ProtectionStatus, memory_diagnostics};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, SeqAccess, Visitor},
};
use std::{borrow::Borrow, fmt, io, ops::Deref};
use zeroize::Zeroize;

#[derive(Default)]
pub struct SensitiveBytes {
    region: Option<Region>,
    len: usize,
}
impl SensitiveBytes {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn try_with_capacity(capacity: usize) -> io::Result<Self> {
        Ok(Self {
            region: if capacity == 0 {
                None
            } else {
                Some(Region::new(capacity)?)
            },
            len: 0,
        })
    }
    /// Infallible convenience for APIs whose allocation failures are unrecoverable.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::try_with_capacity(capacity)
            .unwrap_or_else(|_| std::alloc::handle_alloc_error(std::alloc::Layout::new::<u8>()))
    }
    pub fn capacity(&self) -> usize {
        self.region.as_ref().map_or(0, |r| r.bytes().len())
    }
    pub fn protection_status(&self) -> ProtectionStatus {
        self.region
            .as_ref()
            .map_or(ProtectionStatus::default(), |r| r.status)
    }
    pub fn as_slice(&self) -> &[u8] {
        self.region.as_ref().map_or(&[], |r| &r.bytes()[..self.len])
    }
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.region
            .as_mut()
            .map_or(&mut [], |r| &mut r.bytes_mut()[..self.len])
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn clear(&mut self) {
        if let Some(r) = &mut self.region {
            r.bytes_mut().zeroize();
        }
        self.len = 0;
    }
    pub fn truncate(&mut self, len: usize) {
        if len < self.len {
            self.as_mut_slice()[len..].zeroize();
            self.len = len;
        }
    }
    pub fn try_extend_from_slice(&mut self, value: &[u8]) -> io::Result<()> {
        let needed = self.len.checked_add(value.len()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "sensitive length overflow")
        })?;
        if needed > self.capacity() {
            let capacity = needed.max(self.capacity().saturating_mul(2));
            let mut replacement = Self::try_with_capacity(capacity)?;
            replacement.try_extend_from_slice(self.as_slice())?;
            std::mem::swap(self, &mut replacement);
            // Drop wipes the old region before releasing its independent reservation.
        }
        if let Some(r) = &mut self.region {
            r.bytes_mut()[self.len..needed].copy_from_slice(value);
        }
        self.len = needed;
        Ok(())
    }
    pub fn extend_from_slice(&mut self, value: &[u8]) {
        self.try_extend_from_slice(value)
            .unwrap_or_else(|_| std::alloc::handle_alloc_error(std::alloc::Layout::new::<u8>()));
    }
    pub fn push(&mut self, value: u8) {
        self.extend_from_slice(&[value]);
    }
    pub fn resize(&mut self, len: usize, value: u8) {
        if len <= self.len {
            self.truncate(len);
            return;
        }
        let old_len = self.len;
        if len > self.capacity() {
            let mut replacement = Self::with_capacity(len.max(self.capacity().saturating_mul(2)));
            replacement.extend_from_slice(self.as_slice());
            std::mem::swap(self, &mut replacement);
        }
        if let Some(region) = &mut self.region {
            region.bytes_mut()[old_len..len].fill(value);
        }
        self.len = len;
    }
    /// Limit a serialization/write operation without changing the buffer's policy.
    pub fn bounded_writer(&mut self, maximum: usize) -> BoundedWriter<'_> {
        BoundedWriter {
            buffer: self,
            maximum,
        }
    }
}
pub struct BoundedWriter<'a> {
    buffer: &'a mut SensitiveBytes,
    maximum: usize,
}
impl io::Write for BoundedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .buffer
            .len
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.maximum)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "sensitive writer limit exceeded",
            ));
        }
        self.buffer.try_extend_from_slice(bytes)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl io::Write for SensitiveBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.try_extend_from_slice(bytes)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Deref for SensitiveBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}
impl AsRef<[u8]> for SensitiveBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}
impl Zeroize for SensitiveBytes {
    fn zeroize(&mut self) {
        self.clear();
    }
}
impl Clone for SensitiveBytes {
    fn clone(&self) -> Self {
        Self::from(self.as_slice())
    }
}
impl PartialEq for SensitiveBytes {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl Eq for SensitiveBytes {}
impl fmt::Debug for SensitiveBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SensitiveBytes")
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}
impl From<&[u8]> for SensitiveBytes {
    fn from(value: &[u8]) -> Self {
        let mut result = Self::with_capacity(value.len());
        result.extend_from_slice(value);
        result
    }
}
impl From<Vec<u8>> for SensitiveBytes {
    fn from(mut value: Vec<u8>) -> Self {
        let result = Self::from(value.as_slice());
        value.zeroize();
        result
    }
}
impl From<SensitiveText> for SensitiveBytes {
    fn from(value: SensitiveText) -> Self {
        value.0
    }
}
impl Serialize for SensitiveBytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.as_slice().serialize(s)
    }
}
impl<'de> Deserialize<'de> for SensitiveBytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct BytesVisitor;
        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = SensitiveBytes;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a byte sequence")
            }
            fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                let mut result = SensitiveBytes::new();
                result.try_extend_from_slice(value).map_err(E::custom)?;
                Ok(result)
            }
            fn visit_byte_buf<E: de::Error>(self, mut value: Vec<u8>) -> Result<Self::Value, E> {
                let result = self.visit_bytes(&value);
                value.zeroize();
                result
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut result = SensitiveBytes::new();
                while let Some(byte) = seq.next_element::<u8>()? {
                    result
                        .try_extend_from_slice(&[byte])
                        .map_err(de::Error::custom)?;
                }
                Ok(result)
            }
        }
        d.deserialize_byte_buf(BytesVisitor)
    }
}

#[derive(Default, Clone, PartialEq, Eq)]
pub struct SensitiveText(SensitiveBytes);
impl SensitiveText {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self(SensitiveBytes::with_capacity(capacity))
    }
    pub fn try_with_capacity(capacity: usize) -> io::Result<Self> {
        SensitiveBytes::try_with_capacity(capacity).map(Self)
    }
    pub fn from_utf8(bytes: SensitiveBytes) -> Result<Self, std::str::Utf8Error> {
        std::str::from_utf8(bytes.as_slice())?;
        Ok(Self(bytes))
    }
    pub fn as_str(&self) -> &str {
        // Only validated UTF-8 enters this wrapper; all mutation preserves boundaries.
        unsafe { std::str::from_utf8_unchecked(self.0.as_slice()) }
    }
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
    pub fn protection_status(&self) -> ProtectionStatus {
        self.0.protection_status()
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn truncate(&mut self, len: usize) {
        assert!(len >= self.len() || self.as_str().is_char_boundary(len));
        self.0.truncate(len);
    }
    pub fn push_str(&mut self, value: &str) {
        self.0.extend_from_slice(value.as_bytes());
    }
    pub fn push(&mut self, value: char) {
        let mut bytes = [0; 4];
        self.push_str(value.encode_utf8(&mut bytes));
        bytes.zeroize();
    }
    pub fn remove(&mut self, index: usize) -> char {
        let Some(ch) = self.as_str()[index..].chars().next() else {
            panic!("cannot remove at text end");
        };
        let end = index + ch.len_utf8();
        let len = self.len();
        self.0.as_mut_slice().copy_within(end..len, index);
        self.0.truncate(len - ch.len_utf8());
        ch
    }
}
impl TryFrom<SensitiveBytes> for SensitiveText {
    type Error = std::str::Utf8Error;
    fn try_from(value: SensitiveBytes) -> Result<Self, Self::Error> {
        Self::from_utf8(value)
    }
}
impl From<&str> for SensitiveText {
    fn from(value: &str) -> Self {
        Self(SensitiveBytes::from(value.as_bytes()))
    }
}
impl From<&String> for SensitiveText {
    fn from(value: &String) -> Self {
        Self::from(value.as_str())
    }
}
impl From<String> for SensitiveText {
    fn from(value: String) -> Self {
        Self(SensitiveBytes::from(value.into_bytes()))
    }
}
impl FromIterator<char> for SensitiveText {
    fn from_iter<T: IntoIterator<Item = char>>(iter: T) -> Self {
        let mut result = Self::new();
        for ch in iter {
            result.push(ch);
        }
        result
    }
}
impl Deref for SensitiveText {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl AsRef<str> for SensitiveText {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl Borrow<str> for SensitiveText {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}
impl Zeroize for SensitiveText {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
impl fmt::Debug for SensitiveText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SensitiveText")
            .field("len", &self.len())
            .finish_non_exhaustive()
    }
}
impl fmt::Write for SensitiveText {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0
            .try_extend_from_slice(s.as_bytes())
            .map_err(|_| fmt::Error)
    }
}
impl PartialEq<str> for SensitiveText {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl PartialEq<&str> for SensitiveText {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}
impl PartialEq<String> for SensitiveText {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}
impl Serialize for SensitiveText {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for SensitiveText {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct TextVisitor;
        impl Visitor<'_> for TextVisitor {
            type Value = SensitiveText;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("UTF-8 text")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let mut result = SensitiveText::new();
                result
                    .0
                    .try_extend_from_slice(value.as_bytes())
                    .map_err(E::custom)?;
                Ok(result)
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                let mut bytes = value.into_bytes();
                let result = self.visit_str(std::str::from_utf8(&bytes).map_err(E::custom)?);
                bytes.zeroize();
                result
            }
        }
        d.deserialize_str(TextVisitor)
    }
}

impl std::hash::Hash for SensitiveText {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}
impl PartialOrd for SensitiveText {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SensitiveText {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialEq<SensitiveText> for String {
    fn eq(&self, other: &SensitiveText) -> bool {
        self.as_str() == other.as_str()
    }
}
impl PartialEq<SensitiveText> for str {
    fn eq(&self, other: &SensitiveText) -> bool {
        self == other.as_str()
    }
}
impl PartialEq<SensitiveText> for &str {
    fn eq(&self, other: &SensitiveText) -> bool {
        *self == other.as_str()
    }
}
