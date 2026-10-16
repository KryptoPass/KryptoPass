//! Read-only demo composition. All values are invented and reset on restart.

pub mod app_shell;
pub mod backup_controller;
pub mod contacts_controller;
pub mod import_controller;
pub mod keyring_controller;
pub mod markdown_document;
pub mod secrets_controller;
pub mod sensitive_clipboard;
pub mod store;

const UNAVAILABLE: &str = "Esta función no está disponible en la demostración.";
