mod app_controller;

use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QQuickStyle, QString, QUrl};
use std::pin::Pin;

const MAIN_QML: &str = "qrc:/qt/qml/org/kryptopass/desktop/qml/Main.qml";

fn main() {
    let mut app = QGuiApplication::new();

    // Fija `org.kde.desktop` para la integración con Kirigami, salvo `QT_QUICK_CONTROLS_STYLE` explícito.
    if std::env::var("QT_QUICK_CONTROLS_STYLE").is_err() {
        QQuickStyle::set_style(&QString::from("org.kde.desktop"));
    }
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(MAIN_QML));
    }

    if let Some(engine) = engine.as_mut() {
        let engine: Pin<&mut QQmlEngine> = engine.upcast_pin();
        engine
            .on_quit(|_| {
                println!("KryptoPass Desktop closed cleanly.");
            })
            .release();
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
