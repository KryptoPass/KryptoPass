mod app_info;
mod backup;
mod i18n;
mod import;
mod infrastructure;
mod keyring;
mod markdown;
mod process_security;
mod qt;
mod ui_adapters;

use app_info::APP;
use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QQuickStyle, QString, QUrl};
use std::pin::Pin;
use tracing_subscriber::EnvFilter;

const DEFAULT_STYLE: &str = "org.kde.desktop";

fn main() {
    init_tracing();
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
    engine.load(&QUrl::from(main_qml.as_str()));
    if let Some(path) = std::env::args()
        .skip_while(|arg| arg != "--screenshot")
        .nth(1)
    {
        ui_adapters::native_ffi::ffi::capture_screenshot(&QString::from(path));
    }
    let exit_code = app.exec();
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn configure_quick_style() {
    // Explicit user preferences take precedence over the KDE default.
    if std::env::var("QT_QUICK_CONTROLS_STYLE").is_err() {
        QQuickStyle::set_style(&QString::from(DEFAULT_STYLE));
    }
}
