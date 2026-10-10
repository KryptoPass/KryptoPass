//! Plain Rust commands for the local OpenPGP keyring.

#[path = "edit.rs"]
mod edit;
#[path = "files.rs"]
mod files;
#[path = "operations.rs"]
mod operations;
#[path = "request.rs"]
mod request;

use kryptopass_core::{KeyInfo, Keyring};

pub use request::OperationOutcome;

/// Executes a typed keyring-management operation from the desktop bridge.
pub fn execute_operation(request_json: &str) -> Result<OperationOutcome, String> {
    operations::execute_operation(request_json)
}

/// Lists public OpenPGP metadata without exposing GPGME to the UI layer.
pub fn list_keys() -> Result<Vec<KeyInfo>, String> {
    let mut keyring = Keyring::open().map_err(|error| error.to_string())?;

    keyring.list_keys().map_err(|error| error.to_string())
}

/// Signs a fixed probe message so the configured system agent can request a passphrase.
pub fn probe_agent() -> Result<(), String> {
    let mut keyring = Keyring::open().map_err(|error| error.to_string())?;

    keyring
        .sign_test_message()
        .map_err(|error| error.to_string())
}
