//! Typed management of the local OpenPGP keyring.
//!
//! [`Keyring`] is the only entry point. Every operation takes validated
//! values ([`Fingerprint`], [`UserId`], [`Expiry`], ...) instead of JSON or
//! loose strings, and reports failures as [`Error`] variants rather than
//! finished sentences: wording and translation belong to the caller.
//!
//! No GPGME type crosses the public API. Private key material only ever
//! travels from GnuPG to a local file; it is never returned to the caller.
//!
//! It also encrypts and decrypts with those keys. With the `vault` feature,
//! [`KeyringCipher`] serves as the vault's encryption port.
//!
//! All methods block until GnuPG finishes, and some wait for the user in
//! pinentry. Call them from a worker thread, never from a UI thread.
//!
//! GnuPG [`MINIMUM_GNUPG`] or later is required; older engines are refused
//! with [`Error::UnsupportedEngine`] before any operation runs.

mod backend;
mod crypto;
mod error;
mod files;
mod gpg_cli;
mod keyring;
mod model;
mod operation;
mod revocation;
mod types;
#[cfg(feature = "vault")]
mod vault;

pub use backend::MINIMUM_GNUPG;
pub use crypto::Unlock;
pub use error::{Backend, CommandError, Error, FileError, InvalidInput, Result, Step, Unconfirmed};
pub use keyring::Keyring;
pub use model::{CertificationInfo, KeyInfo, SubkeyInfo, UserIdInfo, Validity};
pub use operation::{Operation, Scope};
pub use types::{
    CertificationKind, DeleteScope, Expiry, ExpiryTarget, Fetched, Fingerprint, ImportReport,
    Keyserver, Published, SubkeyUsage, UserId,
};
#[cfg(feature = "vault")]
pub use vault::KeyringCipher;
