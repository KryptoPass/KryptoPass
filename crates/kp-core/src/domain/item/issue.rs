//! Validation problems as stable codes, not sentences.
//!
//! Core never writes text a person reads. An issue carries a code, what it
//! concerns and the limit involved; the UI turns that into a message in the
//! user's language, so adding a language never touches validation. A code is a
//! plain string, so a rule added outside this crate can define its own.
//!
//! Targets name template ids or positions, never strings read from the
//! payload, so an issue is safe to log even when the payload is hostile.

use std::error::Error;
use std::fmt;

use kp_template::{FieldId, SectionId};

/// Stable, language-neutral identifier of a kind of problem, such as `too-long`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IssueCode(&'static str);

impl IssueCode {
    pub const TEMPLATE_MISMATCH: Self = Self("template-mismatch");
    pub const UNKNOWN_FIELD: Self = Self("unknown-field");
    pub const DUPLICATE_FIELD: Self = Self("duplicate-field");
    pub const WRONG_SHAPE: Self = Self("wrong-shape");
    pub const REQUIRED: Self = Self("required");
    pub const TOO_LONG: Self = Self("too-long");
    pub const TOO_MANY: Self = Self("too-many");
    pub const INVALID_CHARACTERS: Self = Self("invalid-characters");
    pub const INVALID_URL: Self = Self("invalid-url");
    pub const INVALID_EMAIL: Self = Self("invalid-email");
    pub const INVALID_PHONE: Self = Self("invalid-phone");
    pub const INVALID_NUMBER: Self = Self("invalid-number");
    pub const INVALID_DATE: Self = Self("invalid-date");
    pub const INVALID_TOTP: Self = Self("invalid-totp");
    pub const INVALID_CARD_NUMBER: Self = Self("invalid-card-number");
    pub const INVALID_DIGITS: Self = Self("invalid-digits");
    pub const INVALID_CHOICE: Self = Self("invalid-choice");
    pub const INVALID_BOOLEAN: Self = Self("invalid-boolean");
    pub const INVALID_SSH_KEY: Self = Self("invalid-ssh-key");
    /// A derived field does not hold what its source produces.
    pub const DERIVED_MISMATCH: Self = Self("derived-mismatch");
    pub const INVALID_WIFI_PASSWORD: Self = Self("invalid-wifi-password");
    pub const INVALID_CUSTOM_ID: Self = Self("invalid-custom-id");
    pub const DUPLICATE_CUSTOM_ID: Self = Self("duplicate-custom-id");
    pub const INVALID_CUSTOM_KEY: Self = Self("invalid-custom-key");
    pub const UNKNOWN_SECTION: Self = Self("unknown-section");

    /// Defines a code for a rule added outside this crate.
    pub const fn new(code: &'static str) -> Self {
        Self(code)
    }

    /// The code as shown to the UI layer for translation.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// What an issue is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueTarget {
    /// The payload as a whole.
    Item,
    /// A field the template declares.
    Field(FieldId),
    /// An entry of `values` by position, when its id is unknown or repeated.
    Value(usize),
    /// A user-added field by its position in `custom`.
    Custom(usize),
    /// A section, for the limit on its user-added fields.
    Section(SectionId),
}

impl fmt::Display for IssueTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Item => formatter.write_str("item"),
            Self::Field(id) => write!(formatter, "field {}", id.as_str()),
            Self::Value(index) => write!(formatter, "value #{index}"),
            Self::Custom(index) => write!(formatter, "custom #{index}"),
            Self::Section(id) => write!(formatter, "section {}", id.as_str()),
        }
    }
}

/// One problem found in a payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemIssue {
    pub code: IssueCode,
    pub target: IssueTarget,
    /// The limit that was exceeded, for `too-long` (bytes) and `too-many`.
    pub limit: Option<u32>,
}

impl ItemIssue {
    pub(super) fn new(code: IssueCode, target: IssueTarget) -> Self {
        Self {
            code,
            target,
            limit: None,
        }
    }

    pub(super) fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(u32::try_from(limit).unwrap_or(u32::MAX));
        self
    }
}

/// Every problem found, so a form can mark all the wrong fields at once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemIssues(pub Vec<ItemIssue>);

impl fmt::Display for ItemIssues {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid item:")?;
        for issue in &self.0 {
            write!(formatter, " {} ({})", issue.code.as_str(), issue.target)?;
        }
        Ok(())
    }
}

impl Error for ItemIssues {}
