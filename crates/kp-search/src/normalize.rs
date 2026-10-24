use crate::{SearchError, allocation_error};
use deunicode::deunicode_char;
use kryptopass_secure_memory::ProtectedBytes;
use std::io::{self, Write};
use zeroize::Zeroizing;

/// A minimal normalized field. It cannot be cloned, formatted or serialized.
pub struct Projection(ProtectedBytes);

impl Projection {
    pub fn from_text(text: &str, maximum: usize) -> Result<Self, SearchError> {
        Self::from_characters(text.chars().map(Ok), maximum)
    }

    /// A repeatable decoder can stream escaped input directly into protected
    /// storage. Neither pass materializes a decoded ordinary heap string.
    pub fn from_characters<I>(characters: I, maximum: usize) -> Result<Self, SearchError>
    where
        I: Iterator<Item = Result<char, SearchError>> + Clone,
    {
        let mut counter = Counter { length: 0, maximum };
        normalize(characters.clone(), &mut counter)?;
        let mut bytes =
            ProtectedBytes::try_with_capacity(counter.length).map_err(allocation_error)?;
        normalize(characters, &mut bytes)?;
        if bytes.len() != counter.length {
            return Err(SearchError::InvalidInput);
        }
        Ok(Self(bytes))
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

pub(crate) fn public_field(text: &str, maximum: usize) -> Result<String, SearchError> {
    let mut counter = Counter { length: 0, maximum };
    normalize(text.chars().map(Ok), &mut counter)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(counter.length)
        .map_err(|_| SearchError::LimitExceeded)?;
    normalize(text.chars().map(Ok), &mut bytes)?;
    String::from_utf8(bytes).map_err(|_| SearchError::InvalidInput)
}

fn normalize<I, W>(characters: I, writer: &mut W) -> Result<(), SearchError>
where
    I: Iterator<Item = Result<char, SearchError>>,
    W: Write,
{
    let mut pending_space = false;
    let mut emitted = false;
    for character in characters {
        let character = character?;
        let mut scratch = Zeroizing::new([0_u8; 4]);
        let mapped = if character.is_whitespace() {
            " "
        } else {
            deunicode_char(character).unwrap_or_else(|| character.encode_utf8(&mut *scratch))
        };
        for byte in mapped.bytes() {
            if byte.is_ascii_whitespace() {
                pending_space = emitted;
                continue;
            }
            if pending_space {
                writer.write_all(b" ").map_err(allocation_error)?;
                pending_space = false;
            }
            writer
                .write_all(&[byte.to_ascii_lowercase()])
                .map_err(allocation_error)?;
            emitted = true;
        }
    }
    Ok(())
}

struct Counter {
    length: usize,
    maximum: usize,
}

impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.length = self
            .length
            .checked_add(bytes.len())
            .filter(|length| *length <= self.maximum)
            .ok_or_else(|| io::Error::other("normalized field limit exceeded"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
