use std::collections::HashMap;

use regex::Regex;
use url::Url;

use crate::config::{
    AuthPathRuleDef, CaseMode, Config, ConfigError, PathSegmentRuleDef, QueryAction, QueryRuleDef,
    ScoreMatchDef, ScoreRuleDef, Settings, StringMatchDef, TrailingSlashRuleDef, ValueMatchDef,
};

pub(crate) struct CompiledConfig {
    pub(crate) settings: Settings,
    pub(crate) query_rules: Vec<CompiledQueryRule>,
    pub(crate) auth_paths: Vec<CompiledPathRule>,
    pub(crate) segment_rules: Vec<CompiledSegmentRule>,
    pub(crate) trailing_slashes: Vec<CompiledPathRule>,
    pub(crate) score_rules: Vec<CompiledScoreRule>,
    pub(crate) matrix_regex: Option<Regex>,
    pub(crate) multi_frame_candidate: Option<Regex>,
    pub(crate) multi_frame_group: String,
}

impl CompiledConfig {
    pub(crate) fn compile(config: &Config) -> Result<Self, ConfigError> {
        let lowercase_keys = config.settings.query.key_case == CaseMode::Lower;

        let mut query_rules = config
            .query_rules
            .iter()
            .map(|rule| CompiledQueryRule::compile(rule, lowercase_keys))
            .collect::<Result<Vec<_>, _>>()?;
        query_rules.sort_by(|left, right| right.priority.cmp(&left.priority));

        let auth_paths = config
            .auth_path_rules
            .iter()
            .map(CompiledPathRule::compile_auth)
            .collect::<Result<Vec<_>, _>>()?;
        let segment_rules = config
            .path_segment_rules
            .iter()
            .map(CompiledSegmentRule::compile)
            .collect::<Result<Vec<_>, _>>()?;
        let trailing_slashes = config
            .trailing_slash_rules
            .iter()
            .map(CompiledPathRule::compile_trailing)
            .collect::<Result<Vec<_>, _>>()?;
        let score_rules = config
            .score_rules
            .iter()
            .map(CompiledScoreRule::compile)
            .collect::<Result<Vec<_>, _>>()?;

        let matrix_regex = if config.settings.path.strip_matrix
            && !config.settings.path.matrix_pattern.is_empty()
        {
            Some(compile_regex(&config.settings.path.matrix_pattern)?)
        } else {
            None
        };

        let multi_frame_candidate = if config.settings.multi_frame.enabled
            && !config.settings.multi_frame.candidate_pattern.is_empty()
        {
            Some(compile_regex(
                &config.settings.multi_frame.candidate_pattern,
            )?)
        } else {
            None
        };

        Ok(Self {
            settings: config.settings.clone(),
            query_rules,
            auth_paths,
            segment_rules,
            trailing_slashes,
            score_rules,
            matrix_regex,
            multi_frame_candidate,
            multi_frame_group: config.settings.multi_frame.candidate_group.clone(),
        })
    }
}

fn compile_regex(pattern: &str) -> Result<Regex, ConfigError> {
    Regex::new(pattern).map_err(|source| ConfigError::Regex {
        pattern: pattern.to_owned(),
        source,
    })
}

pub(crate) enum CompiledStringMatch {
    Exact(Vec<String>),
    Prefix(Vec<String>),
    Suffix(Vec<String>),
    Contains(Vec<String>),
    Regex(Regex),
    Any,
}

impl CompiledStringMatch {
    pub(crate) fn compile(
        definition: &StringMatchDef,
        lowercase_literals: bool,
    ) -> Result<Self, ConfigError> {
        let literals = |values: &[String]| {
            values
                .iter()
                .map(|value| {
                    if lowercase_literals {
                        value.to_ascii_lowercase()
                    } else {
                        value.clone()
                    }
                })
                .collect::<Vec<_>>()
        };
        Ok(match definition {
            StringMatchDef::Exact { values } => Self::Exact(literals(values)),
            StringMatchDef::Prefix { values } => Self::Prefix(literals(values)),
            StringMatchDef::Suffix { values } => Self::Suffix(literals(values)),
            StringMatchDef::Contains { values } => Self::Contains(literals(values)),
            StringMatchDef::Regex { pattern } => Self::Regex(compile_regex(pattern)?),
            StringMatchDef::Any => Self::Any,
        })
    }

    pub(crate) fn matches(&self, value: &str) -> bool {
        match self {
            Self::Exact(values) => values.iter().any(|candidate| candidate == value),
            Self::Prefix(values) => values.iter().any(|candidate| value.starts_with(candidate)),
            Self::Suffix(values) => values.iter().any(|candidate| value.ends_with(candidate)),
            Self::Contains(values) => values.iter().any(|candidate| value.contains(candidate)),
            Self::Regex(regex) => regex.is_match(value),
            Self::Any => true,
        }
    }
}

pub(crate) enum CompiledValueMatch {
    Empty,
    Length {
        min: Option<usize>,
        max: Option<usize>,
    },
    Prefix {
        values: Vec<String>,
        case_insensitive: bool,
    },
    Contains {
        values: Vec<String>,
        case_insensitive: bool,
    },
    Regex(Regex),
    Entropy {
        min_length: usize,
        min_bits: f64,
        require_alpha: bool,
        require_digit: bool,
        require_mixed_case: bool,
    },
    CharacterClasses {
        charset: Regex,
        min_length: usize,
        min_digits: usize,
        min_uppercase: usize,
        min_lowercase: usize,
        min_transitions: usize,
    },
    Any(Vec<Self>),
    All(Vec<Self>),
}

impl CompiledValueMatch {
    pub(crate) fn compile(definition: &ValueMatchDef) -> Result<Self, ConfigError> {
        Ok(match definition {
            ValueMatchDef::Empty => Self::Empty,
            ValueMatchDef::Length { min, max } => Self::Length {
                min: *min,
                max: *max,
            },
            ValueMatchDef::Prefix {
                values,
                case_insensitive,
            } => Self::Prefix {
                values: normalize_literals(values, *case_insensitive),
                case_insensitive: *case_insensitive,
            },
            ValueMatchDef::Contains {
                values,
                case_insensitive,
            } => Self::Contains {
                values: normalize_literals(values, *case_insensitive),
                case_insensitive: *case_insensitive,
            },
            ValueMatchDef::Regex { pattern } => Self::Regex(compile_regex(pattern)?),
            ValueMatchDef::Entropy {
                min_length,
                min_bits,
                require_alpha,
                require_digit,
                require_mixed_case,
            } => Self::Entropy {
                min_length: *min_length,
                min_bits: *min_bits,
                require_alpha: *require_alpha,
                require_digit: *require_digit,
                require_mixed_case: *require_mixed_case,
            },
            ValueMatchDef::CharacterClasses {
                min_length,
                charset_pattern,
                min_digits,
                min_uppercase,
                min_lowercase,
                min_transitions,
            } => Self::CharacterClasses {
                charset: compile_regex(charset_pattern)?,
                min_length: *min_length,
                min_digits: *min_digits,
                min_uppercase: *min_uppercase,
                min_lowercase: *min_lowercase,
                min_transitions: *min_transitions,
            },
            ValueMatchDef::Any { matches } => Self::Any(
                matches
                    .iter()
                    .map(Self::compile)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            ValueMatchDef::All { matches } => Self::All(
                matches
                    .iter()
                    .map(Self::compile)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        })
    }

    pub(crate) fn matches(&self, value: &str) -> bool {
        match self {
            Self::Empty => value.is_empty(),
            Self::Length { min, max } => {
                let length = value.chars().count();
                min.is_none_or(|min| length >= min) && max.is_none_or(|max| length <= max)
            }
            Self::Prefix {
                values,
                case_insensitive,
            } => {
                let value = normalize_value(value, *case_insensitive);
                values.iter().any(|candidate| value.starts_with(candidate))
            }
            Self::Contains {
                values,
                case_insensitive,
            } => {
                let value = normalize_value(value, *case_insensitive);
                values.iter().any(|candidate| value.contains(candidate))
            }
            Self::Regex(regex) => regex.is_match(value),
            Self::Entropy {
                min_length,
                min_bits,
                require_alpha,
                require_digit,
                require_mixed_case,
            } => {
                if value.chars().count() < *min_length {
                    return false;
                }
                let alpha = value.chars().any(|ch| ch.is_ascii_alphabetic());
                let digit = value.chars().any(|ch| ch.is_ascii_digit());
                let lowercase = value.chars().any(|ch| ch.is_ascii_lowercase());
                let uppercase = value.chars().any(|ch| ch.is_ascii_uppercase());
                (!*require_alpha || alpha)
                    && (!*require_digit || digit)
                    && (!*require_mixed_case || lowercase && uppercase)
                    && shannon_entropy(value) >= *min_bits
            }
            Self::CharacterClasses {
                charset,
                min_length,
                min_digits,
                min_uppercase,
                min_lowercase,
                min_transitions,
            } => {
                if value.chars().count() < *min_length || !charset.is_match(value) {
                    return false;
                }
                let mut digits = 0;
                let mut uppercase = 0;
                let mut lowercase = 0;
                let mut transitions = 0;
                let mut previous = None;
                for ch in value.chars() {
                    let kind = if ch.is_ascii_digit() {
                        digits += 1;
                        0
                    } else if ch.is_ascii_uppercase() {
                        uppercase += 1;
                        1
                    } else {
                        if ch.is_ascii_lowercase() {
                            lowercase += 1;
                        }
                        2
                    };
                    if previous.is_some_and(|previous| previous != kind) {
                        transitions += 1;
                    }
                    previous = Some(kind);
                }
                digits >= *min_digits
                    && uppercase >= *min_uppercase
                    && lowercase >= *min_lowercase
                    && transitions >= *min_transitions
            }
            Self::Any(matches) => matches.iter().any(|matcher| matcher.matches(value)),
            Self::All(matches) => matches.iter().all(|matcher| matcher.matches(value)),
        }
    }
}

fn normalize_literals(values: &[String], case_insensitive: bool) -> Vec<String> {
    values
        .iter()
        .map(|value| normalize_single(value, case_insensitive))
        .collect()
}

fn normalize_single(value: &str, case_insensitive: bool) -> String {
    if case_insensitive {
        value.to_ascii_lowercase()
    } else {
        value.to_owned()
    }
}

fn normalize_value<'a>(value: &'a str, case_insensitive: bool) -> std::borrow::Cow<'a, str> {
    if case_insensitive {
        std::borrow::Cow::Owned(value.to_ascii_lowercase())
    } else {
        std::borrow::Cow::Borrowed(value)
    }
}

fn shannon_entropy(value: &str) -> f64 {
    let mut frequencies: HashMap<char, usize> = HashMap::new();
    let mut length = 0usize;
    for ch in value.chars() {
        *frequencies.entry(ch).or_insert(0) += 1;
        length += 1;
    }
    if length == 0 {
        return 0.0;
    }
    let length = length as f64;
    frequencies.values().fold(0.0, |entropy, count| {
        let probability = *count as f64 / length;
        entropy - probability * probability.log2()
    })
}

pub(crate) struct CompiledQueryRule {
    pub(crate) id: String,
    pub(crate) class: String,
    pub(crate) key: CompiledStringMatch,
    pub(crate) value: Option<CompiledValueMatch>,
    pub(crate) path: Option<CompiledStringMatch>,
    pub(crate) action: QueryAction,
    pub(crate) priority: i32,
}

impl CompiledQueryRule {
    fn compile(definition: &QueryRuleDef, lowercase_keys: bool) -> Result<Self, ConfigError> {
        Ok(Self {
            id: definition.id.clone(),
            class: definition.class.clone(),
            key: CompiledStringMatch::compile(&definition.key, lowercase_keys)?,
            value: definition
                .value
                .as_ref()
                .map(CompiledValueMatch::compile)
                .transpose()?,
            path: definition
                .path
                .as_ref()
                .map(|path| CompiledStringMatch::compile(path, false))
                .transpose()?,
            action: definition.action,
            priority: definition.priority,
        })
    }

    pub(crate) fn matches(&self, key: &str, value: &str, path: &str) -> bool {
        self.key.matches(key)
            && self
                .path
                .as_ref()
                .is_none_or(|path_match| path_match.matches(path))
            && self
                .value
                .as_ref()
                .is_none_or(|value_match| value_match.matches(value))
    }
}

pub(crate) struct CompiledPathRule {
    pub(crate) id: String,
    pub(crate) class: String,
    pub(crate) path: CompiledStringMatch,
}

impl CompiledPathRule {
    fn compile_auth(definition: &AuthPathRuleDef) -> Result<Self, ConfigError> {
        Ok(Self {
            id: definition.id.clone(),
            class: definition.class.clone(),
            path: CompiledStringMatch::compile(&definition.path, false)?,
        })
    }

    fn compile_trailing(definition: &TrailingSlashRuleDef) -> Result<Self, ConfigError> {
        Ok(Self {
            id: definition.id.clone(),
            class: definition.class.clone(),
            path: CompiledStringMatch::compile(&definition.path, false)?,
        })
    }

    pub(crate) fn matches(&self, path: &str) -> bool {
        self.path.matches(path)
    }
}

pub(crate) struct CompiledSegmentRule {
    pub(crate) id: String,
    pub(crate) class: String,
    pub(crate) matcher: CompiledValueMatch,
}

impl CompiledSegmentRule {
    fn compile(definition: &PathSegmentRuleDef) -> Result<Self, ConfigError> {
        Ok(Self {
            id: definition.id.clone(),
            class: definition.class.clone(),
            matcher: CompiledValueMatch::compile(&definition.r#match)?,
        })
    }

    pub(crate) fn matches(&self, segment: &str) -> bool {
        self.matcher.matches(segment)
    }
}

pub(crate) struct CompiledScoreRule {
    pub(crate) id: String,
    pub(crate) score: i32,
    pub(crate) matcher: CompiledScoreMatch,
}

impl CompiledScoreRule {
    fn compile(definition: &ScoreRuleDef) -> Result<Self, ConfigError> {
        Ok(Self {
            id: definition.id.clone(),
            score: definition.score,
            matcher: CompiledScoreMatch::compile(&definition.r#match)?,
        })
    }

    pub(crate) fn matches(&self, url: &Url) -> bool {
        self.matcher.matches(url)
    }
}

pub(crate) enum CompiledScoreMatch {
    HostPathRegex(Regex),
    PathRegex(Regex),
    QueryKeyPresent {
        values: Vec<String>,
        case_insensitive: bool,
    },
    QueryValueEquals {
        key: String,
        value: String,
        case_insensitive: bool,
    },
    QueryLength {
        greater_than: usize,
    },
    Scheme(String),
}

impl CompiledScoreMatch {
    fn compile(definition: &ScoreMatchDef) -> Result<Self, ConfigError> {
        Ok(match definition {
            ScoreMatchDef::HostPathRegex { pattern } => {
                Self::HostPathRegex(compile_regex(pattern)?)
            }
            ScoreMatchDef::PathRegex { pattern } => Self::PathRegex(compile_regex(pattern)?),
            ScoreMatchDef::QueryKeyPresent {
                values,
                case_insensitive,
            } => Self::QueryKeyPresent {
                values: normalize_literals(values, *case_insensitive),
                case_insensitive: *case_insensitive,
            },
            ScoreMatchDef::QueryValueEquals {
                key,
                value,
                case_insensitive,
            } => Self::QueryValueEquals {
                key: normalize_single(key, *case_insensitive),
                value: normalize_single(value, *case_insensitive),
                case_insensitive: *case_insensitive,
            },
            ScoreMatchDef::QueryLength { greater_than } => Self::QueryLength {
                greater_than: *greater_than,
            },
            ScoreMatchDef::Scheme { value } => Self::Scheme(value.to_ascii_lowercase()),
        })
    }

    fn matches(&self, url: &Url) -> bool {
        match self {
            Self::HostPathRegex(regex) => {
                let host_path = format!("{}{}", url.host_str().unwrap_or_default(), url.path());
                regex.is_match(&host_path)
            }
            Self::PathRegex(regex) => regex.is_match(url.path()),
            Self::QueryKeyPresent {
                values,
                case_insensitive,
            } => url.query_pairs().any(|(key, _)| {
                let key = normalize_value(&key, *case_insensitive);
                values
                    .iter()
                    .any(|candidate| candidate.as_str() == key.as_ref())
            }),
            Self::QueryValueEquals {
                key,
                value,
                case_insensitive,
            } => url.query_pairs().any(|(candidate_key, candidate_value)| {
                let candidate_key = normalize_value(&candidate_key, *case_insensitive);
                let candidate_value = normalize_value(&candidate_value, *case_insensitive);
                candidate_key.as_ref() == key.as_str() && candidate_value.as_ref() == value.as_str()
            }),
            Self::QueryLength { greater_than } => url.query().map_or(0, str::len) > *greater_than,
            Self::Scheme(value) => url.scheme().eq_ignore_ascii_case(value),
        }
    }
}
