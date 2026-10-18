//! Thin CXX-Qt adapters that expose application use cases to QML.

pub mod app_shell;
pub mod application;
pub mod backup_controller;
pub mod contacts_controller;
pub mod import_controller;
pub mod keyring_controller;
mod keyring_projection;
pub mod markdown_document;
pub(crate) mod native_ffi;
pub mod pairing_lab_controller;
mod secret_qr;
pub mod secrets_controller;
mod secrets_projection;
pub mod sensitive_clipboard;
