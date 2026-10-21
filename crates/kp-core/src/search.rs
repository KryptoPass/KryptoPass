//! Word matching for public list metadata, without opening encrypted content.

use deunicode::deunicode;
use regex::{RegexSet, RegexSetBuilder};

/// Compiles a query once while a list evaluates its public metadata rows.
#[derive(Default)]
pub struct MetadataSearch {
    query: String,
    matcher: Option<RegexSet>,
}

impl MetadataSearch {
    /// Matches every literal query word in any order after ASCII transliteration.
    /// An oversized query that cannot be compiled matches no rows.
    pub fn matches(&mut self, query: &str, metadata: &str) -> bool {
        let query = query.trim();
        if query.is_empty() {
            return true;
        }
        if self.query != query {
            self.matcher =
                RegexSetBuilder::new(deunicode(query).split_whitespace().map(regex::escape))
                    .case_insensitive(true)
                    .build()
                    .ok();
            self.query.clear();
            self.query.push_str(query);
        }
        self.matcher.as_ref().is_some_and(|matcher| {
            matcher.matches(&deunicode(metadata)).iter().count() == matcher.len()
        })
    }
}
