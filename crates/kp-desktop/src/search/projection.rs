//! Borrowed JSON projection: escaped private strings never enter serde scratch.

use kryptopass_search::{Projection, SearchError};

const MAX_USERNAME_BYTES: usize = 16 * 1024;
const MAX_NORMALIZED_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 128;

#[cfg(test)]
#[path = "../../tests/unit/search_projection.rs"]
mod tests;

pub(super) fn login_username(bytes: &[u8]) -> Result<Projection, SearchError> {
    if bytes.len() > kryptopass_core::MAX_CONTENT_BYTES {
        return Err(SearchError::LimitExceeded);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| SearchError::InvalidInput)?;
    let mut fields = None;
    let mut version = false;
    let mut login = false;
    let mut seen = 0_u8;
    visit_object(text, |key, value| {
        let flag = if key.is("schemaVersion") {
            version = value == "1";
            1
        } else if key.is("kind") {
            login = string_value(value)?.is("login");
            2
        } else if key.is("fields") {
            fields = Some(value);
            4
        } else {
            return Err(SearchError::InvalidInput);
        };
        if seen & flag != 0 {
            return Err(SearchError::InvalidInput);
        }
        seen |= flag;
        Ok(())
    })?;
    if seen != 7 || !version || !login {
        return Err(SearchError::InvalidInput);
    }
    let mut username = None;
    let mut seen = 0_u8;
    visit_object(fields.ok_or(SearchError::InvalidInput)?, |key, value| {
        let flag = if key.is("username") {
            username = Some(string_value(value)?);
            1
        } else if key.is("password") {
            2
        } else if key.is("notes") {
            4
        } else if key.is("urls") {
            8
        } else if key.is("totp") {
            16
        } else if key.is("history") {
            32
        } else {
            return Err(SearchError::InvalidInput);
        };
        if seen & flag != 0 {
            return Err(SearchError::InvalidInput);
        }
        seen |= flag;
        Ok(())
    })?;
    if seen != 63 {
        return Err(SearchError::InvalidInput);
    }
    let username = username.ok_or(SearchError::InvalidInput)?;
    let mut length = 0usize;
    for character in username.characters() {
        length = length
            .checked_add(character?.len_utf8())
            .ok_or(SearchError::LimitExceeded)?;
        if length > MAX_USERNAME_BYTES {
            return Err(SearchError::LimitExceeded);
        }
    }
    Projection::from_characters(username.characters(), MAX_NORMALIZED_BYTES)
}

#[derive(Clone, Copy)]
struct JsonString<'a>(&'a str);

impl<'a> JsonString<'a> {
    fn characters(self) -> JsonCharacters<'a> {
        JsonCharacters {
            text: self.0,
            offset: 0,
        }
    }

    fn is(self, expected: &str) -> bool {
        self.characters().eq(expected.chars().map(Ok))
    }
}

#[derive(Clone)]
struct JsonCharacters<'a> {
    text: &'a str,
    offset: usize,
}

impl JsonCharacters<'_> {
    fn hex_quad(&mut self) -> Result<u32, SearchError> {
        let mut value = 0;
        for _ in 0..4 {
            let byte = *self
                .text
                .as_bytes()
                .get(self.offset)
                .ok_or(SearchError::InvalidInput)?;
            self.offset += 1;
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'f' => u32::from(byte - b'a' + 10),
                b'A'..=b'F' => u32::from(byte - b'A' + 10),
                _ => return Err(SearchError::InvalidInput),
            };
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn character(&mut self) -> Result<char, SearchError> {
        let character = self
            .text
            .get(self.offset..)
            .and_then(|text| text.chars().next())
            .ok_or(SearchError::InvalidInput)?;
        self.offset += character.len_utf8();
        if character != '\\' {
            return if character < ' ' || character == '"' {
                Err(SearchError::InvalidInput)
            } else {
                Ok(character)
            };
        }
        let escape = *self
            .text
            .as_bytes()
            .get(self.offset)
            .ok_or(SearchError::InvalidInput)?;
        self.offset += 1;
        match escape {
            b'"' => Ok('"'),
            b'\\' => Ok('\\'),
            b'/' => Ok('/'),
            b'b' => Ok('\u{8}'),
            b'f' => Ok('\u{c}'),
            b'n' => Ok('\n'),
            b'r' => Ok('\r'),
            b't' => Ok('\t'),
            b'u' => {
                let first = self.hex_quad()?;
                let scalar = if (0xd800..=0xdbff).contains(&first) {
                    if self.text.as_bytes().get(self.offset..self.offset + 2) != Some(b"\\u") {
                        return Err(SearchError::InvalidInput);
                    }
                    self.offset += 2;
                    let second = self.hex_quad()?;
                    if !(0xdc00..=0xdfff).contains(&second) {
                        return Err(SearchError::InvalidInput);
                    }
                    0x10000 + ((first - 0xd800) << 10) + second - 0xdc00
                } else {
                    first
                };
                char::from_u32(scalar).ok_or(SearchError::InvalidInput)
            }
            _ => Err(SearchError::InvalidInput),
        }
    }
}

impl Iterator for JsonCharacters<'_> {
    type Item = Result<char, SearchError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.text.len() {
            None
        } else {
            let result = self.character();
            if result.is_err() {
                self.offset = self.text.len();
            }
            Some(result)
        }
    }
}

fn visit_object<'a, F>(text: &'a str, visit: F) -> Result<(), SearchError>
where
    F: FnMut(JsonString<'a>, &'a str) -> Result<(), SearchError>,
{
    let mut cursor = Cursor { text, offset: 0 };
    cursor.whitespace();
    cursor.object(0, visit)?;
    cursor.whitespace();
    if cursor.offset != text.len() {
        return Err(SearchError::InvalidInput);
    }
    Ok(())
}

fn string_value(text: &str) -> Result<JsonString<'_>, SearchError> {
    let mut cursor = Cursor { text, offset: 0 };
    let value = cursor.string()?;
    if cursor.offset != text.len() {
        return Err(SearchError::InvalidInput);
    }
    Ok(value)
}

struct Cursor<'a> {
    text: &'a str,
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            self.offset += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.offset).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), SearchError> {
        if self.peek() != Some(byte) {
            return Err(SearchError::InvalidInput);
        }
        self.offset += 1;
        Ok(())
    }

    fn string(&mut self) -> Result<JsonString<'a>, SearchError> {
        self.expect(b'"')?;
        let start = self.offset;
        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    let raw = self
                        .text
                        .get(start..self.offset)
                        .ok_or(SearchError::InvalidInput)?;
                    self.offset += 1;
                    let value = JsonString(raw);
                    for character in value.characters() {
                        character?;
                    }
                    return Ok(value);
                }
                b'\\' => {
                    self.offset += 2;
                }
                0..=31 => return Err(SearchError::InvalidInput),
                _ => self.offset += 1,
            }
        }
        Err(SearchError::InvalidInput)
    }

    fn object<F>(&mut self, depth: usize, mut visit: F) -> Result<(), SearchError>
    where
        F: FnMut(JsonString<'a>, &'a str) -> Result<(), SearchError>,
    {
        if depth >= MAX_DEPTH {
            return Err(SearchError::LimitExceeded);
        }
        self.expect(b'{')?;
        self.whitespace();
        if self.peek() == Some(b'}') {
            self.offset += 1;
            return Ok(());
        }
        loop {
            let key = self.string()?;
            self.whitespace();
            self.expect(b':')?;
            self.whitespace();
            let start = self.offset;
            self.value(depth + 1)?;
            visit(
                key,
                self.text
                    .get(start..self.offset)
                    .ok_or(SearchError::InvalidInput)?,
            )?;
            self.whitespace();
            if self.peek() == Some(b'}') {
                self.offset += 1;
                return Ok(());
            }
            self.expect(b',')?;
            self.whitespace();
        }
    }

    fn value(&mut self, depth: usize) -> Result<(), SearchError> {
        if depth >= MAX_DEPTH {
            return Err(SearchError::LimitExceeded);
        }
        match self.peek() {
            Some(b'{') => self.object(depth, |_, _| Ok(())),
            Some(b'[') => {
                self.offset += 1;
                self.whitespace();
                if self.peek() == Some(b']') {
                    self.offset += 1;
                    return Ok(());
                }
                loop {
                    self.value(depth + 1)?;
                    self.whitespace();
                    if self.peek() == Some(b']') {
                        self.offset += 1;
                        return Ok(());
                    }
                    self.expect(b',')?;
                    self.whitespace();
                }
            }
            Some(b'"') => self.string().map(|_| ()),
            Some(b't') => self.literal("true"),
            Some(b'f') => self.literal("false"),
            Some(b'n') => self.literal("null"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(SearchError::InvalidInput),
        }
    }

    fn literal(&mut self, value: &str) -> Result<(), SearchError> {
        if !self
            .text
            .get(self.offset..)
            .is_some_and(|text| text.starts_with(value))
        {
            return Err(SearchError::InvalidInput);
        }
        self.offset += value.len();
        Ok(())
    }

    fn digits(&mut self) -> Result<(), SearchError> {
        let start = self.offset;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.offset += 1;
        }
        if start == self.offset {
            return Err(SearchError::InvalidInput);
        }
        Ok(())
    }

    fn number(&mut self) -> Result<(), SearchError> {
        if self.peek() == Some(b'-') {
            self.offset += 1;
        }
        if self.peek() == Some(b'0') {
            self.offset += 1;
        } else {
            self.digits()?;
        }
        if self.peek() == Some(b'.') {
            self.offset += 1;
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.offset += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            self.digits()?;
        }
        Ok(())
    }
}
