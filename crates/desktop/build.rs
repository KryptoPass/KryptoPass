use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    let qml_module = QmlModule::new("org.kryptopass.desktop")
        .qml_file("ui/qml/Main.qml")
        .qml_file("ui/qml/PanelHeader.qml")
        .qml_file("ui/qml/PanelFooter.qml")
        .qml_file("ui/qml/SplitterHandle.qml")
        .qml_file("ui/qml/NavigationItem.qml")
        .qml_file("ui/qml/SidebarPane.qml")
        .qml_file("ui/qml/EntryListPane.qml")
        .qml_file("ui/qml/EntryDetailPane.qml")
        .qml_file("ui/qml/SecretEditor.qml")
        .qml_file("ui/qml/SecretStore.qml")
        .qml_file("ui/qml/LoginFields.qml")
        .qml_file("ui/qml/NoteFields.qml")
        .qml_file("ui/qml/RecipientsChooser.qml")
        .qml_file("ui/qml/PasswordGenerator.qml")
        .qml_file("ui/qml/FolderEditor.qml")
        .qml_file("ui/qml/FolderAppearancePicker.qml")
        .qml_file("ui/qml/FolderIcon.qml")
        .qml_file("ui/qml/FolderPicker.qml")
        .qml_file("ui/qml/MoveEntryDialog.qml")
        .qml_file("ui/qml/DeleteFolderDialog.qml")
        .qml_file("ui/qml/DeleteEntryDialog.qml")
        .qml_file("ui/qml/AboutDialog.qml")
        .qml_file("ui/qml/KeyErrorDialog.qml")
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
            "src/ui_adapters/secrets_controller.rs",
        ])
        .build();
}
