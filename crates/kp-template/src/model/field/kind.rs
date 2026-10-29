//! The closed vocabulary of field types and their type-specific settings.

use crate::model::vocabulary::vocabulary;

vocabulary! {
    /// Closed set of field types. The type fixes the default behavior of a field.
    pub enum FieldType {
        /// Single line of visible text.
        Text => "text",
        /// Several lines of visible text, optionally Markdown.
        Multiline => "multiline",
        /// Single concealed line, such as a password.
        Secret => "secret",
        /// Several concealed lines, such as a private key.
        SecretMultiline => "secret-multiline",
        /// Web address, validated and normalized.
        Url => "url",
        /// Email address, with basic validation.
        Email => "email",
        /// Phone number, with basic validation.
        Phone => "phone",
        /// Numeric value. Identifiers that need leading zeros or exact digits
        /// belong in `Text`.
        Number => "number",
        /// Calendar date, shown in the regional format.
        Date => "date",
        /// Value is an `otpauth://` URI; always concealed and never indexed.
        Totp => "totp",
        /// Payment card number: 12 to 19 digits, stored without separators.
        /// Concealed.
        CardNumber => "card-number",
        /// Digits only, with a length range (`min-length`, `max-length`), such
        /// as a CVV or a PIN.
        Digits => "digits",
        /// One of the values listed in the field's `options` property.
        Choice => "choice",
        /// `true` or `false`.
        Boolean => "boolean",
        /// OpenSSH private key. Concealed; it fills the template's
        /// `ssh-public-key` and `ssh-fingerprint` fields.
        SshPrivateKey => "ssh-private-key",
        /// OpenSSH public key, derived from the private key.
        SshPublicKey => "ssh-public-key",
        /// SHA-256 fingerprint of the key, derived from the private key.
        SshFingerprint => "ssh-fingerprint",
    }
}

impl FieldType {
    /// Whether the type is concealed by default and can never be revealed by a template.
    pub const fn is_concealed(self) -> bool {
        matches!(
            self,
            Self::Secret
                | Self::SecretMultiline
                | Self::Totp
                | Self::CardNumber
                | Self::SshPrivateKey
        )
    }

    /// Whether values keep line breaks.
    pub const fn is_multiline(self) -> bool {
        matches!(
            self,
            Self::Multiline | Self::SecretMultiline | Self::SshPrivateKey
        )
    }

    /// Whether the application computes the value from another field, so the
    /// person never types it.
    pub const fn is_derived(self) -> bool {
        matches!(self, Self::SshPublicKey | Self::SshFingerprint)
    }
}

vocabulary! {
    /// Granularity of a `Date` field.
    #[derive(Default)]
    pub enum DatePrecision {
        /// Year, month and day.
        #[default]
        Day => "day",
        /// Year and month only, for example a card expiry.
        Month => "month",
    }
}

vocabulary! {
    /// How a `Multiline` field is rendered.
    #[derive(Default)]
    pub enum TextFormat {
        /// Shown as written.
        #[default]
        Plain => "plain",
        /// Rendered as Markdown.
        Markdown => "markdown",
    }
}
