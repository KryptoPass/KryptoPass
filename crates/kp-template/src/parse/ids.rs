//! Validation of identifiers.

use super::error::ParseError;
use super::reader::El;
use crate::model::{FieldId, InvalidId, Named, SectionId, TemplateId};

/// Arbitrary cap so ids stay short in logs, errors and storage keys.
const MAX_ID_BYTES: usize = 64;

/// Lowercase ASCII letters, digits and `-`, starting with a letter. With
/// `dotted`, `.` separates segments (`user.casa`).
///
/// ASCII-only on purpose: ids are stable storage keys, and restricting them
/// rules out look-alike characters and case variants that would be two keys.
fn is_valid_id(raw: &str, dotted: bool) -> bool {
    let starts_with_letter = raw.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    starts_with_letter
        && raw.len() <= MAX_ID_BYTES
        && !raw.ends_with(['-', '.'])
        && raw.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || (dotted && c == '.')
        })
}

/// Reads attribute `attribute` of `el` as an identifier.
pub(super) fn id_attr(
    el: &El<'_, '_>,
    attribute: impl Named,
    dotted: bool,
) -> Result<String, ParseError> {
    let raw = el.require(attribute)?;
    if is_valid_id(raw, dotted) {
        Ok(raw.to_owned())
    } else {
        Err(el.invalid(attribute))
    }
}

/// Validates an identifier taken from somewhere other than an attribute of `el`.
pub(super) fn checked(
    el: &El<'_, '_>,
    raw: &str,
    attribute: impl Named,
) -> Result<String, ParseError> {
    if is_valid_id(raw, false) {
        Ok(raw.to_owned())
    } else {
        Err(el.invalid(attribute))
    }
}

/// BCP 47 tags are letters, digits and `-`, each subtag at most 8 characters.
pub(super) fn language(el: &El<'_, '_>, raw: &str) -> Result<String, ParseError> {
    let well_formed = !raw.is_empty()
        && raw.len() <= 35
        && raw.split('-').all(|part| {
            (1..=8).contains(&part.len()) && part.chars().all(|c| c.is_ascii_alphanumeric())
        });
    if well_formed {
        Ok(raw.to_owned())
    } else {
        Err(el.invalid_lang())
    }
}

/// `parse` for identifiers read back from storage, with the parser's rules.
///
/// Lets code that persists ids rebuild one without going through XML. It
/// only checks the syntax; whether the id exists is for the template or the
/// library to answer.
macro_rules! parse_from_storage {
    ($($name:ident => $dotted:expr),+ $(,)?) => {$(
        impl $name {
            /// Reads an id from storage, applying the same rules as the parser.
            pub fn parse(raw: &str) -> Result<Self, InvalidId> {
                if is_valid_id(raw, $dotted) {
                    Ok(Self::from_validated(raw.to_owned()))
                } else {
                    Err(InvalidId)
                }
            }
        }
    )+};
}

parse_from_storage! {
    TemplateId => true,
    FieldId => false,
    SectionId => false,
}
