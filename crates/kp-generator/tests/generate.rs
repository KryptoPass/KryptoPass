#![allow(clippy::unwrap_used, reason = "test fixtures")]

use kp_template::CharClass;
use kryptopass_generator::{GeneratorError, Settings, generate};

#[test]
fn a_password_has_the_length_and_one_character_of_each_class() {
    let settings = Settings::new(
        24,
        &[
            CharClass::Lower,
            CharClass::Upper,
            CharClass::Digits,
            CharClass::Symbols,
        ],
    )
    .unwrap();
    for _ in 0..50 {
        let password = generate(&settings).unwrap();
        let text = password.as_str();
        assert_eq!(text.chars().count(), 24);
        assert!(text.chars().any(|c| c.is_ascii_lowercase()));
        assert!(text.chars().any(|c| c.is_ascii_uppercase()));
        assert!(text.chars().any(|c| c.is_ascii_digit()));
        assert!(text.chars().any(|c| c.is_ascii_punctuation()));
    }
}

#[test]
fn excluded_characters_never_appear() {
    let mut settings = Settings::new(64, &[CharClass::Digits]).unwrap();
    settings.set_excluded("0123").unwrap();
    for _ in 0..50 {
        assert!(
            !generate(&settings)
                .unwrap()
                .as_str()
                .contains(['0', '1', '2', '3'])
        );
    }
}

#[test]
fn impossible_settings_are_refused_instead_of_weakened() {
    let no_classes = Settings::new(16, &[]);
    assert!(matches!(
        no_classes.and_then(|settings| generate(&settings)),
        Err(GeneratorError::NoClasses)
    ));

    let mut no_digits = Settings::new(16, &[CharClass::Digits]).unwrap();
    no_digits.set_excluded("0123456789").unwrap();
    assert!(matches!(
        generate(&no_digits),
        Err(GeneratorError::EmptyClass)
    ));

    let mut range = Settings::new(16, &[CharClass::Lower]).unwrap();
    let too_long = range.length_range().end() + 1;
    assert!(matches!(
        range.set_length(too_long),
        Err(GeneratorError::Length)
    ));
    assert_eq!(range.length(), 16);
}

#[test]
fn two_passwords_differ() {
    let settings = Settings::new(32, &[CharClass::Lower, CharClass::Digits]).unwrap();
    assert_ne!(
        generate(&settings).unwrap().as_str(),
        generate(&settings).unwrap().as_str()
    );
}
