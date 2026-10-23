use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
use std::{env, fs, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=../../packaging/gen_identity.py");
    println!("cargo:rerun-if-changed=../../packaging/metadata.py");
    println!("cargo:rerun-if-changed=../../packaging/icons");
    println!("cargo:rerun-if-changed=../../po");
    println!("cargo:rerun-if-changed=../../packaging/translations.py");
    println!("cargo:rerun-if-changed=../../scripts/prepare_synthetic_sync_fixture.py");
    println!("cargo:rerun-if-env-changed=KP_SYNTHETIC_FIXTURE_DIR");
    println!("cargo:rerun-if-env-changed=XDG_CACHE_HOME");
    println!("cargo:rerun-if-env-changed=HOME");
    let generated = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo OUT_DIR is missing")?);
    for (script, args) in [
        (
            "gen_identity.py",
            vec![generated.to_string_lossy().into_owned()],
        ),
        (
            "translations.py",
            vec![
                "compile".into(),
                "--output".into(),
                generated.to_string_lossy().into_owned(),
            ],
        ),
    ] {
        if !Command::new("python3")
            .arg(format!("../../packaging/{script}"))
            .args(args)
            .status()?
            .success()
        {
            return Err(format!("{script} failed").into());
        }
    }
    let manifest_directory = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").ok_or("Cargo manifest directory is missing")?,
    );
    let fixture_directory = synthetic_fixture_directory()?;
    let fixture_directory = if fixture_directory.is_absolute() {
        fixture_directory
    } else {
        manifest_directory.join(fixture_directory)
    };
    let fixture_manifest = fixture_directory.join("fixture.json");
    if env::var("PROFILE").as_deref() == Ok("debug") {
        let fixture_script =
            manifest_directory.join("../../scripts/prepare_synthetic_sync_fixture.py");
        let legacy_directory = manifest_directory.join("../../fixtures/synthetic-key-transfer-v1");
        let prepared = Command::new("python3")
            .arg(&fixture_script)
            .arg("--directory")
            .arg(&fixture_directory)
            .arg("--legacy-dir")
            .arg(legacy_directory)
            .output()?;
        if !prepared.status.success() {
            let message = String::from_utf8_lossy(&prepared.stderr);
            return Err(format!("synthetic fixture preparation failed: {}", message.trim()).into());
        }
        println!("cargo:rerun-if-changed={}", fixture_manifest.display());
        println!(
            "cargo:rerun-if-changed={}",
            fixture_directory.join("secret-key.pgp").display()
        );
    }
    let fixture_directory_text = fixture_directory
        .to_str()
        .ok_or("synthetic fixture path is not valid UTF-8")?;
    let fixture_manifest_text = fixture_manifest
        .to_str()
        .ok_or("synthetic fixture manifest path is not valid UTF-8")?;
    println!("cargo:rustc-env=KP_SYNTHETIC_FIXTURE_DIR={fixture_directory_text}");
    println!("cargo:rustc-env=KP_SYNTHETIC_FIXTURE_MANIFEST={fixture_manifest_text}");
    let kde_includes = kde_dependencies()?;
    let qml_module = QmlModule::new(fs::read_to_string(generated.join("app_id"))?.trim())
        .qml_file("ui/qml/Main.qml")
        .qml_file("ui/qml/LockScreen.qml")
        .qml_file("ui/qml/FirstRunScreen.qml")
        .qml_file("ui/qml/HelpButton.qml")
        .qml_file("ui/qml/DesktopAction.qml")
        .qml_file("ui/qml/AdaptivePanes.qml")
        .qml_file("ui/qml/BrowserPairingDialog.qml")
        .qml_file("ui/qml/DevicesWindow.qml")
        .qml_file("ui/qml/BrowserAccessDialog.qml")
        .qml_file("ui/qml/BrowserAccessWindow.qml")
        .qml_file("ui/qml/AppListView.qml")
        .qml_file("ui/qml/PanelHeader.qml")
        .qml_file("ui/qml/PanelFooter.qml")
        .qml_file("ui/qml/SplitterHandle.qml")
        .qml_file("ui/qml/NavigationItem.qml")
        .qml_file("ui/qml/SidebarPane.qml")
        .qml_file("ui/qml/EntryListPane.qml")
        .qml_file("ui/qml/EntryDetailPane.qml")
        .qml_file("ui/qml/EntryPickerDialog.qml")
        .qml_file("ui/qml/RelatedEntryPicker.qml")
        .qml_file("ui/qml/SecretEditor.qml")
        .qml_file("ui/qml/DraftBaseline.qml")
        .qml_file("ui/qml/SecretStore.qml")
        .qml_file("ui/qml/LoginFields.qml")
        .qml_file("ui/qml/NoteFields.qml")
        .qml_file("ui/qml/CustomFields.qml")
        .qml_file("ui/qml/CardFields.qml")
        .qml_file("ui/qml/TotpFields.qml")
        .qml_file("ui/qml/IdentityFields.qml")
        .qml_file("ui/qml/WifiFields.qml")
        .qml_file("ui/qml/LicenseFields.qml")
        .qml_file("ui/qml/SshKeyFields.qml")
        .qml_file("ui/qml/SecretInput.qml")
        .qml_file("ui/qml/StringListEditor.qml")
        .qml_file("ui/qml/SecretFields.qml")
        .qml_file("ui/qml/SecretFieldsHost.qml")
        .qml_file(QmlFile::from("ui/qml/EntryTypes.qml").singleton(true))
        .qml_file("ui/qml/SecretContentView.qml")
        .qml_file("ui/qml/LoginDetail.qml")
        .qml_file("ui/qml/NoteDetail.qml")
        .qml_file("ui/qml/StructuredSecretDetail.qml")
        .qml_file("ui/qml/SecretTextSection.qml")
        .qml_file("ui/qml/RecipientsChooser.qml")
        .qml_file("ui/qml/BackupWindow.qml")
        .qml_file("ui/qml/ImportWindow.qml")
        .qml_file("ui/qml/PasswordGenerator.qml")
        .qml_file("ui/qml/FolderEditor.qml")
        .qml_file("ui/qml/FolderAppearancePicker.qml")
        .qml_file("ui/qml/FolderIcon.qml")
        .qml_file("ui/qml/EntryIcon.qml")
        .qml_file(QmlFile::from("ui/qml/FolderPalette.qml").singleton(true))
        .qml_file("ui/qml/FolderPicker.qml")
        .qml_file("ui/qml/MoveEntryDialog.qml")
        .qml_file("ui/qml/MoveFolderDialog.qml")
        .qml_file("ui/qml/DeleteFolderDialog.qml")
        .qml_file("ui/qml/DeleteEntryDialog.qml")
        .qml_file("ui/qml/KeyErrorDialog.qml")
        .qml_file("ui/qml/KeyStore.qml")
        .qml_file("ui/qml/KeyDetail.qml")
        .qml_file("ui/qml/KeyOperationDialog.qml")
        .qml_file("ui/qml/OnlineServicesDialog.qml")
        .qml_file("ui/qml/ToastHost.qml")
        .qml_file("ui/qml/KeyContactPanel.qml")
        .qml_file("ui/qml/KeyContactsOverview.qml")
        .qml_file("ui/qml/KeyListDelegate.qml")
        .qml_file("ui/qml/KeyStatus.qml")
        .qml_file("ui/qml/KeyAvatar.qml")
        .qml_file("ui/qml/IdentityValidity.qml")
        .qml_file("ui/qml/SecretDetail.qml")
        .qml_file("ui/qml/DetailField.qml")
        .qml_file("ui/qml/MarkdownView.qml")
        .qml_file("ui/qml/MarkdownEditor.qml")
        .qml_file("ui/qml/NotesEditor.qml")
        .depends([
            "QtQuick",
            "QtQuick.Controls",
            "QtQuick.Layouts",
            "org.kde.kirigami",
        ]);

    let builder = CxxQtBuilder::new_qml_module(qml_module)
        // Qt QML requires Qt Network on some platforms, including macOS.
        .qt_module("Network")
        .qt_module("Quick")
        .qt_module("Widgets")
        .qt_module("DBus")
        .files([
            "src/ui_adapters/app_shell.rs",
            "src/ui_adapters/keyring_controller.rs",
            "src/ui_adapters/markdown_document.rs",
            "src/ui_adapters/secrets_controller.rs",
            "src/ui_adapters/backup_controller.rs",
            "src/ui_adapters/import_controller.rs",
            "src/ui_adapters/sensitive_clipboard.rs",
            "src/ui_adapters/contacts_controller.rs",
            "src/ui_adapters/pairing_lab_controller.rs",
            "src/ui_adapters/native_ffi.rs",
            "src/ui_adapters/application.rs",
        ])
        .qrc(generated.join("translations.qrc"))
        .qrc(generated.join("app_resources.qrc"))
        .cpp_file("src/native/application.cpp")
        .cpp_file("src/native/desktop_bus.h")
        .cpp_file("src/native/desktop_bus.cpp")
        .cpp_file("src/native/desktop_integration.cpp")
        .cpp_file("src/native/session_lock.h")
        .cpp_file("src/native/session_lock.cpp")
        .cpp_file("src/native/file_manager.cpp")
        .cpp_file("src/native/entry_icons.cpp")
        .cpp_file("src/native/entry_icon_image.cpp")
        .cpp_file("src/native/favicon_service.cpp")
        .cpp_file("src/native/sensitive_clipboard.cpp")
        .cpp_file("src/native/kde_contacts.cpp")
        .cpp_file("src/native/online_services.cpp");
    // Only adds include paths; generated CXX-Qt settings remain intact.
    unsafe {
        builder.cc_builder(|cc| {
            cc.include("src");
            cc.include(&generated);
            for include in &kde_includes {
                cc.include(include);
            }
        })
    }
    .build();
    Ok(())
}

fn synthetic_fixture_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(directory) = env::var_os("KP_SYNTHETIC_FIXTURE_DIR") {
        return Ok(PathBuf::from(directory));
    }
    let cache_home = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or("HOME is unavailable for the synthetic fixture cache")?;
    Ok(cache_home.join("kryptopass/synthetic-sync-v1"))
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
        return Err("KDE development libraries are required; see BUILD.md".into());
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
