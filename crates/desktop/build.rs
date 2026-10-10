use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
use std::{env, fs, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let kde_includes = kde_dependencies()?;
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
        .qml_file("ui/qml/BackupDialog.qml")
        .qml_file("ui/qml/PasswordGenerator.qml")
        .qml_file("ui/qml/FolderEditor.qml")
        .qml_file("ui/qml/FolderAppearancePicker.qml")
        .qml_file("ui/qml/FolderIcon.qml")
        .qml_file(QmlFile::from("ui/qml/FolderPalette.qml").singleton(true))
        .qml_file("ui/qml/FolderPicker.qml")
        .qml_file("ui/qml/MoveEntryDialog.qml")
        .qml_file("ui/qml/DeleteFolderDialog.qml")
        .qml_file("ui/qml/DeleteEntryDialog.qml")
        .qml_file("ui/qml/AboutDialog.qml")
        .qml_file("ui/qml/KeyErrorDialog.qml")
        .qml_file("ui/qml/MockStore.qml")
        .qml_file("ui/qml/KeyStore.qml")
        .qml_file("ui/qml/KeyDetail.qml")
        .qml_file("ui/qml/KeyOperationDialog.qml")
        .qml_file("ui/qml/ToastHost.qml")
        .qml_file("ui/qml/KeyContactPanel.qml")
        .qml_file("ui/qml/KeyContactsOverview.qml")
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

    let builder = CxxQtBuilder::new_qml_module(qml_module)
        // Qt QML requires Qt Network on some platforms, including macOS.
        .qt_module("Network")
        .qt_module("Widgets")
        .files([
            "src/ui_adapters/app_shell.rs",
            "src/ui_adapters/keyring_controller.rs",
            "src/ui_adapters/markdown_document.rs",
            "src/ui_adapters/secrets_controller.rs",
            "src/ui_adapters/backup_controller.rs",
            "src/ui_adapters/sensitive_clipboard.rs",
            "src/ui_adapters/contacts_controller.rs",
            "src/ui_adapters/native_ffi.rs",
            "src/ui_adapters/application.rs",
        ])
        .cpp_file("src/native/application.cpp")
        .cpp_file("src/native/sensitive_clipboard.cpp")
        .cpp_file("src/native/kde_contacts.cpp");
    // Only adds include paths; generated CXX-Qt settings remain intact.
    unsafe {
        builder.cc_builder(|cc| {
            cc.include("src");
            for include in &kde_includes {
                cc.include(include);
            }
        })
    }
    .build();
    Ok(())
}

fn kde_dependencies() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=native-deps/CMakeLists.txt");
    for variable in ["CMAKE_PREFIX_PATH", "CMAKE_TOOLCHAIN_FILE"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo OUT_DIR is missing")?)
        .join("kde-dependencies");
    let mut configure = Command::new("cmake");
    configure
        .arg("-S")
        .arg("native-deps")
        .arg("-B")
        .arg(&output);
    if let Some(prefix) = env::var_os("CMAKE_PREFIX_PATH") {
        configure.arg(format!("-DCMAKE_PREFIX_PATH={}", prefix.to_string_lossy()));
    }
    if let Some(toolchain) = env::var_os("CMAKE_TOOLCHAIN_FILE") {
        configure.arg(format!(
            "-DCMAKE_TOOLCHAIN_FILE={}",
            toolchain.to_string_lossy()
        ));
    }
    let status = configure.status()?;
    if !status.success() {
        return Err("KDE contacts development libraries are required".into());
    }
    let libraries = fs::read_to_string(output.join("libraries.txt"))?;
    for library in libraries.lines().filter(|line| !line.is_empty()) {
        println!("cargo:rustc-link-arg={library}");
    }
    Ok(fs::read_to_string(output.join("includes.txt"))?
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}
