use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

use regex::{Regex, RegexBuilder};
use serde::Deserialize;
use thiserror::Error;
use url::{Host, Url};

/// Priority bucket used by [`OriginScope`]'s deterministic ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeKind {
    PublicDomain,
    InternalDomain,
    PrivateIpv4,
    Localhost,
    OtherIp,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ScopeHost {
    Domain { registrable: String, scope: String },
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
}

/// Primary grouping key for login URLs: the registrable domain plus any
/// tenant-specific subdomain labels.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OriginScope {
    kind: ScopeKind,
    host: ScopeHost,
    port: Option<u16>,
}

impl OriginScope {
    pub fn kind(&self) -> ScopeKind {
        self.kind
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn registrable_domain(&self) -> Option<&str> {
        match &self.host {
            ScopeHost::Domain { registrable, .. } if !registrable.is_empty() => Some(registrable),
            _ => None,
        }
    }
}

impl fmt::Display for OriginScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.host {
            ScopeHost::Domain { scope, .. } => formatter.write_str(scope)?,
            ScopeHost::Ipv4(address) => write!(formatter, "{address}")?,
            ScopeHost::Ipv6(address) => write!(formatter, "[{address}]")?,
        }
        if let Some(port) = self.port {
            write!(formatter, ":{port}")?;
        }
        Ok(())
    }
}

impl PartialOrd for OriginScope {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OriginScope {
    fn cmp(&self, other: &Self) -> Ordering {
        self.kind.cmp(&other.kind).then_with(|| {
            let host = HostOrder::of(&self.host).cmp(&HostOrder::of(&other.host));
            match self.kind {
                ScopeKind::Localhost => self.port.cmp(&other.port).then(host),
                _ => host.then_with(|| self.port.cmp(&other.port)),
            }
        })
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum HostOrder<'a> {
    Domain {
        registrable: &'a str,
        scope: &'a str,
    },
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
}

impl<'a> HostOrder<'a> {
    fn of(host: &'a ScopeHost) -> Self {
        match host {
            ScopeHost::Domain { registrable, scope } => Self::Domain { registrable, scope },
            ScopeHost::Ipv4(address) => Self::Ipv4(*address),
            ScopeHost::Ipv6(address) => Self::Ipv6(*address),
        }
    }
}

#[derive(Debug, Error)]
pub enum ScopeError {
    #[error("cannot parse scope configuration: {0}")]
    Config(#[from] toml::de::Error),
    #[error("invalid regex `{pattern}`: {source}")]
    Regex {
        pattern: String,
        #[source]
        source: regex::Error,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    scope: RawScope,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScope {
    #[serde(default)]
    role_labels: Vec<String>,
    #[serde(default)]
    role_label_patterns: Vec<String>,
    #[serde(default)]
    tenant_suffixes: Vec<String>,
    #[serde(default)]
    internal_domain_suffixes: Vec<String>,
    #[serde(default)]
    host_aliases: HashMap<String, String>,
}

pub struct ScopeResolver {
    role_labels: HashSet<String>,
    role_patterns: Vec<Regex>,
    tenant_suffixes: Vec<String>,
    internal_domain_suffixes: Vec<String>,
    host_aliases: HashMap<String, String>,
}

impl ScopeResolver {
    pub fn embedded() -> Result<Self, ScopeError> {
        Self::from_toml(include_str!("../config/scope.toml"))
    }

    pub fn from_toml(source: &str) -> Result<Self, ScopeError> {
        let raw: RawConfig = toml::from_str(source)?;
        let role_labels = raw
            .scope
            .role_labels
            .into_iter()
            .map(|label| label.to_ascii_lowercase())
            .collect();
        let mut role_patterns = Vec::with_capacity(raw.scope.role_label_patterns.len());
        for pattern in raw.scope.role_label_patterns {
            let compiled = RegexBuilder::new(&pattern)
                .case_insensitive(true)
                .build()
                .map_err(|source| ScopeError::Regex {
                    pattern: pattern.clone(),
                    source,
                })?;
            role_patterns.push(compiled);
        }
        let tenant_suffixes = raw
            .scope
            .tenant_suffixes
            .into_iter()
            .map(|suffix| suffix.to_ascii_lowercase())
            .collect();
        let internal_domain_suffixes = raw
            .scope
            .internal_domain_suffixes
            .into_iter()
            .map(|suffix| suffix.to_ascii_lowercase())
            .collect();
        let host_aliases = raw
            .scope
            .host_aliases
            .into_iter()
            .map(|(host, alias)| (normalize_host_key(&host), alias.to_ascii_lowercase()))
            .collect();
        Ok(Self {
            role_labels,
            role_patterns,
            tenant_suffixes,
            internal_domain_suffixes,
            host_aliases,
        })
    }

    pub fn resolve(&self, url: &Url) -> OriginScope {
        let (kind, host) = match url.host() {
            Some(Host::Ipv4(address)) => self.ipv4_scope(address),
            Some(Host::Ipv6(address)) => self.ipv6_scope(address),
            Some(Host::Domain(domain)) => self.domain_scope(domain),
            None => (
                ScopeKind::Other,
                ScopeHost::Domain {
                    registrable: String::new(),
                    scope: String::new(),
                },
            ),
        };
        OriginScope {
            kind,
            host,
            port: url.port(),
        }
    }

    pub fn resolve_host(&self, host: &str) -> OriginScope {
        let bare = host.trim().trim_start_matches('[').trim_end_matches(']');
        let (kind, host) = if let Ok(address) = bare.parse::<Ipv4Addr>() {
            self.ipv4_scope(address)
        } else if let Ok(address) = bare.parse::<Ipv6Addr>() {
            self.ipv6_scope(address)
        } else {
            self.domain_scope(host)
        };
        OriginScope {
            kind,
            host,
            port: None,
        }
    }

    fn ipv4_scope(&self, address: Ipv4Addr) -> (ScopeKind, ScopeHost) {
        if let Some(alias) = self
            .host_aliases
            .get(&normalize_host_key(&address.to_string()))
        {
            return self.domain_scope_inner(alias, false);
        }
        let kind = if address.is_loopback() {
            ScopeKind::Localhost
        } else if address.is_private() {
            ScopeKind::PrivateIpv4
        } else {
            ScopeKind::OtherIp
        };
        (kind, ScopeHost::Ipv4(address))
    }

    fn ipv6_scope(&self, address: Ipv6Addr) -> (ScopeKind, ScopeHost) {
        if let Some(alias) = self
            .host_aliases
            .get(&normalize_host_key(&address.to_string()))
        {
            return self.domain_scope_inner(alias, false);
        }
        let kind = if address.is_loopback() {
            ScopeKind::Localhost
        } else {
            ScopeKind::OtherIp
        };
        (kind, ScopeHost::Ipv6(address))
    }

    fn domain_scope(&self, host: &str) -> (ScopeKind, ScopeHost) {
        self.domain_scope_inner(host, true)
    }

    fn domain_scope_inner(&self, host: &str, check_alias: bool) -> (ScopeKind, ScopeHost) {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if check_alias && let Some(alias) = self.host_aliases.get(&normalize_host_key(&host)) {
            return self.domain_scope_inner(alias, false);
        }
        let Some(registrable) = psl::domain_str(&host) else {
            let kind = self.domain_kind(&host);
            return (
                kind,
                ScopeHost::Domain {
                    registrable: host.clone(),
                    scope: host,
                },
            );
        };
        let subdomain = host
            .strip_suffix(registrable)
            .map(|prefix| prefix.strip_suffix('.').unwrap_or(prefix))
            .unwrap_or_default();
        let labels: Vec<&str> = subdomain
            .split('.')
            .filter(|label| !label.is_empty())
            .collect();
        let start = labels
            .iter()
            .position(|label| !self.is_role(label))
            .unwrap_or(labels.len());
        let scope = if let Some((tenant, rest)) = labels[start..].split_first() {
            let tenant = self.strip_tenant_suffix(tenant);
            let mut scope = String::with_capacity(host.len());
            scope.push_str(tenant);
            for label in rest {
                scope.push('.');
                scope.push_str(label);
            }
            scope.push('.');
            scope.push_str(registrable);
            scope
        } else {
            registrable.to_owned()
        };
        let kind = self.domain_kind(&scope);
        (
            kind,
            ScopeHost::Domain {
                registrable: registrable.to_owned(),
                scope,
            },
        )
    }

    fn domain_kind(&self, scope: &str) -> ScopeKind {
        if scope == "localhost" {
            ScopeKind::Localhost
        } else if self
            .internal_domain_suffixes
            .iter()
            .any(|suffix| scope.ends_with(suffix.as_str()))
        {
            ScopeKind::InternalDomain
        } else {
            ScopeKind::PublicDomain
        }
    }

    fn is_role(&self, label: &str) -> bool {
        self.role_labels.contains(label)
            || self.role_patterns.iter().any(|rule| rule.is_match(label))
    }

    fn strip_tenant_suffix<'a>(&self, label: &'a str) -> &'a str {
        for suffix in &self.tenant_suffixes {
            if let Some(stripped) = label.strip_suffix(suffix.as_str())
                && !stripped.is_empty()
            {
                return stripped;
            }
        }
        label
    }
}

fn normalize_host_key(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}
