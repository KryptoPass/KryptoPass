//! Rules across fields that a template can ask the application to enforce.

use super::id::FieldId;
use super::vocabulary::vocabulary;

vocabulary! {
    /// A rule the application implements; a template can only reference it.
    pub enum CheckKind {
        /// The password fits the security mode. Fields: security (a `choice`
        /// with `none`, `wep`, `wpa`, `wpa3`), password.
        WifiPassword => "wifi-password",
    }
}

impl CheckKind {
    /// How many fields the rule reads, in the order its documentation gives.
    pub const fn arity(self) -> usize {
        match self {
            Self::WifiPassword => 2,
        }
    }
}

/// A rule applied to fields of an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    /// Which rule.
    pub kind: CheckKind,
    /// Fields the rule reads, in its order.
    pub fields: Vec<FieldId>,
}
