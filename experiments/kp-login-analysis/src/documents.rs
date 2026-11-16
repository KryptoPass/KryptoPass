use std::fmt;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DocumentId {
    country: String,
    kind: String,
    number: String,
}

impl DocumentId {
    pub fn country(&self) -> &str {
        &self.country
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn number(&self) -> &str {
        &self.number
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}:{}", self.country, self.kind, self.number)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanonicalUser {
    Document(DocumentId),
    Text(String),
}

impl CanonicalUser {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Document(_) => None,
        }
    }
}

impl fmt::Display for CanonicalUser {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(document) => write!(formatter, "dni:{document}"),
            Self::Text(value) => formatter.write_str(value),
        }
    }
}

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("cannot parse document policy: {0}")]
    Config(#[from] toml::de::Error),
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    documents: RawDocuments,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawDocuments {
    #[serde(default)]
    countries: Vec<RawCountry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCountry {
    code: String,
    #[serde(default)]
    tlds: Vec<String>,
    min_digits: usize,
    max_digits: usize,
    default_kind: String,
    #[serde(default)]
    number_separators: Vec<String>,
    #[serde(default)]
    prefixes: Vec<RawPrefix>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPrefix {
    value: String,
    kind: String,
}

struct CountryRule {
    code: String,
    tlds: Vec<String>,
    min_digits: usize,
    max_digits: usize,
    default_kind: String,
    separators: Vec<char>,
    prefixes: Vec<PrefixRule>,
}

struct PrefixRule {
    value: String,
    kind: String,
}

pub struct DocumentPolicy {
    countries: Vec<CountryRule>,
}

impl DocumentPolicy {
    pub fn embedded() -> Result<Self, DocumentError> {
        Self::from_toml(include_str!("../config/documents.toml"))
    }

    pub fn from_toml(source: &str) -> Result<Self, DocumentError> {
        let raw: RawConfig = toml::from_str(source)?;
        let countries = raw
            .documents
            .countries
            .into_iter()
            .map(|country| CountryRule {
                code: country.code.to_ascii_lowercase(),
                tlds: country
                    .tlds
                    .into_iter()
                    .map(|tld| tld.to_ascii_lowercase())
                    .collect(),
                min_digits: country.min_digits,
                max_digits: country.max_digits,
                default_kind: country.default_kind.to_ascii_lowercase(),
                separators: country
                    .number_separators
                    .iter()
                    .flat_map(|value| value.chars())
                    .collect(),
                prefixes: country
                    .prefixes
                    .into_iter()
                    .map(|prefix| PrefixRule {
                        value: prefix.value.to_ascii_lowercase(),
                        kind: prefix.kind.to_ascii_lowercase(),
                    })
                    .collect(),
            })
            .collect();
        Ok(Self { countries })
    }

    pub fn canonical(&self, value: &str, registrable_domain: Option<&str>) -> CanonicalUser {
        let value = value.trim().to_lowercase();
        for country in &self.countries {
            for prefix in &country.prefixes {
                if let Some(rest) = value.strip_prefix(prefix.value.as_str())
                    && let Some(number) = country.normalize_number(rest)
                {
                    return CanonicalUser::Document(DocumentId {
                        country: country.code.clone(),
                        kind: prefix.kind.clone(),
                        number,
                    });
                }
            }
        }
        if let Some(domain) = registrable_domain
            && let Some(tld) = domain.rsplit('.').next()
            && let Some(country) = self
                .countries
                .iter()
                .find(|country| country.tlds.iter().any(|candidate| candidate == tld))
            && let Some(number) = country.normalize_number(&value)
        {
            return CanonicalUser::Document(DocumentId {
                country: country.code.clone(),
                kind: country.default_kind.clone(),
                number,
            });
        }
        CanonicalUser::Text(value)
    }

    pub fn primary_variant<'a>(
        &self,
        document: &DocumentId,
        variants: &'a std::collections::BTreeMap<String, usize>,
    ) -> Option<&'a str> {
        let country = self
            .countries
            .iter()
            .find(|country| country.code == document.country)?;
        variants
            .iter()
            .filter_map(|(value, count)| {
                let lower = value.trim().to_lowercase();
                let mut prefix_len = 0;
                for prefix in &country.prefixes {
                    if prefix.kind != document.kind {
                        continue;
                    }
                    if let Some(rest) = lower.strip_prefix(prefix.value.as_str())
                        && country.normalize_number(rest).as_deref()
                            == Some(document.number.as_str())
                    {
                        prefix_len = prefix_len.max(prefix.value.chars().count());
                    }
                }
                let bare = document.kind == country.default_kind
                    && country.normalize_number(&lower).as_deref()
                        == Some(document.number.as_str());
                if prefix_len == 0 && !bare {
                    return None;
                }
                let uppercase = value.chars().any(char::is_uppercase);
                Some((value.as_str(), *count, prefix_len, uppercase))
            })
            .max_by(
                |(left_value, left_count, left_prefix, left_uppercase),
                 (right_value, right_count, right_prefix, right_uppercase)| {
                    left_count
                        .cmp(right_count)
                        .then_with(|| left_prefix.cmp(right_prefix))
                        .then_with(|| left_uppercase.cmp(right_uppercase))
                        .then_with(|| right_value.cmp(left_value))
                },
            )
            .map(|(value, _, _, _)| value)
    }
}

impl CountryRule {
    fn normalize_number(&self, value: &str) -> Option<String> {
        let number: String = value
            .chars()
            .filter(|character| !self.separators.contains(character))
            .collect();
        let length = number.chars().count();
        (length >= self.min_digits
            && length <= self.max_digits
            && number.chars().all(|character| character.is_ascii_digit()))
        .then_some(number)
    }
}
