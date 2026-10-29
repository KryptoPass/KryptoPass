//! Decoding JSON string literals into protected memory.
//!
//! A parser that decodes escapes (`\"`, `\n`, `\u00e9`…) itself writes the
//! plaintext into a buffer it never wipes. Callers that can get the raw
//! literal instead (serde_json's `RawValue`, a borrowed scanner) decode it here,
//! straight into a [`SensitiveText`].

use crate::SensitiveText;

/// Decodes one raw JSON string literal, quotes included.
///
/// The parser already checked the syntax while scanning; this checks again
/// because it is the code that builds the value. Messages never quote input.
pub fn unescape_json_string(raw: &str) -> Result<SensitiveText, &'static str> {
    let inner = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or("expected a string")?;
    // Decoding never makes the text longer, so this never reallocates and
    // leaves no stale copy in a freed buffer.
    let mut text = SensitiveText::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        let decoded = match chars.next() {
            Some('"') => '"',
            Some('\\') => '\\',
            Some('/') => '/',
            Some('b') => '\u{8}',
            Some('f') => '\u{c}',
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('u') => unicode_escape(&mut chars)?,
            _ => return Err("invalid escape in string"),
        };
        text.push(decoded);
    }
    Ok(text)
}

/// The character of a `\uXXXX` escape, joining a UTF-16 surrogate pair.
fn unicode_escape(chars: &mut std::str::Chars<'_>) -> Result<char, &'static str> {
    let high = hex4(chars)?;
    let code = match high {
        0xD800..=0xDBFF => {
            if chars.next() != Some('\\') || chars.next() != Some('u') {
                return Err("unpaired surrogate in string");
            }
            let low = hex4(chars)?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err("unpaired surrogate in string");
            }
            0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
        }
        0xDC00..=0xDFFF => return Err("unpaired surrogate in string"),
        _ => high,
    };
    char::from_u32(code).ok_or("invalid unicode escape in string")
}

fn hex4(chars: &mut std::str::Chars<'_>) -> Result<u32, &'static str> {
    (0..4).try_fold(0, |code, _| {
        let digit = chars
            .next()
            .and_then(|c| c.to_digit(16))
            .ok_or("invalid unicode escape in string")?;
        Ok(code * 16 + digit)
    })
}
