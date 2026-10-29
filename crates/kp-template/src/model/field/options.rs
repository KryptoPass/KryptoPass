//! Per-field options shared by all types.

use crate::model::vocabulary::vocabulary;

vocabulary! {
    /// Whether the decrypted value enters the in-memory search index after unlock.
    ///
    /// Concealed fields are never indexed, whatever the template asks.
    #[derive(Default)]
    pub enum SearchMode {
        /// Not indexed. This is the default when the attribute is absent.
        #[default]
        None => "none",
        /// Indexed while the vault is unlocked.
        Index => "index",
    }
}

/// Whether a field holds one value or a list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Multiplicity {
    /// Exactly one value.
    #[default]
    Single,
    /// An ordered list; the first entry is the primary one.
    Many {
        /// Maximum number of entries.
        max: u16,
    },
}

vocabulary! {
    /// HTML `autocomplete` token, a subset of the standard list. The name is the
    /// token itself.
    pub enum Autocomplete {
        /// Account name.
        Username => "username",
        /// Existing password.
        CurrentPassword => "current-password",
        /// Password being chosen.
        NewPassword => "new-password",
        /// One-time verification code.
        OneTimeCode => "one-time-code",
        /// Email address.
        Email => "email",
        /// Telephone number.
        Tel => "tel",
        /// Given name.
        GivenName => "given-name",
        /// Middle name.
        AdditionalName => "additional-name",
        /// Family name.
        FamilyName => "family-name",
        /// Birthday.
        Bday => "bday",
        /// Street address.
        StreetAddress => "street-address",
        /// Region or state.
        AddressLevel1 => "address-level1",
        /// City.
        AddressLevel2 => "address-level2",
        /// Postal code.
        PostalCode => "postal-code",
        /// Country name.
        CountryName => "country-name",
        /// Name on the card.
        CcName => "cc-name",
        /// Card number.
        CcNumber => "cc-number",
        /// Card expiry.
        CcExp => "cc-exp",
        /// Card security code.
        CcCsc => "cc-csc",
    }
}

/// Plain-text property read only by the implementation of a specific template.
///
/// The generic renderer ignores properties it does not know and shows a plain
/// text field, so an unknown one never loses data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prop {
    /// Property name, for example `widget` or `options`.
    pub name: String,
    /// Property value; stored values are always text.
    pub value: String,
}

/// Accepted number of digits of a `digits` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DigitCount {
    /// Fewest digits.
    pub min: u8,
    /// Most digits.
    pub max: u8,
}
