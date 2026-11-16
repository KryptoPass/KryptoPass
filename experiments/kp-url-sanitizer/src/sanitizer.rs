use std::borrow::Cow;

use percent_encoding::percent_decode_str;
use url::Url;

use crate::config::{CaseMode, Config, ConfigError, Phase, QueryAction};
use crate::matcher::CompiledConfig;

/// One rule or pipeline stage that changed the URL.
#[derive(Debug, Clone)]
pub struct Trace {
    /// Pipeline stage, such as `query` or `path_segments`.
    pub stage: String,
    /// Identifier declared by the rule file.
    pub rule: String,
    /// Human-readable explanation of what happened.
    pub detail: String,
}

impl Trace {
    fn new(stage: &str, rule: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            stage: stage.to_owned(),
            rule: rule.into(),
            detail: detail.into(),
        }
    }
}

/// Result of sanitizing one captured URL.
#[derive(Debug, Clone)]
pub struct Sanitized {
    /// Canonical URL after applying the configured phases.
    pub clean: String,
    /// Rules and stages that changed or examined the URL.
    pub trace: Vec<Trace>,
    /// Whether `clean` differs from the trimmed input.
    pub modified: bool,
}

/// A compiled rule set. The sanitizer is immutable and safe to reuse.
pub struct Sanitizer {
    config: CompiledConfig,
}

impl Sanitizer {
    /// Compile a merged configuration.
    pub fn new(config: Config) -> Result<Self, ConfigError> {
        Ok(Self {
            config: CompiledConfig::compile(&config)?,
        })
    }

    /// Compile the rule files embedded in this crate.
    pub fn embedded() -> Result<Self, ConfigError> {
        Self::new(Config::embedded()?)
    }

    /// Compile only the `*.toml` files under `root`.
    pub fn from_dir(root: impl AsRef<std::path::Path>) -> Result<Self, ConfigError> {
        Self::new(Config::from_dir(root)?)
    }

    /// Extend the embedded rules with `*.toml` files under `root`.
    pub fn embedded_with_dir(root: impl AsRef<std::path::Path>) -> Result<Self, ConfigError> {
        Self::new(Config::embedded_with_dir(root)?)
    }

    /// Sanitize a captured URL. Strings that cannot be parsed are returned
    /// trimmed, with a `parse_error` trace entry.
    pub fn sanitize(&self, input: &str) -> Sanitized {
        let raw = input.trim();
        let mut trace = Vec::new();
        let working = if self.config.settings.phases.contains(&Phase::MultiFrame) {
            self.resolve_multi_frame(raw, &mut trace)
        } else {
            raw.to_owned()
        };

        let mut url = match Url::parse(&working) {
            Ok(url) => url,
            Err(error) => {
                trace.push(Trace::new(
                    "parse",
                    "parse_error",
                    format!("cannot parse URL: {error}"),
                ));
                return Sanitized {
                    clean: working,
                    trace,
                    modified: false,
                };
            }
        };

        for phase in &self.config.settings.phases {
            match phase {
                Phase::MultiFrame => {}
                Phase::MatrixParams => self.strip_matrix(&mut url, &mut trace),
                Phase::PathSegments => self.strip_path_segments(&mut url, &mut trace),
                Phase::Fragment => self.normalize_fragment(&mut url, &mut trace),
                Phase::AuthPath => self.clear_auth_path_query(&mut url, &mut trace),
                Phase::Query => self.filter_query(&mut url, &mut trace),
                Phase::TrailingSlash => self.strip_trailing_slash(&mut url, &mut trace),
            }
        }

        let mut clean = url.to_string();
        if self.config.settings.strip_dangling_query && clean.ends_with('?') {
            clean.pop();
        }
        Sanitized {
            modified: clean != raw,
            clean,
            trace,
        }
    }

    fn resolve_multi_frame(&self, input: &str, trace: &mut Vec<Trace>) -> String {
        let Some(pattern) = &self.config.multi_frame_candidate else {
            return input.to_owned();
        };

        let mut starts = pattern
            .captures_iter(input)
            .filter_map(|captures| {
                captures
                    .name(&self.config.multi_frame_group)
                    .or_else(|| captures.get(0))
                    .map(|matched| matched.start())
            })
            .collect::<Vec<_>>();
        starts.sort_unstable();
        starts.dedup();
        if starts.len() < 2 {
            return input.to_owned();
        }

        let candidates = starts
            .iter()
            .enumerate()
            .map(|(index, start)| {
                let end = starts.get(index + 1).copied().unwrap_or(input.len());
                input[*start..end]
                    .trim_end_matches(|character: char| {
                        character == ',' || character.is_whitespace()
                    })
                    .to_owned()
            })
            .collect::<Vec<_>>();

        let mut best_index = 0;
        let mut best_score = i32::MIN;
        let mut best_rules = Vec::new();
        let mut scores = Vec::with_capacity(candidates.len());
        for (index, candidate) in candidates.iter().enumerate() {
            let (score, rules) = self.score_candidate(candidate);
            scores.push(format!("{index}:{score}"));
            if index == 0 || score > best_score {
                best_index = index;
                best_score = score;
                best_rules = rules;
            }
        }

        trace.push(Trace::new(
            "multi_frame",
            "multi_frame.resolved",
            format!(
                "chose candidate {best_index} with score {best_score} ({}) using {}",
                scores.join(", "),
                if best_rules.is_empty() {
                    "no scoring rules".to_owned()
                } else {
                    best_rules.join(", ")
                }
            ),
        ));
        candidates
            .get(best_index)
            .cloned()
            .unwrap_or_else(|| input.to_owned())
    }

    fn score_candidate(&self, candidate: &str) -> (i32, Vec<String>) {
        let Ok(url) = Url::parse(candidate) else {
            return (self.config.settings.multi_frame.invalid_score, Vec::new());
        };
        let mut score = 0;
        let mut applied = Vec::new();
        for rule in &self.config.score_rules {
            if rule.matches(&url) {
                score += rule.score;
                applied.push(rule.id.clone());
            }
        }
        (score, applied)
    }

    fn strip_matrix(&self, url: &mut Url, trace: &mut Vec<Trace>) {
        let Some(pattern) = &self.config.matrix_regex else {
            return;
        };
        let path = url.path().to_owned();
        let stripped = pattern.replace_all(&path, "");
        if stripped.as_ref() != path.as_str() {
            url.set_path(&stripped);
            trace.push(Trace::new(
                "matrix_params",
                "matrix_params.removed",
                "removed matrix session parameters from the path",
            ));
        }
    }

    fn strip_path_segments(&self, url: &mut Url, trace: &mut Vec<Trace>) {
        let path = url.path().to_owned();
        let mut kept = Vec::new();
        let mut removed = Vec::new();

        for segment in path.split('/') {
            let decoded: Cow<'_, str> = if self.config.settings.path.decode_segments {
                percent_decode_str(segment).decode_utf8_lossy()
            } else {
                Cow::Borrowed(segment)
            };
            let matched = if segment.is_empty() {
                None
            } else {
                self.config
                    .segment_rules
                    .iter()
                    .find(|rule| rule.matches(&decoded))
            };
            if let Some(rule) = matched {
                removed.push(format!("{} ({})", rule.id, rule.class));
            } else {
                kept.push(segment);
            }
        }

        if !removed.is_empty() {
            let clean = kept.join("/");
            let clean = if clean.is_empty() {
                "/".to_owned()
            } else {
                clean
            };
            url.set_path(&clean);
            trace.push(Trace::new(
                "path_segments",
                "path_segments.removed",
                format!(
                    "removed {} high-entropy path segment(s): {}",
                    removed.len(),
                    removed.join(", ")
                ),
            ));
        }
    }

    fn normalize_fragment(&self, url: &mut Url, trace: &mut Vec<Trace>) {
        let Some(fragment) = url.fragment().map(str::to_owned) else {
            return;
        };
        if fragment.is_empty() {
            if self.config.settings.fragment.drop_empty {
                url.set_fragment(None);
                trace.push(Trace::new(
                    "fragment",
                    "fragment.empty",
                    "removed the dangling hash",
                ));
            }
            return;
        }

        let settings = &self.config.settings.fragment;
        let routable = settings
            .spa_prefixes
            .iter()
            .any(|prefix| fragment.starts_with(prefix))
            || settings
                .auth_values
                .iter()
                .any(|value| fragment.eq_ignore_ascii_case(value));

        if routable {
            let mut next = fragment;
            if settings.strip_query {
                if let Some(position) = next.find('?') {
                    next.truncate(position);
                    trace.push(Trace::new(
                        "fragment",
                        "fragment.spa_query",
                        "kept the SPA hash route and removed its query string",
                    ));
                }
            }
            if settings.drop_root && next == "/" {
                url.set_fragment(None);
                trace.push(Trace::new(
                    "fragment",
                    "fragment.root",
                    "removed the redundant root hash",
                ));
            } else if next.is_empty() {
                url.set_fragment(None);
            } else {
                url.set_fragment(Some(&next));
            }
        } else if settings.drop_unroutable {
            url.set_fragment(None);
            trace.push(Trace::new(
                "fragment",
                "fragment.unroutable",
                "removed a non-routable fragment",
            ));
        }
    }

    fn clear_auth_path_query(&self, url: &mut Url, trace: &mut Vec<Trace>) {
        if url.query().is_none() {
            return;
        }
        let path = url.path().to_owned();
        if let Some(rule) = self
            .config
            .auth_paths
            .iter()
            .find(|rule| rule.matches(&path))
        {
            let id = rule.id.clone();
            let class = rule.class.clone();
            url.set_query(None);
            trace.push(Trace::new(
                "auth_path",
                id,
                format!(
                    "cleared the query because the path is an explicit authentication endpoint ({class})"
                ),
            ));
        }
    }

    fn filter_query(&self, url: &mut Url, trace: &mut Vec<Trace>) {
        let Some(query) = url.query().map(str::to_owned) else {
            return;
        };
        if self.config.settings.query.drop_valueless && !query.contains('=') {
            url.set_query(None);
            trace.push(Trace::new(
                "query",
                "query.valueless",
                "removed a query string without key/value pairs",
            ));
            return;
        }

        let path = url.path().to_owned();
        let pairs = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        let mut kept = Vec::new();
        let mut removed = Vec::new();

        for (key, value) in pairs {
            let normalized = if self.config.settings.query.key_case == CaseMode::Lower {
                key.to_ascii_lowercase()
            } else {
                key.clone()
            };
            let matched = self
                .config
                .query_rules
                .iter()
                .find(|rule| rule.matches(&normalized, &value, &path));
            match matched {
                Some(rule) if rule.action == QueryAction::Keep => kept.push((key, value)),
                Some(rule) => removed.push((key, rule.id.clone(), rule.class.clone())),
                None => kept.push((key, value)),
            }
        }

        if removed.is_empty() {
            if self.config.settings.query.trace_kept {
                trace.push(Trace::new(
                    "query",
                    "query.kept",
                    "all remaining parameters are application routing",
                ));
            }
            return;
        }

        if kept.is_empty() {
            url.set_query(None);
        } else {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            serializer.extend_pairs(
                kept.iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            );
            let mut serialized = serializer.finish();
            if self.config.settings.query.decode_slashes {
                serialized = serialized.replace("%2F", "/").replace("%2f", "/");
            }
            url.set_query(Some(&serialized));
        }

        for (key, rule, class) in removed {
            trace.push(Trace::new(
                "query",
                rule,
                format!("removed parameter `{key}` ({class})"),
            ));
        }
    }

    fn strip_trailing_slash(&self, url: &mut Url, trace: &mut Vec<Trace>) {
        let path = url.path().to_owned();
        if !path.ends_with('/') {
            return;
        }
        let Some(rule) = self
            .config
            .trailing_slashes
            .iter()
            .find(|rule| rule.matches(&path))
        else {
            return;
        };
        let id = rule.id.clone();
        let class = rule.class.clone();
        url.set_path(&path[..path.len() - 1]);
        trace.push(Trace::new(
            "trailing_slash",
            id,
            format!("normalized the trailing slash on an authentication endpoint ({class})"),
        ));
    }
}
