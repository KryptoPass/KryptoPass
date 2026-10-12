mod app_info;
mod i18n;
mod infrastructure;
mod keyring;
mod markdown;
mod qt;
mod ui_adapters;

use app_info::APP;
use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QQuickStyle, QString, QUrl};
use std::pin::Pin;
use tracing_subscriber::EnvFilter;

const MAIN_QML: &str = "qrc:/qt/qml/org/kryptopass/desktop/ui/qml/Main.qml";
const DEFAULT_STYLE: &str = "org.kde.desktop";

fn main() {
    init_tracing();

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

    engine.load(&QUrl::from(MAIN_QML));
    app.exec();
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
