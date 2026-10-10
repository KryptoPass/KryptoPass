//! Centralized application identity.
//!
//! Single runtime source for the metadata declared in `Cargo.toml`.
//! Everything else (Qt setup, bridge properties, install files) reads
//! from here; nothing hardcodes name, version or organization twice.
//!
//! Values come from the `CARGO_PKG_*` variables Cargo injects at compile
//! time for this crate, so they always match the manifest.
//!
//! NOTE: this module must live in the binary crate on purpose — each crate
//! only sees its own `CARGO_PKG_*` values.

/// Application identity shared by the Qt setup, the QML bridge and tooling.
///
/// Only what the app consumes today lives here; description, license and
/// binary name stay in `Cargo.toml` (and the install generator reads them
/// from there) until a consumer needs them at runtime.
pub struct AppInfo {
    /// Display name for task managers, About dialogs and install files.
    pub display_name: &'static str,
    /// SemVer version from the workspace manifest.
    pub version: &'static str,
    /// Organization shown by the OS (task manager, config paths).
    pub organization: &'static str,
    /// Organization domain (settings storage, reverse-DNS roots).
    pub organization_domain: &'static str,
    /// `.desktop` file id: must match the installed file name exactly.
    pub desktop_file_id: &'static str,
}

/// Canonical application identity. Update `Cargo.toml`, not this file.
pub const APP: AppInfo = AppInfo {
    display_name: "KryptoPass",
    version: env!("CARGO_PKG_VERSION"),
    organization: "KryptoPass",
    organization_domain: "kryptopass.org",
    desktop_file_id: "org.kryptopass.desktop",
};
