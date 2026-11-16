use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

include!(concat!(env!("OUT_DIR"), "/embedded_configs.rs"));

/// Errors raised while reading or compiling the rule files.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read configuration: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot parse configuration file {file}: {source}")]
    Parse {
        file: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid regular expression `{pattern}`: {source}")]
    Regex {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    #[error("no TOML files found in {0}")]
    NoFiles(PathBuf),
}

/// A merged set of settings and rules.
///
/// Parse order matters only for rule priority and for scalar settings: later
/// files override settings they declare, while rule arrays are appended.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub(crate) settings: Settings,
    pub(crate) query_rules: Vec<QueryRuleDef>,
    pub(crate) auth_path_rules: Vec<AuthPathRuleDef>,
    pub(crate) path_segment_rules: Vec<PathSegmentRuleDef>,
    pub(crate) trailing_slash_rules: Vec<TrailingSlashRuleDef>,
    pub(crate) score_rules: Vec<ScoreRuleDef>,
}

impl Config {
    /// The configuration embedded from this crate's `config/` directory.
    pub fn embedded() -> Result<Self, ConfigError> {
        let mut config = Self::default();
        for (file, source) in EMBEDDED_CONFIGS {
            config.extend_from_source(file, source)?;
        }
        Ok(config)
    }

    /// Parse a single TOML fragment. Missing sections stay at their neutral
    /// defaults.
    pub fn from_toml_str(source: &str) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        config.extend_from_source("<memory>", source)?;
        Ok(config)
    }

    /// Load every `*.toml` file under `root`, recursively.
    pub fn from_dir(root: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        config.extend_from_dir(root)?;
        Ok(config)
    }

    /// Extend the embedded defaults with every `*.toml` file under `root`.
    pub fn embedded_with_dir(root: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let mut config = Self::embedded()?;
        config.extend_from_dir(root)?;
        Ok(config)
    }

    /// Merge rule files into this configuration.
    pub fn extend_from_dir(&mut self, root: impl AsRef<Path>) -> Result<(), ConfigError> {
        let root = root.as_ref();
        let files = toml_files(root)?;
        if files.is_empty() {
            return Err(ConfigError::NoFiles(root.to_path_buf()));
        }
        for path in files {
            let source = fs::read_to_string(&path)?;
            self.extend_from_source(&path.display().to_string(), &source)?;
        }
        Ok(())
    }

    /// Merge one TOML fragment into this configuration.
    pub fn extend_from_source(&mut self, file: &str, source: &str) -> Result<(), ConfigError> {
        let raw: RawConfig = toml::from_str(source).map_err(|source| ConfigError::Parse {
            file: file.to_owned(),
            source,
        })?;
        raw.merge_into(self);
        Ok(())
    }
}

fn toml_files(root: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    fn visit(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), ConfigError> {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(&path, files)?;
            } else if path
                .extension()
                .is_some_and(|extension| extension == "toml")
            {
                files.push(path);
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(root, &mut files)?;
    files.sort();
    Ok(files)
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    settings: Option<RawSettings>,
    #[serde(default, rename = "query_rule")]
    query_rules: Vec<QueryRuleDef>,
    #[serde(default, rename = "auth_path_rule")]
    auth_path_rules: Vec<AuthPathRuleDef>,
    #[serde(default, rename = "path_segment_rule")]
    path_segment_rules: Vec<PathSegmentRuleDef>,
    #[serde(default, rename = "trailing_slash_rule")]
    trailing_slash_rules: Vec<TrailingSlashRuleDef>,
    #[serde(default, rename = "score_rule")]
    score_rules: Vec<ScoreRuleDef>,
}

impl RawConfig {
    fn merge_into(self, config: &mut Config) {
        if let Some(settings) = self.settings {
            config.settings.apply(settings);
        }
        config.query_rules.extend(self.query_rules);
        config.auth_path_rules.extend(self.auth_path_rules);
        config.path_segment_rules.extend(self.path_segment_rules);
        config
            .trailing_slash_rules
            .extend(self.trailing_slash_rules);
        config.score_rules.extend(self.score_rules);
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Settings {
    pub(crate) phases: Vec<Phase>,
    pub(crate) strip_dangling_query: bool,
    pub(crate) query: QuerySettings,
    pub(crate) path: PathSettings,
    pub(crate) fragment: FragmentSettings,
    pub(crate) multi_frame: MultiFrameSettings,
}

impl Settings {
    fn apply(&mut self, patch: RawSettings) {
        if let Some(phases) = patch.phases {
            self.phases = phases;
        }
        if let Some(value) = patch.strip_dangling_query {
            self.strip_dangling_query = value;
        }
        if let Some(query) = patch.query {
            self.query.apply(query);
        }
        if let Some(path) = patch.path {
            self.path.apply(path);
        }
        if let Some(fragment) = patch.fragment {
            self.fragment.apply(fragment);
        }
        if let Some(multi_frame) = patch.multi_frame {
            self.multi_frame.apply(multi_frame);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    MultiFrame,
    MatrixParams,
    PathSegments,
    Fragment,
    AuthPath,
    Query,
    TrailingSlash,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawSettings {
    phases: Option<Vec<Phase>>,
    strip_dangling_query: Option<bool>,
    query: Option<RawQuerySettings>,
    path: Option<RawPathSettings>,
    fragment: Option<RawFragmentSettings>,
    multi_frame: Option<RawMultiFrameSettings>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct QuerySettings {
    pub(crate) key_case: CaseMode,
    pub(crate) drop_valueless: bool,
    pub(crate) decode_slashes: bool,
    pub(crate) trace_kept: bool,
}

impl QuerySettings {
    fn apply(&mut self, patch: RawQuerySettings) {
        if let Some(value) = patch.key_case {
            self.key_case = value;
        }
        if let Some(value) = patch.drop_valueless {
            self.drop_valueless = value;
        }
        if let Some(value) = patch.decode_slashes {
            self.decode_slashes = value;
        }
        if let Some(value) = patch.trace_kept {
            self.trace_kept = value;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CaseMode {
    #[default]
    Preserve,
    Lower,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawQuerySettings {
    key_case: Option<CaseMode>,
    drop_valueless: Option<bool>,
    decode_slashes: Option<bool>,
    trace_kept: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PathSettings {
    pub(crate) decode_segments: bool,
    pub(crate) strip_matrix: bool,
    pub(crate) matrix_pattern: String,
}

impl PathSettings {
    fn apply(&mut self, patch: RawPathSettings) {
        if let Some(value) = patch.decode_segments {
            self.decode_segments = value;
        }
        if let Some(value) = patch.strip_matrix {
            self.strip_matrix = value;
        }
        if let Some(value) = patch.matrix_pattern {
            self.matrix_pattern = value;
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawPathSettings {
    decode_segments: Option<bool>,
    strip_matrix: Option<bool>,
    matrix_pattern: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FragmentSettings {
    pub(crate) drop_empty: bool,
    pub(crate) spa_prefixes: Vec<String>,
    pub(crate) auth_values: Vec<String>,
    pub(crate) strip_query: bool,
    pub(crate) drop_root: bool,
    pub(crate) drop_unroutable: bool,
}

impl FragmentSettings {
    fn apply(&mut self, patch: RawFragmentSettings) {
        if let Some(value) = patch.drop_empty {
            self.drop_empty = value;
        }
        if let Some(value) = patch.spa_prefixes {
            self.spa_prefixes = value;
        }
        if let Some(value) = patch.auth_values {
            self.auth_values = value;
        }
        if let Some(value) = patch.strip_query {
            self.strip_query = value;
        }
        if let Some(value) = patch.drop_root {
            self.drop_root = value;
        }
        if let Some(value) = patch.drop_unroutable {
            self.drop_unroutable = value;
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawFragmentSettings {
    drop_empty: Option<bool>,
    spa_prefixes: Option<Vec<String>>,
    auth_values: Option<Vec<String>>,
    strip_query: Option<bool>,
    drop_root: Option<bool>,
    drop_unroutable: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MultiFrameSettings {
    pub(crate) enabled: bool,
    pub(crate) candidate_pattern: String,
    pub(crate) candidate_group: String,
    pub(crate) invalid_score: i32,
}

impl MultiFrameSettings {
    fn apply(&mut self, patch: RawMultiFrameSettings) {
        if let Some(value) = patch.enabled {
            self.enabled = value;
        }
        if let Some(value) = patch.candidate_pattern {
            self.candidate_pattern = value;
        }
        if let Some(value) = patch.candidate_group {
            self.candidate_group = value;
        }
        if let Some(value) = patch.invalid_score {
            self.invalid_score = value;
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawMultiFrameSettings {
    enabled: Option<bool>,
    candidate_pattern: Option<String>,
    candidate_group: Option<String>,
    invalid_score: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryRuleDef {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) class: String,
    pub(crate) key: StringMatchDef,
    #[serde(default)]
    pub(crate) value: Option<ValueMatchDef>,
    #[serde(default)]
    pub(crate) path: Option<StringMatchDef>,
    #[serde(default)]
    pub(crate) action: QueryAction,
    #[serde(default)]
    pub(crate) priority: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QueryAction {
    #[default]
    Drop,
    Keep,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StringMatchDef {
    Exact { values: Vec<String> },
    Prefix { values: Vec<String> },
    Suffix { values: Vec<String> },
    Contains { values: Vec<String> },
    Regex { pattern: String },
    Any,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ValueMatchDef {
    Empty,
    Length {
        #[serde(default)]
        min: Option<usize>,
        #[serde(default)]
        max: Option<usize>,
    },
    Prefix {
        values: Vec<String>,
        #[serde(default)]
        case_insensitive: bool,
    },
    Contains {
        values: Vec<String>,
        #[serde(default)]
        case_insensitive: bool,
    },
    Regex {
        pattern: String,
    },
    Entropy {
        min_length: usize,
        min_bits: f64,
        #[serde(default)]
        require_alpha: bool,
        #[serde(default)]
        require_digit: bool,
        #[serde(default)]
        require_mixed_case: bool,
    },
    CharacterClasses {
        min_length: usize,
        charset_pattern: String,
        #[serde(default)]
        min_digits: usize,
        #[serde(default)]
        min_uppercase: usize,
        #[serde(default)]
        min_lowercase: usize,
        #[serde(default)]
        min_transitions: usize,
    },
    Any {
        matches: Vec<ValueMatchDef>,
    },
    All {
        matches: Vec<ValueMatchDef>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthPathRuleDef {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) class: String,
    pub(crate) path: StringMatchDef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PathSegmentRuleDef {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) class: String,
    pub(crate) r#match: ValueMatchDef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrailingSlashRuleDef {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) class: String,
    pub(crate) path: StringMatchDef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScoreRuleDef {
    pub(crate) id: String,
    pub(crate) score: i32,
    pub(crate) r#match: ScoreMatchDef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ScoreMatchDef {
    HostPathRegex {
        pattern: String,
    },
    PathRegex {
        pattern: String,
    },
    QueryKeyPresent {
        values: Vec<String>,
        #[serde(default)]
        case_insensitive: bool,
    },
    QueryValueEquals {
        key: String,
        value: String,
        #[serde(default)]
        case_insensitive: bool,
    },
    QueryLength {
        greater_than: usize,
    },
    Scheme {
        value: String,
    },
}
