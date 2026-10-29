//! What an item can do besides showing its fields: one-time codes and QR
//! codes, as its template declares.

use kp_template::{Action, FieldType, Template};
use kryptopass_core::{
    ActionError, FieldValue, ItemContent, SensitiveText, TotpSnapshot, generate_totp,
    parse_totp_input,
};

use crate::error::AppError;

/// The current one-time code of a `totp` field at `unix_seconds`, with the
/// next one and the time left. `None` when the field is empty.
pub fn totp_code(
    template: &Template,
    content: &ItemContent,
    field: &str,
    unix_seconds: u64,
) -> Result<Option<TotpSnapshot>, AppError> {
    let field = template.field(field).ok_or(AppError::UnknownField)?;
    if field.field_type != FieldType::Totp {
        return Err(AppError::WrongShape);
    }
    let Some(uri) = content.value(field.id.as_str()).and_then(FieldValue::first) else {
        return Ok(None);
    };
    let config = parse_totp_input(uri)
        .map_err(|_| AppError::InvalidTotp)?
        .ok_or(AppError::InvalidTotp)?;
    generate_totp(&config, unix_seconds)
        .map(Some)
        .map_err(|_| AppError::InvalidTotp)
}

/// The text of the template's QR action, for a QR encoder. `None` when the
/// template offers no QR code.
pub fn qr_payload(
    template: &Template,
    content: &ItemContent,
) -> Result<Option<SensitiveText>, AppError> {
    let Some(action) = template
        .actions
        .iter()
        .find(|action| matches!(action, Action::ShowQr { .. }))
    else {
        return Ok(None);
    };
    content
        .qr_payload(template, action)
        .map(Some)
        .map_err(AppError::Action)
}

impl From<ActionError> for AppError {
    fn from(error: ActionError) -> Self {
        Self::Action(error)
    }
}
