//! Rust owns application identity, composition, style, and QML startup.

use crate::app_info::APP;
use cxx_qt_lib::{QQmlApplicationEngine, QQuickStyle, QString, QUrl};

#[cxx::bridge(namespace = "kryptopass")]
pub mod ffi {
    unsafe extern "C++" {
        include!("native/runtime.h");
        #[namespace = ""]
        type QGuiApplication = cxx_qt_lib::QGuiApplication;
        #[namespace = ""]
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;
        #[namespace = ""]
        type QString = cxx_qt_lib::QString;
        fn new_application(args: &Vec<String>) -> UniquePtr<QGuiApplication>;
        fn configure_engine(engine: Pin<&mut QQmlApplicationEngine>, smoke: bool);
        fn log_message(message: &QString);
    }
    extern "Rust" {
        fn run(args: Vec<String>) -> i32;
    }
}

fn run(args: Vec<String>) -> i32 {
    let smoke = args.iter().any(|arg| arg == "--smoke");
    let mut application = ffi::new_application(&args);
    let Some(mut app) = application.as_mut() else {
        return 1;
    };
    app.as_mut()
        .set_application_name(&QString::from(format!("{} — UI demo", APP.display_name)));
    app.as_mut()
        .set_application_version(&QString::from(APP.version));
    app.as_mut()
        .set_organization_name(&QString::from(APP.organization));
    app.as_mut()
        .set_organization_domain(&QString::from(APP.organization_domain));
    // This composition intentionally uses bundled controls on both host and Android.
    QQuickStyle::set_style(&QString::from("Material"));
    let mut engine = QQmlApplicationEngine::new();
    let Some(mut engine) = engine.as_mut() else {
        return 1;
    };
    ffi::configure_engine(engine.as_mut(), smoke);
    let main = format!(
        "qrc:/qt/qml/{}/Main.qml",
        APP.desktop_file_id.replace('.', "/")
    );
    ffi::log_message(&QString::from(
        "KryptoPass: Rust startup, demo backend enabled",
    ));
    engine.as_mut().load(&QUrl::from(main.as_str()));
    if smoke {
        let probe = format!(
            "qrc:/qt/qml/{}/BridgeSmoke.qml",
            APP.desktop_file_id.replace('.', "/")
        );
        engine.as_mut().load(&QUrl::from(probe.as_str()));
    }
    app.exec()
}
