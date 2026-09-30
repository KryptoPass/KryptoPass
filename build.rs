use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    let qml_module = QmlModule::new("org.kryptopass.desktop")
        .qml_file("qml/Main.qml")
        .qml_file("qml/components/InfoCard.qml")
        .qml_file("qml/components/KpButton.qml")
        .qml_file("qml/components/NavItem.qml")
        .qml_file("qml/components/StatusChip.qml");

    CxxQtBuilder::new_qml_module(qml_module)
        // Qt QML requires Qt Network on some platforms, including macOS.
        .qt_module("Network")
        .files(["src/app_controller.rs"])
        .build();
}
