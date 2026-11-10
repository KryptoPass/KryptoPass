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

/// Signs a fixed probe message with one specific key so the agent asks for that key's passphrase.
pub fn probe_agent_for(fingerprint: &str) -> Result<(), String> {
    let fingerprint = kryptopass_keyring::Fingerprint::parse(fingerprint)
        .map_err(|_| crate::i18n::text("La huella de la clave no es válida."))?;
    Keyring::system()
        .probe_agent_for(&fingerprint)
        .map_err(|error| super::errors::describe(&error))
}

/// Signs a fixed probe message so the configured system agent can request a passphrase.
pub fn probe_agent() -> Result<(), String> {
    Keyring::system()
        .probe_agent()
        .map_err(|error| super::errors::describe(&error))
}

/// Observed secret availability, without reading or exporting secret material.
pub fn availability() -> Result<String, String> {
    let read = || -> anyhow::Result<String> {
        let mut context = gpgme::Context::from_protocol(gpgme::Protocol::OpenPgp)?;
        context.add_key_list_mode(gpgme::KeyListMode::WITH_SECRET)?;
        let mut records = serde_json::Map::new();
        for key in context.keys()? {
            for component in key?.subkeys() {
                if let Ok(fingerprint) = component.fingerprint() {
                    let location = if component.is_card_key() {
                        "card"
                    } else if component.is_secret() {
                        "local"
                    } else {
                        "public"
                    };
                    records.insert(
                        fingerprint.to_owned(),
                        serde_json::json!({"location": location}),
                    );
                }
            }
        }
        Ok(serde_json::Value::Object(records).to_string())
    };
    read().map_err(|_| crate::i18n::text("No se pudo comprobar dónde está disponible cada clave."))
}
