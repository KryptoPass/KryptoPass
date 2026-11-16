use std::collections::BTreeMap;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseClass {
    AllCaps,
    Mixed,
    Lower,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsernameContext {
    Internal,
    Public,
    Email,
}

#[derive(Debug, Error)]
pub enum UsernameError {
    #[error("cannot parse username policy: {0}")]
    Config(#[from] toml::de::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    shift_typo: ShiftTypo,
    preferences: Preferences,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShiftTypo {
    uppercase_ratio: f64,
    max_lowercase: usize,
}

impl Default for ShiftTypo {
    fn default() -> Self {
        Self {
            uppercase_ratio: 0.70,
            max_lowercase: 1,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    internal: Vec<CaseClass>,
    public: Vec<CaseClass>,
    email: Vec<CaseClass>,
}

pub struct UsernamePolicy {
    shift_typo: ShiftTypo,
    preferences: Preferences,
}

impl UsernamePolicy {
    pub fn embedded() -> Result<Self, UsernameError> {
        Self::from_toml(include_str!("../config/username.toml"))
    }

    pub fn from_toml(source: &str) -> Result<Self, UsernameError> {
        let raw: RawConfig = toml::from_str(source)?;
        Ok(Self {
            shift_typo: raw.shift_typo,
            preferences: raw.preferences,
        })
    }

    /// Picks the original variant to use for autofill. Frequencies and context
    /// decide; the canonical key itself is never rewritten.
    pub fn primary<'a>(
        &self,
        variants: &'a BTreeMap<String, usize>,
        context: UsernameContext,
    ) -> Option<&'a str> {
        let candidates: Vec<(&str, usize)> = variants
            .iter()
            .filter(|(value, _)| !self.is_shift_typo(value))
            .map(|(value, count)| (value.as_str(), *count))
            .collect();
        let candidates = if candidates.is_empty() {
            variants
                .iter()
                .map(|(value, count)| (value.as_str(), *count))
                .collect()
        } else {
            candidates
        };
        candidates
            .into_iter()
            .max_by(|(left_value, left_count), (right_value, right_count)| {
                left_count
                    .cmp(right_count)
                    .then_with(|| {
                        self.preference_rank(context, right_value)
                            .cmp(&self.preference_rank(context, left_value))
                    })
                    .then_with(|| right_value.cmp(left_value))
            })
            .map(|(value, _)| value)
    }

    fn preference_rank(&self, context: UsernameContext, value: &str) -> usize {
        let order = match context {
            UsernameContext::Internal => &self.preferences.internal,
            UsernameContext::Public => &self.preferences.public,
            UsernameContext::Email => &self.preferences.email,
        };
        let class = CaseClass::of(value);
        order
            .iter()
            .position(|candidate| *candidate == class)
            .unwrap_or(order.len())
    }

    pub fn is_shift_typo(&self, value: &str) -> bool {
        let mut uppercase = 0usize;
        let mut lowercase = 0usize;
        for character in value.chars() {
            if character.is_uppercase() {
                uppercase += 1;
            } else if character.is_lowercase() {
                lowercase += 1;
            }
        }
        let letters = uppercase + lowercase;
        letters > 0
            && lowercase > 0
            && lowercase <= self.shift_typo.max_lowercase
            && uppercase as f64 / letters as f64 > self.shift_typo.uppercase_ratio
    }
}

impl CaseClass {
    fn of(value: &str) -> Self {
        let mut uppercase = false;
        let mut lowercase = false;
        for character in value.chars() {
            if character.is_uppercase() {
                uppercase = true;
            } else if character.is_lowercase() {
                lowercase = true;
            }
        }
        match (uppercase, lowercase) {
            (true, true) => Self::Mixed,
            (true, false) => Self::AllCaps,
            (false, true) => Self::Lower,
            (false, false) => Self::Neutral,
        }
    }
}
