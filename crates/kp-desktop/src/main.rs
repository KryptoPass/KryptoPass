mod app_info;
mod backup;
mod i18n;
mod import;
mod infrastructure;
mod keyring;
mod markdown;
mod process_security;
mod qt;
mod storage;
mod transport_diagnostics;
mod ui_adapters;
mod vault_sync;

use app_info::APP;
use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QQuickStyle, QString, QUrl};
use std::pin::Pin;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

const DEFAULT_STYLE: &str = "org.kde.desktop";

fn main() {
    init_tracing();
    process_security::prepare();
    if run_synthetic_sync_cli() {
        return;
    }

    let mut application = ui_adapters::application::new();
    let Some(mut app) = application.as_mut() else {
        tracing::error!("Failed to initialize QApplication");
        return;
    };

    app.as_mut()
        .set_application_name(&QString::from(APP.display_name));
    app.as_mut()
        .set_application_version(&QString::from(APP.version));
    app.as_mut()
        .set_organization_name(&QString::from(APP.organization));
    app.as_mut()
        .set_organization_domain(&QString::from(APP.organization_domain));

    // Match the installed desktop entry for proper Wayland integration.
    QGuiApplication::set_desktop_file_name(&QString::from(APP.desktop_file_id));

    ui_adapters::native_ffi::ffi::initialize_desktop();
    // Browser hosts, screenshots and isolated profiles run beside the user's instance.
    let separate_instance = std::env::args().any(|arg| {
        matches!(
            arg.as_str(),
            "--browser-integration" | "--screenshot" | "--new-instance"
        )
    });
    if !separate_instance && !ui_adapters::native_ffi::ffi::register_unique_application() {
        tracing::warn!("Could not register the single-instance D-Bus service");
    }
    ui_adapters::application::finish_desktop_registration();
    process_security::initialize();

    let _browser_server = if std::env::args().any(|arg| arg == "--browser-integration") {
        match kryptopass_browser::start(APP.desktop_file_id, APP.version) {
            Ok(server) => Some(server),
            Err(error) => {
                tracing::error!(%error, "Could not start browser integration");
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    configure_quick_style();

    let mut qml_engine = QQmlApplicationEngine::new();
    let Some(mut engine) = qml_engine.as_mut() else {
        tracing::error!("Failed to initialize QQmlApplicationEngine");
        return;
    };

    ui_adapters::native_ffi::ffi::install_entry_icon_provider(
        engine.as_mut().upcast_pin(),
        &QString::from(APP.desktop_file_id),
    );

    let base_engine: Pin<&mut QQmlEngine> = engine.as_mut().upcast_pin();
    base_engine
        .on_quit(|_| tracing::info!("QML requested application shutdown"))
        .release();

    let main_qml = format!(
        "qrc:/qt/qml/{}/ui/qml/Main.qml",
        APP.desktop_file_id.replace('.', "/")
    );
    engine.as_mut().load(&QUrl::from(main_qml.as_str()));
    let arguments = std::env::args().collect::<Vec<_>>();
    let screenshot_index = arguments.iter().position(|arg| arg == "--screenshot");
    if screenshot_index.is_none() {
        ui_adapters::application::install_main_window_state_saver(engine.as_ref().get_ref());
    }
    if let Some(path) = screenshot_index.and_then(|index| arguments.get(index + 1)) {
        ui_adapters::native_ffi::ffi::capture_screenshot(&QString::from(path));
    }
    let exit_code = app.exec();
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

/// Local debug control for the same fixed public-fixture actions exposed by Devices.
/// Output contains counts/public device IDs only, never decrypted fields or key material.
fn run_synthetic_sync_cli() -> bool {
    let arguments = std::env::args().collect::<Vec<_>>();
    let Some(index) = arguments
        .iter()
        .position(|arg| arg == "--synthetic-sync-cli")
    else {
        return false;
    };
    if !cfg!(debug_assertions) {
        std::process::exit(2);
    }
    let result = (|| -> anyhow::Result<serde_json::Value> {
        use ui_adapters::synthetic_sync::{self, SyntheticSyncAction};
        let action = arguments
            .get(index + 1)
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing action"))?;
        let directory = storage::paths::browser_state_directory(APP.desktop_file_id)?
            .join("emulator-pairing-lab-v1");
        if action == "peers" {
            let host = kryptopass_pairing_lab::LabHost::open_existing_system_profile(&directory)?;
            return Ok(
                serde_json::json!({"devices":host.trusted_peers()?.iter().map(|peer| peer.device_id.to_string()).collect::<Vec<_>>()}),
            );
        }
        let encoded = arguments
            .get(index + 2)
            .ok_or_else(|| anyhow::anyhow!("missing device"))?;
        anyhow::ensure!(
            encoded.len() == 64 && encoded.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid device"
        );
        let mut bytes = [0; 32];
        for (offset, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&encoded[offset * 2..offset * 2 + 2], 16)?;
        }
        let peer = kryptopass_identity::DeviceId::from_bytes(bytes);
        let action = match action {
            "activate" => SyntheticSyncAction::Activate,
            "create" => SyntheticSyncAction::CreateCredential,
            "read" => SyntheticSyncAction::Refresh,
            _ => anyhow::bail!("invalid fixture action"),
        };
        let snapshot = synthetic_sync::run(&directory, peer, action).map_err(|error| {
            // Fixed, payload-free diagnostics from the same error mapping as the UI.
            eprintln!("{}", error.message());
            anyhow::anyhow!("synthetic fixture operation failed")
        })?;
        Ok(
            serde_json::json!({"revisions":snapshot.local_revision_count,"heads":snapshot.rows.len()}),
        )
    })();
    match result {
        Ok(value) => println!("{value}"),
        Err(_) => {
            eprintln!("Synthetic Sync action failed; existing vaults were preserved.");
            std::process::exit(1);
        }
    }
    true
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_filter(transport_diagnostics::PacketWarningFilter::new()),
        )
        .init();
}

fn configure_quick_style() {
    // Explicit user preferences take precedence over the KDE default.
    if std::env::var("QT_QUICK_CONTROLS_STYLE").is_err() {
        QQuickStyle::set_style(&QString::from(DEFAULT_STYLE));
    }
}
