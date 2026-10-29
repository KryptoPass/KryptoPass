//! Rules across fields and built-in actions, as a template references them.

use thiserror::Error;

use super::content::ItemContent;
use super::issue::{IssueCode, IssueTarget, ItemIssue};
use super::value::FieldValue;
use crate::SensitiveText;
use crate::domain::wifi::{self, Security};

use kp_template::{Action, CheckKind, FieldId, QrFormat, Template};

/// Why an action could not produce its result. Never carries a value.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActionError {
    #[error("the item does not belong to the template")]
    WrongTemplate,
    #[error("a field the action reads is empty or invalid")]
    InvalidInput,
    /// The format cannot represent this item, such as a WPA3 network in a
    /// Wi-Fi QR code.
    #[error("the item cannot be expressed in this format")]
    Unsupported,
}

impl ItemContent {
    /// Runs the template's cross-field rules.
    pub(super) fn run_checks(&self, template: &Template, issues: &mut Vec<ItemIssue>) {
        for check in &template.checks {
            match check.kind {
                CheckKind::WifiPassword => {
                    let [security, password] = check.fields.as_slice() else {
                        continue;
                    };
                    // An unknown mode is reported by the `choice` rule.
                    let Some(mode) = Security::from_option(self.text(security)) else {
                        continue;
                    };
                    if !wifi::password_fits(mode, self.text(password)) {
                        issues.push(ItemIssue::new(
                            IssueCode::INVALID_WIFI_PASSWORD,
                            IssueTarget::Field(password.clone()),
                        ));
                    }
                }
            }
        }
    }

    /// The text a QR action encodes, built from the fields it names.
    pub fn qr_payload(
        &self,
        template: &Template,
        action: &Action,
    ) -> Result<SensitiveText, ActionError> {
        if self.template != template.id {
            return Err(ActionError::WrongTemplate);
        }
        let Action::ShowQr { format, fields } = action;
        match format {
            QrFormat::Wifi => {
                let [ssid, security, password, hidden] = fields.as_slice() else {
                    return Err(ActionError::InvalidInput);
                };
                let mode =
                    Security::from_option(self.text(security)).ok_or(ActionError::InvalidInput)?;
                let ssid = self.text(ssid);
                let password = self.text(password);
                if ssid.is_empty() || !wifi::password_fits(mode, password) {
                    return Err(ActionError::InvalidInput);
                }
                wifi::qr_payload(ssid, mode, password, self.text(hidden) == "true")
                    .ok_or(ActionError::Unsupported)
            }
        }
    }

    /// The first text of a field, or empty.
    fn text(&self, field: &FieldId) -> &str {
        self.value(field.as_str())
            .and_then(FieldValue::first)
            .map_or("", |text| text.as_str())
    }
}
