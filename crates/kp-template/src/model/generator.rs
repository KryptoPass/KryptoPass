//! Generator definitions and the references templates make to them.

use super::id::GeneratorId;
use super::vocabulary::vocabulary;

vocabulary! {
    /// A class of characters a generator can draw from.
    pub enum CharClass {
        /// Lowercase letters.
        Lower => "lower",
        /// Uppercase letters.
        Upper => "upper",
        /// Decimal digits.
        Digits => "digits",
        /// Punctuation and symbols.
        Symbols => "symbols",
    }
}

/// A numeric parameter of a generator with its allowed range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamDef {
    /// Parameter name, for example `length`.
    pub name: String,
    /// Value used when the field does not override it.
    pub default: u32,
    /// Smallest accepted value.
    pub min: u32,
    /// Largest accepted value.
    pub max: u32,
}

/// A character class available to a generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassDef {
    /// The class.
    pub class: CharClass,
    /// Whether the class is on unless the person turns it off.
    pub default: bool,
}

/// A named generator declared once in the generators file and reused by fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratorDef {
    /// Name that fields use to reference the generator.
    pub id: GeneratorId,
    /// Numeric parameters the generator accepts.
    pub params: Vec<ParamDef>,
    /// Character classes the generator can draw from.
    pub classes: Vec<ClassDef>,
}

/// A field's override of one generator parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratorParam {
    /// Name of the parameter being overridden.
    pub name: String,
    /// Value to use instead of the default.
    pub value: u32,
}

/// A field's use of a generator, with optional parameter overrides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratorRef {
    /// Generator being used.
    pub id: GeneratorId,
    /// Parameters this field sets instead of the generator defaults.
    pub overrides: Vec<GeneratorParam>,
}
