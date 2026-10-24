//! Private search independent of vault schemas, persistence, cryptography and Qt.
//!
//! The index owns protected bytes. A generic source supplies minimal projections;
//! the session coordinates invalidation without holding a lock during decryption.

#![forbid(unsafe_code)]

mod arena;
mod index;
mod normalize;
mod session;

pub use index::{CatalogRecord, Coverage, Index, Limits, MemoryUsage, RecordId, RecordKey};
pub use normalize::Projection;
pub use session::{SearchSession, SearchSource};

/// Payload-free failures: caller diagnostics must never include a private value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchError {
    ProtectionUnavailable,
    LimitExceeded,
    InvalidInput,
    StaleRevision,
    Unavailable,
    Closed,
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SearchError {}

pub(crate) fn allocation_error(error: std::io::Error) -> SearchError {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => SearchError::ProtectionUnavailable,
        _ => SearchError::LimitExceeded,
    }
}
