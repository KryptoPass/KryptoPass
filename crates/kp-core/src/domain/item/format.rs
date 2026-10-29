//! Format rules per field type, replaceable and independent of language.
//!
//! Stored values are canonical: dates are ISO (`2026-10-28`), numbers use `.`
//! as the decimal separator, URLs are absolute. Regional input and display
//! (`28/10/2026`, `1,5`) belong to the UI, which converts before saving, so a
//! rule never has to know a locale and a vault reads the same in any language.
//!
//! Rules see values after [`ItemContent::normalize`](super::ItemContent::normalize),
//! so they check the exact text and never trim it themselves.

use std::str::FromStr;
use std::sync::OnceLock;

use super::issue::IssueCode;
use crate::parse_totp_input;

use email_address::EmailAddress;
use kp_template::{DatePrecision, Field, FieldType};
use time::{Date, Month};

/// A rule for the values of one field type. Closures with the same shape work.
pub trait FormatCheck: Send + Sync {
    /// `value` is never empty; empty values are skipped by the caller.
    fn check(&self, field: &Field, value: &str) -> Result<(), IssueCode>;
}

impl<F> FormatCheck for F
where
    F: Fn(&Field, &str) -> Result<(), IssueCode> + Send + Sync,
{
    fn check(&self, field: &Field, value: &str) -> Result<(), IssueCode> {
        self(field, value)
    }
}

/// The rules in force, one per field type. Types without a rule accept any text.
pub struct FormatRules {
    rules: Vec<(FieldType, Box<dyn FormatCheck>)>,
}

impl FormatRules {
    /// The rules shipped with the application.
    pub fn standard() -> Self {
        Self { rules: Vec::new() }
            .with(FieldType::Url, check_url)
            .with(FieldType::Email, check_email)
            .with(FieldType::Phone, check_phone)
            .with(FieldType::Number, check_number)
            .with(FieldType::Date, check_date)
            .with(FieldType::Totp, check_totp)
            .with(FieldType::CardNumber, check_card_number)
            .with(FieldType::Digits, check_digits)
            .with(FieldType::Choice, check_choice)
            .with(FieldType::Boolean, check_boolean)
            .with(FieldType::SshPrivateKey, check_ssh_private_key)
            .with(FieldType::SshPublicKey, check_ssh_public_key)
            .with(FieldType::SshFingerprint, check_ssh_fingerprint)
    }

    /// Shared instance of [`FormatRules::standard`].
    pub(super) fn shared() -> &'static Self {
        static RULES: OnceLock<FormatRules> = OnceLock::new();
        RULES.get_or_init(Self::standard)
    }

    /// Sets the rule for `field_type`, replacing any earlier one.
    #[must_use]
    pub fn with(mut self, field_type: FieldType, check: impl FormatCheck + 'static) -> Self {
        self.rules.retain(|(existing, _)| *existing != field_type);
        self.rules.push((field_type, Box::new(check)));
        self
    }

    pub(super) fn check(&self, field: &Field, value: &str) -> Result<(), IssueCode> {
        match self
            .rules
            .iter()
            .find(|(kind, _)| *kind == field.field_type)
        {
            Some((_, rule)) => rule.check(field, value),
            None => Ok(()),
        }
    }
}

/// Schemes that run code or reach local data when opened, never a site.
const BLOCKED_URL_SCHEMES: [&str; 5] = ["javascript", "vbscript", "data", "file", "blob"];

/// Any absolute URL except the blocked schemes, so `ssh://` and app links
/// still work.
fn check_url(_: &Field, value: &str) -> Result<(), IssueCode> {
    match url::Url::parse(value) {
        Ok(url) if !BLOCKED_URL_SCHEMES.contains(&url.scheme()) => Ok(()),
        _ => Err(IssueCode::INVALID_URL),
    }
}

fn check_email(_: &Field, value: &str) -> Result<(), IssueCode> {
    if EmailAddress::is_valid(value) {
        Ok(())
    } else {
        Err(IssueCode::INVALID_EMAIL)
    }
}

/// Digits with the separators people type; a `+` only at the start. It does not
/// judge numbering plans, so extensions and local formats are accepted.
fn check_phone(_: &Field, value: &str) -> Result<(), IssueCode> {
    let allowed = value
        .char_indices()
        .all(|(index, c)| c.is_ascii_digit() || " -().".contains(c) || (c == '+' && index == 0));
    let digits = value.chars().filter(char::is_ascii_digit).count();
    if allowed && (3..=20).contains(&digits) {
        Ok(())
    } else {
        Err(IssueCode::INVALID_PHONE)
    }
}

/// Plain decimal: optional `-`, digits, optional `.` and digits.
fn check_number(_: &Field, value: &str) -> Result<(), IssueCode> {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, "0"));
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if value.len() <= 64 && digits(whole) && digits(fraction) {
        Ok(())
    } else {
        Err(IssueCode::INVALID_NUMBER)
    }
}

/// `YYYY-MM-DD`, or `YYYY-MM` for month precision, naming a real calendar date.
fn check_date(field: &Field, value: &str) -> Result<(), IssueCode> {
    let mut parts = value.split('-');
    let parsed = (|| {
        let year: u16 = fixed_digits(parts.next()?, 4)?;
        let month = Month::try_from(fixed_digits::<u8>(parts.next()?, 2)?).ok()?;
        let day = match field.precision.unwrap_or_default() {
            DatePrecision::Month => 1,
            DatePrecision::Day => fixed_digits(parts.next()?, 2)?,
        };
        parts.next().is_none().then_some(())?;
        Date::from_calendar_date(i32::from(year), month, day).ok()
    })();
    parsed.map(drop).ok_or(IssueCode::INVALID_DATE)
}

/// Exactly `len` ASCII digits. `str::parse` alone would also accept a sign.
fn fixed_digits<T: FromStr>(part: &str, len: usize) -> Option<T> {
    let digits = part.len() == len && part.bytes().all(|byte| byte.is_ascii_digit());
    digits.then(|| part.parse().ok()).flatten()
}

/// The stored form is an `otpauth://` URI; a bare secret is converted to one
/// by normalization before it gets here.
fn check_totp(_: &Field, value: &str) -> Result<(), IssueCode> {
    let is_uri = value
        .get(.."otpauth://".len())
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("otpauth://"));
    match parse_totp_input(value) {
        Ok(Some(_)) if is_uri => Ok(()),
        _ => Err(IssueCode::INVALID_TOTP),
    }
}

fn all_digits(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_digit())
}

/// The stored form has no separators; normalization removes them.
fn check_card_number(_: &Field, value: &str) -> Result<(), IssueCode> {
    if all_digits(value) && (12..=19).contains(&value.len()) {
        Ok(())
    } else {
        Err(IssueCode::INVALID_CARD_NUMBER)
    }
}

fn check_digits(field: &Field, value: &str) -> Result<(), IssueCode> {
    let fits = field.digits.is_none_or(|count| {
        (usize::from(count.min)..=usize::from(count.max)).contains(&value.len())
    });
    if all_digits(value) && fits {
        Ok(())
    } else {
        Err(IssueCode::INVALID_DIGITS)
    }
}

fn check_choice(field: &Field, value: &str) -> Result<(), IssueCode> {
    if field.choices.iter().any(|choice| choice == value) {
        Ok(())
    } else {
        Err(IssueCode::INVALID_CHOICE)
    }
}

fn check_boolean(_: &Field, value: &str) -> Result<(), IssueCode> {
    match value {
        "true" | "false" => Ok(()),
        _ => Err(IssueCode::INVALID_BOOLEAN),
    }
}

/// Parses the key file; an encrypted key is accepted without its passphrase.
fn check_ssh_private_key(_: &Field, value: &str) -> Result<(), IssueCode> {
    ssh_key::PrivateKey::from_openssh(value)
        .map(drop)
        .map_err(|_| IssueCode::INVALID_SSH_KEY)
}

fn check_ssh_public_key(_: &Field, value: &str) -> Result<(), IssueCode> {
    ssh_key::PublicKey::from_openssh(value)
        .map(drop)
        .map_err(|_| IssueCode::INVALID_SSH_KEY)
}

fn check_ssh_fingerprint(_: &Field, value: &str) -> Result<(), IssueCode> {
    match value.parse::<ssh_key::Fingerprint>() {
        Ok(fingerprint) if fingerprint.algorithm() == ssh_key::HashAlg::Sha256 => Ok(()),
        _ => Err(IssueCode::INVALID_SSH_KEY),
    }
}
