//! The application layer: everything the app does with a vault, without UI.
//!
//! It joins the pieces below it: the vault use cases (`kp-core`), the SQLite
//! store (`kp-store`), GnuPG through the keyring (`kp-keyring`), templates
//! (`kp-template`), generators (`kp-generator`) and private search
//! (`kp-search`). A UI only adapts these types to its widgets; it holds no
//! logic of its own.
//!
//! Every call blocks (GnuPG, SQLite) and some wait for pinentry, so callers
//! run them off the UI thread.

#![forbid(unsafe_code)]

mod actions;
mod browser;
mod draft;
mod error;
mod form;
mod generator;
mod search;
mod session;

pub use actions::{qr_payload, totp_code};
pub use browser::{Candidate, Credentials, candidates, credentials};
pub use draft::{Draft, Origin};
pub use error::AppError;
pub use form::{Form, FormChoice, FormField, FormSection};
pub use generator::{generate_for, settings_for};
pub use search::{VaultSearchSource, catalog_records};
pub use session::{AppVault, VaultSession};
