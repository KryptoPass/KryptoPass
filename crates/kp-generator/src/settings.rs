//! What to generate: length, character classes and excluded characters.

use std::ops::RangeInclusive;

use kp_template::{CharClass, GeneratorDef, GeneratorRef};
use thiserror::Error;

/// Absolute bounds, whatever a template declares.
const LENGTH_LIMITS: RangeInclusive<usize> = 4..=1024;
const MAX_EXCLUDED_BYTES: usize = 256;
/// Name of the length parameter in the generators file.
const LENGTH_PARAM: &str = "length";

/// Why a value could not be generated. Never carries generated material.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GeneratorError {
    #[error("length is outside the allowed range")]
    Length,
    #[error("the generator declares no length")]
    NoLengthParam,
    #[error("no character class is enabled")]
    NoClasses,
    #[error("an enabled class has no characters left after exclusions")]
    EmptyClass,
    #[error("too many excluded characters")]
    TooManyExclusions,
    #[error("length is too short for the enabled classes")]
    TooShort,
    #[error("secure randomness is unavailable")]
    Randomness,
}

/// Settings for one generation. Built from a template's generator, then
/// adjusted by the person within the generator's bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    length: usize,
    range: RangeInclusive<usize>,
    classes: Vec<CharClass>,
    excluded: String,
}

impl Settings {
    /// Settings with any length in the absolute bounds.
    pub fn new(length: usize, classes: &[CharClass]) -> Result<Self, GeneratorError> {
        let mut settings = Self {
            length: *LENGTH_LIMITS.start(),
            range: LENGTH_LIMITS,
            classes: Vec::new(),
            excluded: String::new(),
        };
        settings.set_length(length)?;
        classes
            .iter()
            .for_each(|class| settings.set_class(*class, true));
        Ok(settings)
    }

    /// The defaults of `generator`, with the overrides of the field that uses
    /// it. The generator's length range stays the limit for later changes.
    pub fn from_template(
        generator: &GeneratorDef,
        field: Option<&GeneratorRef>,
    ) -> Result<Self, GeneratorError> {
        let param = generator
            .params
            .iter()
            .find(|param| param.name == LENGTH_PARAM)
            .ok_or(GeneratorError::NoLengthParam)?;
        let to_usize = |value: u32| usize::try_from(value).map_err(|_| GeneratorError::Length);
        let declared = to_usize(param.min)?..=to_usize(param.max)?;
        let range = (*declared.start()).max(*LENGTH_LIMITS.start())
            ..=(*declared.end()).min(*LENGTH_LIMITS.end());
        let length = field
            .and_then(|field| {
                field
                    .overrides
                    .iter()
                    .find(|over| over.name == LENGTH_PARAM)
            })
            .map_or(param.default, |over| over.value);
        let mut settings = Self {
            length: *range.start(),
            range,
            classes: Vec::new(),
            excluded: String::new(),
        };
        settings.set_length(to_usize(length)?)?;
        for class in generator.classes.iter().filter(|class| class.default) {
            settings.set_class(class.class, true);
        }
        Ok(settings)
    }

    pub fn length(&self) -> usize {
        self.length
    }

    /// The lengths this generator allows.
    pub fn length_range(&self) -> RangeInclusive<usize> {
        self.range.clone()
    }

    pub fn set_length(&mut self, length: usize) -> Result<(), GeneratorError> {
        if !self.range.contains(&length) {
            return Err(GeneratorError::Length);
        }
        self.length = length;
        Ok(())
    }

    pub fn classes(&self) -> &[CharClass] {
        &self.classes
    }

    pub fn set_class(&mut self, class: CharClass, enabled: bool) {
        self.classes.retain(|existing| *existing != class);
        if enabled {
            self.classes.push(class);
        }
    }

    /// Characters never used, such as look-alikes (`0O1l`).
    pub fn set_excluded(&mut self, excluded: &str) -> Result<(), GeneratorError> {
        if excluded.len() > MAX_EXCLUDED_BYTES {
            return Err(GeneratorError::TooManyExclusions);
        }
        excluded.clone_into(&mut self.excluded);
        Ok(())
    }

    /// The enabled classes' characters, minus exclusions, in class order.
    pub(crate) fn alphabets(&self) -> Result<Vec<Vec<u8>>, GeneratorError> {
        if self.classes.is_empty() {
            return Err(GeneratorError::NoClasses);
        }
        let mut ordered = self.classes.clone();
        ordered.sort_by_key(|class| CharClass::ALL.iter().position(|known| known == class));
        ordered
            .iter()
            .map(|class| {
                let alphabet: Vec<u8> = characters(*class)
                    .iter()
                    .copied()
                    .filter(|byte| !self.excluded.contains(char::from(*byte)))
                    .collect();
                if alphabet.is_empty() {
                    Err(GeneratorError::EmptyClass)
                } else {
                    Ok(alphabet)
                }
            })
            .collect()
    }
}

fn characters(class: CharClass) -> &'static [u8] {
    match class {
        CharClass::Lower => b"abcdefghijklmnopqrstuvwxyz",
        CharClass::Upper => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        CharClass::Digits => b"0123456789",
        CharClass::Symbols => b"!@#$%^&*()-_=+[]{};:,.?",
    }
}
