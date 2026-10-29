//! What the browser extension may fill, decided by templates.
//!
//! An item is fillable when its template has a field the browser matches
//! sites with (`browser-match`) and a password field. Which value is the
//! username, the password or the one-time code comes from the fields'
//! `autocomplete` tokens and types, never from the item's kind.

use kp_template::{Autocomplete, FieldType, Template};
use kryptopass_core::vault::{TemplateRegistry, VaultError};
use kryptopass_core::{FieldValue, ItemContent, SecretId, SensitiveText};

use crate::error::AppError;
use crate::session::AppVault;

/// An item the browser may offer, without its values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub id: SecretId,
    pub title: String,
    pub revision: u64,
}

/// The values to fill. Every text is zeroized on drop.
pub struct Credentials {
    pub username: Option<SensitiveText>,
    pub password: Option<SensitiveText>,
    /// `otpauth://` URI; the caller computes the current code.
    pub totp: Option<SensitiveText>,
    pub urls: Vec<SensitiveText>,
}

/// Items that are not in the trash and whose template is fillable.
pub fn candidates(vault: &AppVault) -> Result<Vec<Candidate>, AppError> {
    let templates = vault.templates();
    Ok(vault
        .list_items()?
        .into_iter()
        .filter(|meta| !meta.state.trashed && fillable(templates, meta.state.template.as_str()))
        .map(|meta| Candidate {
            id: meta.id,
            title: meta.state.title,
            revision: meta.revision,
        })
        .collect())
}

/// The values of a candidate, only if it is still the revision offered.
pub fn credentials(vault: &AppVault, candidate: &Candidate) -> Result<Credentials, AppError> {
    let item = vault.open_item(candidate.id)?;
    if item.meta.revision != candidate.revision || item.meta.state.trashed {
        return Err(VaultError::StaleRevision.into());
    }
    let template = vault
        .templates()
        .template(item.meta.state.template.as_str())?;
    if !is_fillable(template) {
        return Err(AppError::NotFillable);
    }
    let content = &item.content;
    let by_token = |tokens: &[Autocomplete]| {
        tokens.iter().find_map(|token| {
            template
                .fields()
                .filter(|field| field.autocomplete == Some(*token))
                .find_map(|field| first(content, field.id.as_str()))
        })
    };
    Ok(Credentials {
        username: by_token(&[Autocomplete::Username, Autocomplete::Email]),
        password: by_token(&[Autocomplete::CurrentPassword, Autocomplete::NewPassword]),
        totp: template
            .fields()
            .filter(|field| field.field_type == FieldType::Totp)
            .find_map(|field| first(content, field.id.as_str())),
        urls: template
            .fields()
            .filter(|field| field.browser_match)
            .filter_map(|field| content.value(field.id.as_str()))
            .flat_map(|value| value.texts().iter().cloned())
            .collect(),
    })
}

fn fillable(templates: &TemplateRegistry, id: &str) -> bool {
    templates.template(id).is_ok_and(is_fillable)
}

fn is_fillable(template: &Template) -> bool {
    let matches_sites = template.fields().any(|field| field.browser_match);
    let has_password = template.fields().any(|field| {
        matches!(
            field.autocomplete,
            Some(Autocomplete::CurrentPassword | Autocomplete::NewPassword)
        )
    });
    matches_sites && has_password
}

fn first(content: &ItemContent, field: &str) -> Option<SensitiveText> {
    content
        .value(field)
        .and_then(FieldValue::first)
        .filter(|text| !text.is_empty())
        .cloned()
}
