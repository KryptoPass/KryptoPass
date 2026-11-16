use std::collections::HashSet;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecisionError {
    #[error("cannot parse decision policy: {0}")]
    Config(#[from] toml::de::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    generic_usernames: Vec<String>,
    #[serde(default)]
    reset_path_keywords: Vec<String>,
    #[serde(default)]
    typo: TypoConfig,
    #[serde(default)]
    rotation: RotationConfig,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TypoConfig {
    max_logins: usize,
    dominant_logins: usize,
    max_password_distance: usize,
}

impl Default for TypoConfig {
    fn default() -> Self {
        Self {
            max_logins: 1,
            dominant_logins: 3,
            max_password_distance: 1,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RotationConfig {
    min_time_gap_days: u64,
    max_password_distance: usize,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            min_time_gap_days: 7,
            max_password_distance: 2,
        }
    }
}

pub struct DecisionPolicy {
    generic_usernames: HashSet<String>,
    reset_path_keywords: Vec<String>,
    typo: TypoConfig,
    rotation: RotationConfig,
}

impl DecisionPolicy {
    pub fn embedded() -> Result<Self, DecisionError> {
        Self::from_toml(include_str!("../config/decisions.toml"))
    }

    pub fn from_toml(source: &str) -> Result<Self, DecisionError> {
        let raw: RawConfig = toml::from_str(source)?;
        Ok(Self {
            generic_usernames: raw
                .generic_usernames
                .into_iter()
                .map(|value| value.to_ascii_lowercase())
                .collect(),
            reset_path_keywords: raw
                .reset_path_keywords
                .into_iter()
                .map(|value| value.to_ascii_lowercase())
                .collect(),
            typo: raw.typo,
            rotation: raw.rotation,
        })
    }

    pub fn is_generic(&self, value: &str) -> bool {
        self.generic_usernames.contains(value)
    }

    pub fn is_reset_path(&self, path: &str) -> bool {
        let path = path.to_ascii_lowercase();
        self.reset_path_keywords
            .iter()
            .any(|keyword| path.contains(keyword))
    }

    pub fn typo_max_logins(&self) -> usize {
        self.typo.max_logins
    }

    pub fn typo_dominant_logins(&self) -> usize {
        self.typo.dominant_logins
    }

    pub fn typo_max_password_distance(&self) -> usize {
        self.typo.max_password_distance
    }

    pub fn rotation_min_time_gap_ms(&self) -> u64 {
        self.rotation.min_time_gap_days.saturating_mul(86_400_000)
    }

    pub fn rotation_max_password_distance(&self) -> usize {
        self.rotation.max_password_distance
    }
}
