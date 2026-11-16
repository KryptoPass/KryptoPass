//! Configurable heuristics for canonicalizing login URLs.
//!
//! The crate contains no URL vocabulary: keys, path patterns, fragment rules,
//! scoring weights and heuristic thresholds are TOML data. Files under
//! `config/` are embedded at build time; callers can extend the same schema at
//! runtime with [`Config::extend_from_dir`].
//!
//! ```no_run
//! let sanitizer = kp_url_sanitizer::Sanitizer::embedded()?;
//! let result = sanitizer.sanitize("https://example.test/login?next=%2Fhome");
//! # Ok::<(), kp_url_sanitizer::ConfigError>(())
//! ```

mod config;
mod matcher;
mod sanitizer;

pub use config::{Config, ConfigError};
pub use sanitizer::{Sanitized, Sanitizer, Trace};
