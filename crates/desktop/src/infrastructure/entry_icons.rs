//! Bounded local image ingestion and vault-scoped image-provider resources.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Result, ensure};
use kryptopass_core::{EntryIcon, EntryIconAsset, MAX_ENTRY_ICON_BYTES};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use uuid::Uuid;

#[derive(Default)]
struct IconStore {
    path: Option<PathBuf>,
    scope: Uuid,
    staged: Option<EntryIconAsset>,
}

fn store() -> &'static Mutex<IconStore> {
    static STORE: OnceLock<Mutex<IconStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(IconStore::default()))
}

pub(crate) fn bind_store(path: &Path) {
    if let Ok(mut state) = store().lock() {
        if state.path.as_deref() != Some(path) {
            state.path = Some(path.to_owned());
            state.scope = Uuid::new_v4();
            state.staged = None;
        }
    }
}

pub(crate) fn source_for(icon: Option<&EntryIcon>) -> String {
    if let Some(EntryIcon::Favicon { host }) = icon {
        return format!("image://org.kryptopass.entryicons/favicon/{host}");
    }
    let Some(EntryIcon::Image { asset_id }) = icon else {
        return String::new();
    };
    let Ok(state) = store().lock() else {
        return String::new();
    };
    format!(
        "image://org.kryptopass.entryicons/{}/{}",
        state.scope, asset_id
    )
}

pub(crate) fn stage(asset: &EntryIconAsset) {
    if let Ok(mut state) = store().lock() {
        state.staged = Some(asset.clone());
    }
}

pub(crate) fn clear_stage() {
    if let Ok(mut state) = store().lock() {
        state.staged = None;
    }
}

pub(crate) fn prepare_image(path: &Path) -> Result<EntryIconAsset> {
    const MAX_INPUT: u64 = 4 * 1024 * 1024;
    let file = File::open(path)?;
    ensure!(file.metadata()?.is_file(), "icon source is not a file");
    ensure!(
        file.metadata()?.len() <= MAX_INPUT,
        "icon source is too large"
    );
    let mut input = Vec::new();
    file.take(MAX_INPUT + 1).read_to_end(&mut input)?;
    ensure!(input.len() as u64 <= MAX_INPUT, "icon source is too large");
    let png = crate::ui_adapters::native_ffi::ffi::normalize_entry_icon(&input);
    let asset = EntryIconAsset {
        id: Uuid::new_v4(),
        png,
    };
    asset.validate()?;
    Ok(asset)
}

pub(crate) fn valid_normalized_image(png: &[u8]) -> bool {
    crate::ui_adapters::native_ffi::ffi::valid_entry_icon(png)
}

pub(crate) fn entry_icon_bytes(source: &str) -> Vec<u8> {
    read_resource(source).unwrap_or_default()
}

fn read_resource(source: &str) -> Result<Vec<u8>> {
    if let Some(host) = source.strip_prefix("favicon/") {
        ensure!(
            kryptopass_core::valid_favicon_host(host),
            "invalid favicon host"
        );
        return Ok(crate::ui_adapters::native_ffi::ffi::fetch_favicon(
            &cxx_qt_lib::QString::from(host),
        ));
    }
    let (scope, asset) = source
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("invalid icon source"))?;
    let scope = Uuid::parse_str(scope)?;
    let id = Uuid::parse_str(asset)?;
    let path = {
        let state = store()
            .lock()
            .map_err(|_| anyhow::anyhow!("icon store unavailable"))?;
        ensure!(scope == state.scope, "icon belongs to another vault");
        if let Some(asset) = &state.staged {
            if asset.id == id {
                return Ok(asset.png.clone());
            }
        }
        state
            .path
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no icon store"))?
    };
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    connection.pragma_update(None, "trusted_schema", false)?;
    let length: Option<i64> = connection
        .query_row(
            "SELECT length(png) FROM entry_icon_assets WHERE id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    ensure!(
        length.is_some_and(|length| length >= 45 && length <= MAX_ENTRY_ICON_BYTES as i64),
        "icon missing or too large"
    );
    let png: Vec<u8> = connection.query_row(
        "SELECT png FROM entry_icon_assets WHERE id = ?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    EntryIconAsset {
        id,
        png: png.clone(),
    }
    .validate()?;
    Ok(png)
}
