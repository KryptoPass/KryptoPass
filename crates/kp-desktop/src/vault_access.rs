//! Opening the vault: its file, the templates and the identity that signs what
//! this device writes.
//!
//! Every operation opens its own session on a worker thread, as SQLite
//! connections do not cross threads here. The identity and the templates live
//! for the process.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use kryptopass_app::{AppVault, VaultSession};
use kryptopass_core::vault::TemplateRegistry;
use kryptopass_identity::DeviceIdentity;
use kryptopass_keyring::Keyring;
use kryptopass_store::SqliteStore;

use crate::storage::paths;

const PROFILE_DIRECTORY: &str = "emulator-pairing-lab-v1";

static IDENTITY: Mutex<Option<Arc<DeviceIdentity>>> = Mutex::new(None);
static TEMPLATES: OnceLock<Arc<TemplateRegistry>> = OnceLock::new();

fn app_id() -> &'static str {
    crate::app_info::APP.desktop_file_id
}

/// The file of the active vault.
pub(crate) fn vault_path() -> Result<PathBuf> {
    paths::database_path(app_id())
}

/// The private directory of the device profile: identity and trust.
pub(crate) fn profile_directory() -> Result<PathBuf> {
    Ok(paths::browser_state_directory(app_id())?.join(PROFILE_DIRECTORY))
}

/// The templates items are built from.
pub(crate) fn templates() -> Result<&'static Arc<TemplateRegistry>> {
    if let Some(templates) = TEMPLATES.get() {
        return Ok(templates);
    }
    let templates =
        Arc::new(TemplateRegistry::builtin().context("built-in templates are invalid")?);
    Ok(TEMPLATES.get_or_init(|| templates))
}

/// The language templates are shown in.
pub(crate) fn language() -> String {
    std::env::var("LC_ALL")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("LC_MESSAGES")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .or_else(|| std::env::var("LANG").ok().filter(|value| !value.is_empty()))
        .and_then(|value| value.split(['_', '.', '@']).next().map(str::to_owned))
        .filter(|value| !value.is_empty() && value != "C" && value != "POSIX")
        .unwrap_or_else(|| "es".to_owned())
}

/// This device's identity, read once from the system keyring.
pub(crate) fn identity() -> Result<Arc<DeviceIdentity>> {
    let mut cached = IDENTITY.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(identity) = cached.as_ref() {
        return Ok(Arc::clone(identity));
    }
    let directory = profile_directory()?;
    let parent = directory
        .parent()
        .context("the device profile has no parent directory")?;
    std::fs::create_dir_all(parent).context("could not prepare the data directory")?;
    let identity = Arc::new(
        kryptopass_peers::PeerHost::signing_identity(&directory)
            .inspect_err(|error| tracing::warn!(%error, "device identity unavailable"))
            .context("the device identity is unavailable")?,
    );
    *cached = Some(Arc::clone(&identity));
    Ok(identity)
}

/// Drops the cached identity after the profile was reset.
pub(crate) fn forget_identity() {
    *IDENTITY.lock().unwrap_or_else(|poison| poison.into_inner()) = None;
}

/// A session that may ask for a passphrase, creating the default vault if new.
pub(crate) fn open_session() -> Result<VaultSession> {
    let path = vault_path()?;
    if let Some(active) = paths::active_vault_path(app_id())? {
        anyhow::ensure!(active.exists(), "the active vault no longer exists");
    }
    crate::infrastructure::entry_icons::bind_store(&path);
    VaultSession::open(
        path,
        Keyring::system(),
        Arc::clone(templates()?),
        identity()?,
    )
    .inspect_err(|error| tracing::warn!(code = error.code(), "vault did not open"))
    .context("could not open the vault")
}

/// A session on another vault file, which must already exist and be valid.
pub(crate) fn open_existing_session(path: PathBuf) -> Result<VaultSession> {
    SqliteStore::open_existing(&path, identity()?).context("not a valid vault")?;
    VaultSession::open(
        path,
        Keyring::system(),
        Arc::clone(templates()?),
        identity()?,
    )
    .context("could not open the vault")
}

/// A vault that only decrypts with passphrases gpg-agent already caches.
pub(crate) fn open_background() -> Result<AppVault> {
    Ok(open_session()?.background()?)
}

/// The store of the active vault, for a Sync session.
pub(crate) fn open_sync_store() -> Result<SqliteStore> {
    SqliteStore::open_existing(vault_path()?, identity()?).context("could not open the vault store")
}
