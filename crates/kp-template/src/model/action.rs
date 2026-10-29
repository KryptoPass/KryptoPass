//! Built-in actions a template can expose.

use super::id::FieldId;
use super::vocabulary::vocabulary;

vocabulary! {
    /// Payload format encoded into a QR code.
    pub enum QrFormat {
        /// The `WIFI:` string that phones read to join a network.
        Wifi => "wifi",
    }
}

vocabulary! {
    /// Name a template uses to reference a built-in action.
    pub enum ActionKind {
        /// Shows a QR code built from fields of the item.
        ShowQr => "show-qr",
    }
}

/// An action offered on an item. The set is closed: the application implements
/// each action and a template can only reference it, never define new behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Shows a QR code built from fields of the item.
    ShowQr {
        /// How the fields are encoded into the QR payload.
        format: QrFormat,
        /// Fields read to build the payload, in the order the format expects.
        fields: Vec<FieldId>,
    },
}
