use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    let qml_module = QmlModule::new("org.kryptopass.desktop")
        .qml_file("ui/qml/Main.qml")
        .qml_file("ui/qml/MockStore.qml")
        .qml_file("ui/qml/KeyStore.qml")
        .qml_file("ui/qml/KeyDetail.qml")
        .qml_file("ui/qml/KeyListDelegate.qml")
        .qml_file("ui/qml/KeyStatus.qml")
        .qml_file("ui/qml/KeyAvatar.qml")
        .qml_file("ui/qml/IdentityValidity.qml")
        .qml_file("ui/qml/SecretDetail.qml")
        .qml_file("ui/qml/MarkdownView.qml")
        .qml_file("ui/qml/MarkdownDemo.qml")
        .depends([
            "QtQuick",
            "QtQuick.Controls",
            "QtQuick.Layouts",
            "org.kde.kirigami",
        ]);

    CxxQtBuilder::new_qml_module(qml_module)
        // Qt QML requires Qt Network on some platforms, including macOS.
        .qt_module("Network")
        .files([
            "src/ui_adapters/app_shell.rs",
            "src/ui_adapters/keyring_controller.rs",
            "src/ui_adapters/markdown_document.rs",
        ])
        .build();
}
