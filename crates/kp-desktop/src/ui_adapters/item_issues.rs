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
pub(super) struct InvalidItem(pub(super) String);

impl fmt::Display for InvalidItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for InvalidItem {}

/// Keeps every error as it is, except a refusal for the item's fields, which it explains
/// with the labels of `template`.
pub(super) fn explain(error: VaultError, template: &str) -> anyhow::Error {
    match error {
        VaultError::Invalid(issues) => {
            tracing::info!(issues = %issues, "the item was refused for its fields");
            anyhow::Error::new(InvalidItem(describe(&issues, template)))
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
        return "El elemento no es válido.".to_owned();
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
            .map(|label| format!("«{label}»"))
            .unwrap_or_else(|| "un campo".to_owned()),
        IssueTarget::Custom(index) => format!("el campo añadido n.º {}", index + 1),
        IssueTarget::Value(index) => format!("el valor n.º {}", index + 1),
        IssueTarget::Section(id) => section_title(id.as_str())
            .map(|title| format!("la sección «{title}»"))
            .unwrap_or_else(|| "una sección".to_owned()),
        IssueTarget::Item => "el elemento".to_owned(),
    };
    let limit = issue
        .limit
        .map(|limit| limit.to_string())
        .unwrap_or_default();
    let text = match issue.code.as_str() {
        "required" => format!("Falta {subject}."),
        "too-long" => format!("{subject} supera el máximo de {limit} bytes."),
        "too-many" => format!("{subject} admite como máximo {limit} valores."),
        "invalid-url" => format!("{subject} no contiene una dirección web válida."),
        "invalid-email" => format!("{subject} no contiene un correo válido."),
        "invalid-phone" => format!("{subject} no contiene un teléfono válido."),
        "invalid-number" => format!("{subject} no contiene un número válido."),
        "invalid-date" => format!("{subject} no contiene una fecha válida; usa AAAA-MM-DD."),
        "invalid-totp" => {
            format!("{subject} no contiene una semilla TOTP válida (Base32 u otpauth://).")
        }
        "invalid-card-number" => format!("{subject} no contiene un número de tarjeta válido."),
        "invalid-digits" => format!("{subject} no tiene la cantidad de dígitos esperada."),
        "invalid-choice" => format!("{subject} tiene una opción que no existe."),
        "invalid-characters" => format!("{subject} contiene caracteres no permitidos."),
        "invalid-ssh-key" => format!("{subject} no contiene una clave SSH válida."),
        "invalid-wifi-password" => format!("{subject} no contiene una contraseña Wi-Fi válida."),
        "duplicate-field" | "duplicate-custom-id" => format!("{subject} aparece más de una vez."),
        _ => format!("{subject} no tiene un valor válido."),
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
