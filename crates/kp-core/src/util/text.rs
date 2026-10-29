//! Checks for short text shown on one line: titles, names, keys.

/// Control characters break single-line rendering, autofill and logs, and can
/// hide part of a value. Multi-line text keeps line breaks and tabs.
pub fn has_control_characters(text: &str, multiline: bool) -> bool {
    text.chars()
        .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
}

/// Whether `text` is already a clean line: non-empty, without surrounding
/// whitespace or control characters, and at most `max` bytes.
pub fn is_clean_line(text: &str, max: usize) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text.trim().len() == text.len()
        && !has_control_characters(text, false)
}

/// `text` trimmed, if the result is a clean line of at most `max` bytes.
pub fn clean_line(text: &str, max: usize) -> Option<String> {
    let text = text.trim();
    is_clean_line(text, max).then(|| text.to_owned())
}
