//! The editor's message for an item the vault refused: what is wrong and in which field.
//!
//! Core reports problems as codes and targets, never sentences (`kp-core` issue.rs); this
//! turns them into text with the field's own label, so a missing username does not read as a
//! generic "check the fields" that mentions dates on a form with none. Only labels and limits
//! go into the text, never values, so it is safe to show and to log.

use std::fmt;

use kryptopass_core::vault::VaultError;
use kryptopass_core::{IssueTarget, ItemIssue, ItemIssues};

/// A save the vault refused for its fields, already explained for the editor.
#[derive(Debug)]
pub(super) struct InvalidItem {
    pub(super) message: String,
    /// First template field named by the refusal, for focusing its input.
    pub(super) field: Option<String>,
}

impl fmt::Display for InvalidItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for InvalidItem {}

/// Keeps every error as it is, except a refusal for the item's fields, which it explains
/// with the labels of `template`.
pub(super) fn explain(error: VaultError, template: &str) -> anyhow::Error {
    match error {
        VaultError::Invalid(issues) => {
            tracing::info!(issues = %issues, "the item was refused for its fields");
            let field = issues.0.iter().find_map(|issue| match &issue.target {
                IssueTarget::Field(id) => Some(id.as_str().to_owned()),
                _ => None,
            });
            anyhow::Error::new(InvalidItem {
                message: describe(&issues, template),
                field,
            })
        }
        other => other.into(),
    }
}

fn describe(issues: &ItemIssues, template: &str) -> String {
    let registry = crate::vault_access::templates().ok();
    let language = crate::vault_access::language();
    let template = registry
        .as_ref()
        .and_then(|registry| registry.template(template).ok());
    let field_label = |id: &str| -> Option<String> {
        let (registry, template) = (registry.as_ref()?, template?);
        let field = template
            .sections
            .iter()
            .flat_map(|section| section.fields.iter())
            .find(|field| field.id.as_str() == id)?;
        registry
            .library()
            .text(&field.label, &language)
            .map(str::to_owned)
    };
    let section_title = |id: &str| -> Option<String> {
        let title = registry.as_ref()?.library().section_title(id, &language);
        (!title.is_empty()).then(|| title.to_owned())
    };
    let mut sentences: Vec<String> = Vec::new();
    for issue in &issues.0 {
        let sentence = sentence(issue, &field_label, &section_title);
        if !sentences.contains(&sentence) {
            sentences.push(sentence);
        }
    }
    if sentences.is_empty() {
        return crate::i18n::tr_noop("The item is not valid.").to_owned();
    }
    sentences.join(" ")
}

fn sentence(
    issue: &ItemIssue,
    field_label: &dyn Fn(&str) -> Option<String>,
    section_title: &dyn Fn(&str) -> Option<String>,
) -> String {
    // Phrased so the sentence never agrees with the label's gender: «Contraseña» and
    // «Sitio web» read the same way.
    let subject = match &issue.target {
        IssueTarget::Field(id) => field_label(id.as_str())
            .map(|label| crate::i18n::trf("“%1”", &[label]))
            .unwrap_or_else(|| crate::i18n::tr("a field")),
        IssueTarget::Custom(index) => {
            crate::i18n::trf("the added field #%1", &[(index + 1).to_string()])
        }
        IssueTarget::Value(index) => crate::i18n::trf("the value #%1", &[(index + 1).to_string()]),
        IssueTarget::Section(id) => section_title(id.as_str())
            .map(|title| crate::i18n::trf("the section “%1”", &[title]))
            .unwrap_or_else(|| crate::i18n::tr("a section")),
        IssueTarget::Item => crate::i18n::tr("the item"),
    };
    let limit = issue
        .limit
        .map(|limit| limit.to_string())
        .unwrap_or_default();
    let arguments = [subject, limit];
    let text = match issue.code.as_str() {
        "required" => crate::i18n::trf("%1 is missing.", &arguments),
        "too-long" => crate::i18n::trf("%1 exceeds the maximum of %2 bytes.", &arguments),
        "too-many" => crate::i18n::trf("%1 accepts at most %2 values.", &arguments),
        "invalid-url" => crate::i18n::trf("%1 does not contain a valid web address.", &arguments),
        "invalid-email" => {
            crate::i18n::trf("%1 does not contain a valid email address.", &arguments)
        }
        "invalid-phone" => {
            crate::i18n::trf("%1 does not contain a valid phone number.", &arguments)
        }
        "invalid-number" => crate::i18n::trf("%1 does not contain a valid number.", &arguments),
        "invalid-date" => crate::i18n::trf(
            "%1 does not contain a valid date; use YYYY-MM-DD.",
            &arguments,
        ),
        "invalid-totp" => crate::i18n::trf(
            "%1 does not contain a valid TOTP seed (Base32 or otpauth://).",
            &arguments,
        ),
        "invalid-card-number" => {
            crate::i18n::trf("%1 does not contain a valid card number.", &arguments)
        }
        "invalid-digits" => crate::i18n::trf(
            "%1 does not have the expected number of digits.",
            &arguments,
        ),
        "invalid-choice" => crate::i18n::trf("%1 has an option that does not exist.", &arguments),
        "invalid-characters" => {
            crate::i18n::trf("%1 contains characters that are not allowed.", &arguments)
        }
        "invalid-ssh-key" => crate::i18n::trf("%1 does not contain a valid SSH key.", &arguments),
        "invalid-wifi-password" => {
            crate::i18n::trf("%1 does not contain a valid Wi-Fi password.", &arguments)
        }
        "duplicate-field" | "duplicate-custom-id" => {
            crate::i18n::trf("%1 appears more than once.", &arguments)
        }
        _ => crate::i18n::trf("%1 does not have a valid value.", &arguments),
    };
    capitalized(text)
}

/// Upper-cases the first letter, for sentences that start with a lower-case subject.
fn capitalized(text: String) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}
