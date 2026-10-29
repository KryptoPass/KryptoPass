//! Trust in other devices and pairing invitations, in their own database so
//! forgetting or resetting pairing never touches the vault.

mod sqlite;

pub use sqlite::SqliteTrustStore;
