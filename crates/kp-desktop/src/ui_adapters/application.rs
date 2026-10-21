use cxx_qt_lib::{QByteArray, QGuiApplication, QQmlApplicationEngine, QVector};

#[cxx::bridge(namespace = "kryptopass")]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qguiapplication.h");
        #[namespace = ""]
        type QGuiApplication = cxx_qt_lib::QGuiApplication;
        include!("cxx-qt-lib/core/qvector/qvector_QByteArray.h");
        #[namespace = ""]
        type QVector_QByteArray = cxx_qt_lib::QVector<cxx_qt_lib::QByteArray>;
        include!("native/application.h");
        #[rust_name = "new_application"]
        fn newApplication(args: &QVector_QByteArray) -> UniquePtr<QGuiApplication>;
        #[rust_name = "finish_desktop_registration"]
        fn finishDesktopRegistration();
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        #[namespace = ""]
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;
        #[rust_name = "install_main_window_state_saver"]
        fn installMainWindowStateSaver(engine: &QQmlApplicationEngine);
    }
}

pub fn install_main_window_state_saver(engine: &QQmlApplicationEngine) {
    ffi::install_main_window_state_saver(engine);
}

pub fn finish_desktop_registration() {
    ffi::finish_desktop_registration();
}

pub fn new() -> cxx::UniquePtr<QGuiApplication> {
    let mut args = QVector::<QByteArray>::default();
    for arg in std::env::args() {
        args.append(QByteArray::from(arg.as_bytes()));
    }
    ffi::new_application(&args)
}
