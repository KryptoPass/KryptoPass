//! The password generator.

use super::*;

impl qobject::SecretsController {
    pub fn generate_password(
        mut self: Pin<&mut Self>,
        length: i32,
        lowercase: bool,
        uppercase: bool,
        digits: bool,
        symbols: bool,
        excluded: &QString,
    ) {
        if *self.locked() {
            self.as_mut().fail(crate::i18n::tr_noop(
                "Unlock the vault before generating a password.",
            ));
            return;
        }
        let Ok(length) = usize::try_from(length) else {
            self.as_mut()
                .fail(crate::i18n::tr_noop("The password could not be generated."));
            return;
        };
        match generate_password_value(
            length,
            lowercase,
            uppercase,
            digits,
            symbols,
            &excluded.to_string(),
        ) {
            Ok(password) => {
                let password = QString::from(password.as_str());
                self.password_generated(&password);
            }
            Err(_) => self
                .as_mut()
                .fail(crate::i18n::tr_noop("The password could not be generated.")),
        }
    }
}

pub(super) fn generate_password_value(
    length: usize,
    lowercase: bool,
    uppercase: bool,
    digits: bool,
    symbols: bool,
    excluded: &str,
) -> anyhow::Result<SensitiveText> {
    use kp_template::CharClass;
    use kryptopass_generator::{Settings, generate};
    let classes: Vec<CharClass> = [
        (lowercase, CharClass::Lower),
        (uppercase, CharClass::Upper),
        (digits, CharClass::Digits),
        (symbols, CharClass::Symbols),
    ]
    .into_iter()
    .filter_map(|(enabled, class)| enabled.then_some(class))
    .collect();
    let mut settings = Settings::new(length, &classes)?;
    settings.set_excluded(excluded)?;
    Ok(generate(&settings)?)
}
