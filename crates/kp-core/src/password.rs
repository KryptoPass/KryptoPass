//! Password generation for the current editor, delegating to `kp-generator`.
//!
//! Kept so the existing UI keeps working; new code uses
//! `kryptopass_generator` with the settings of the template's generator.

use crate::SensitiveText;
use anyhow::Result;
use kp_template::CharClass;
use kryptopass_generator::{Settings, generate};

/// Character classes and length for a generated password.
pub struct PasswordOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub excluded: String,
}

/// Generates a password with at least one character from every selected class.
pub fn generate_password(options: &PasswordOptions) -> Result<SensitiveText> {
    let mut settings = Settings::new(options.length, &[])?;
    for (class, enabled) in [
        (CharClass::Lower, options.lowercase),
        (CharClass::Upper, options.uppercase),
        (CharClass::Digits, options.digits),
        (CharClass::Symbols, options.symbols),
    ] {
        settings.set_class(class, enabled);
    }
    settings.set_excluded(&options.excluded)?;
    Ok(generate(&settings)?)
}
