//! Plain Rust commands for the local OpenPGP keyring.

use kryptopass_core::{KeyInfo, Keyring};

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
