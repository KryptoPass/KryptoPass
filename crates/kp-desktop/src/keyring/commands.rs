//! Plain Rust commands for the local OpenPGP keyring.

use kryptopass_keyring::{KeyInfo, Keyring};

pub use super::request::OperationOutcome;

/// Executes a typed keyring-management operation from the desktop bridge.
pub fn execute_operation(request_json: &str) -> Result<OperationOutcome, String> {
    super::operations::execute_operation(request_json)
}

/// Lists public OpenPGP metadata without exposing GPGME to the UI layer.
pub fn list_keys() -> Result<Vec<KeyInfo>, String> {
    Keyring::system()
        .list_keys()
        .map_err(|error| super::errors::describe(&error))
}

/// Signs a fixed probe message so the configured system agent can request a passphrase.
pub fn probe_agent() -> Result<(), String> {
    Keyring::system()
        .probe_agent()
        .map_err(|error| super::errors::describe(&error))
}
