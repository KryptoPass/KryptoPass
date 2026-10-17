//! Translation at the boundary between backend messages and Qt presentation.
use cxx_qt_lib::QString;

pub fn translate(source: &QString) -> QString {
    crate::ui_adapters::native_ffi::ffi::translate_ui(source)
}

pub fn text(source: &str) -> String {
    translate(&QString::from(source)).to_string()
}

/// Substitute Qt-style numbered placeholders without interpreting argument contents.
pub fn format(source: &str, arguments: &[String]) -> String {
    let translated = text(source);
    let mut output = String::new();
    let mut remainder = translated.as_str();
    while let Some(start) = remainder.find('%') {
        output.push_str(&remainder[..start]);
        remainder = &remainder[start + 1..];
        let digits = remainder.bytes().take_while(u8::is_ascii_digit).count();
        let argument = remainder[..digits]
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| arguments.get(n));
        if let Some(argument) = argument {
            output.push_str(argument);
            remainder = &remainder[digits..];
        } else {
            output.push('%');
        }
    }
    output.push_str(remainder);
    output
}

pub fn plural(source: &str, count: usize) -> QString {
    crate::ui_adapters::native_ffi::ffi::translate_plural(
        &QString::from(source),
        i32::try_from(count).unwrap_or(i32::MAX),
    )
}

#[cfg(test)]
#[path = "../tests/unit/i18n.rs"]
mod tests;
