mod app_controller;

use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QUrl};
use std::pin::Pin;

const MAIN_QML: &str = "qrc:/qt/qml/org/kryptopass/desktop/qml/Main.qml";

fn main() {
    let mut app = QGuiApplication::new();
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
