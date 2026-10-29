//! Bringing values to their stored form before validation.

use super::content::ItemContent;
use super::ssh::{DerivedKey, private_key_field};
use super::value::FieldValue;
use crate::{SensitiveText, format_totp_input, parse_totp_input};

use kp_template::{Field, FieldType, Template};

impl ItemContent {
    /// Puts values in canonical form: structured single-line types lose
    /// surrounding whitespace, a TOTP secret becomes its `otpauth://` URI, a
    /// card number loses its separators, derived fields are recomputed from
    /// their source, and empty values and empty list entries are dropped.
    ///
    /// Free text and secrets are never altered: whitespace in a password is
    /// part of it. Entries whose id the template does not know are left for
    /// [`ItemContent::validate`] to report.
    pub fn normalize(&mut self, template: &Template) {
        self.values.retain_mut(|entry| {
            let Some(field) = template.field(entry.id.as_str()) else {
                return true;
            };
            match &mut entry.value {
                FieldValue::One(text) => {
                    normalize_text(field, text);
                    !text.is_empty()
                }
                FieldValue::Many(list) => {
                    list.iter_mut().for_each(|text| normalize_text(field, text));
                    list.retain(|text| !text.is_empty());
                    !list.is_empty()
                }
            }
        });
        self.derive(template);
    }

    /// Fills derived fields from their source, or clears them when the
    /// source is empty. An unreadable source leaves them for validation.
    fn derive(&mut self, template: &Template) {
        let Some(source) = private_key_field(template) else {
            return;
        };
        let derived = self
            .value(source)
            .and_then(FieldValue::first)
            .map(|key| DerivedKey::from_private(key));
        for field in template
            .fields()
            .filter(|field| field.field_type.is_derived())
        {
            match &derived {
                None => {
                    self.remove(field.id.as_str());
                }
                Some(None) => {}
                Some(Some(key)) => {
                    if let Some(value) = key.value_for(field.field_type) {
                        self.set(
                            field.id.clone(),
                            FieldValue::One(SensitiveText::from(value)),
                        );
                    }
                }
            }
        }
    }
}

fn normalize_text(field: &Field, text: &mut SensitiveText) {
    match field.field_type {
        FieldType::Url
        | FieldType::Email
        | FieldType::Phone
        | FieldType::Number
        | FieldType::Date
        | FieldType::Digits
        | FieldType::Choice
        | FieldType::Boolean
        | FieldType::SshPublicKey
        | FieldType::SshFingerprint => trim(text),
        FieldType::CardNumber => {
            if text.contains([' ', '-']) {
                *text = text.chars().filter(|c| !matches!(c, ' ' | '-')).collect();
            }
        }
        FieldType::Totp => {
            trim(text);
            // An unreadable value stays as typed so validation reports it.
            if let Ok(Some(config)) = parse_totp_input(text)
                && let Ok(uri) = format_totp_input(&config)
            {
                *text = uri;
            }
        }
        FieldType::Text
        | FieldType::Multiline
        | FieldType::Secret
        | FieldType::SecretMultiline
        | FieldType::SshPrivateKey => {}
    }
}

/// Replaces the text with a trimmed copy; the old buffer is zeroized on drop.
fn trim(text: &mut SensitiveText) {
    let trimmed = text.trim();
    if trimmed.len() != text.len() {
        *text = SensitiveText::from(trimmed);
    }
}
