use cxx_qt_build::{CxxQtBuilder, QmlModule};
use std::{env, fs, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("CARGO_FEATURE_DEMO").is_none() {
        return Ok(());
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo OUT_DIR is missing")?);
    for source in [
        "../../Cargo.toml",
        "../../packaging/gen_identity.py",
        "../../packaging/metadata.py",
    ] {
        println!("cargo:rerun-if-changed={source}");
    }
    if !Command::new("python3")
        .args(["../../packaging/gen_identity.py", &out.to_string_lossy()])
        .status()?
        .success()
    {
        return Err("Could not generate application identity".into());
    }
    let uri = format!(
        "{}.previewbackend",
        fs::read_to_string(out.join("app_id"))?.trim()
    );
    let main = fs::read_to_string("../desktop/ui/qml/Main.qml")?;
    println!("cargo:rerun-if-changed=../desktop/ui/qml/Main.qml");
    let mut shortcuts = std::collections::BTreeSet::new();
    for tail in main.split("root.shortcuts.").skip(1) {
        let name: String = tail
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .collect();
        shortcuts.insert(format!("\"{name}\":[]"));
    }
    fs::write(
        out.join("shortcuts.json"),
        format!(
            "{{{}}}",
            shortcuts.into_iter().collect::<Vec<_>>().join(",")
        ),
    )?;
    CxxQtBuilder::new_qml_module(QmlModule::new(uri.as_str()))
        .qt_module("Quick")
        .files([
            "src/demo/startup.rs",
            "src/demo/app_shell.rs",
            "src/demo/secrets_controller.rs",
            "src/demo/keyring_controller.rs",
            "src/demo/contacts_controller.rs",
            "src/demo/backup_controller.rs",
            "src/demo/import_controller.rs",
            "src/demo/sensitive_clipboard.rs",
            "src/demo/markdown_document.rs",
        ])
        .cpp_file("src/native/runtime.cpp")
        .include_dir("src")
        .build();
    Ok(())
}
