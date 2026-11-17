use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=../../packaging/gen_identity.py");
    println!("cargo:rerun-if-changed=../../packaging/metadata.py");
    println!("cargo:rerun-if-changed=../../packaging/icons");
    println!("cargo:rerun-if-changed=../../po");
    println!("cargo:rerun-if-changed=../../packaging/translations.py");
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
        .qml_files(qml_files()?)
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
            "src/ui_adapters/search_controller.rs",
            "src/ui_adapters/backup_controller.rs",
            "src/ui_adapters/import_controller.rs",
            "src/ui_adapters/sensitive_clipboard.rs",
            "src/ui_adapters/contacts_controller.rs",
            "src/ui_adapters/peers_controller.rs",
            "src/ui_adapters/native_ffi.rs",
            "src/ui_adapters/application.rs",
        ])
        .qrc(generated.join("translations.qrc"))
        .qrc(generated.join("app_resources.qrc"))
        .cpp_files(native_cpp_files()?);
    // Adds include paths and KDE compiler settings; generated CXX-Qt settings remain intact.
    unsafe {
        builder.cc_builder(|cc| {
            cc.include("src");
            cc.include(&generated);
            for include in &kde_includes {
                cc.include(include);
            }
            kde_compiler_settings(cc);
        })
    }
    .build();
    Ok(())
}

/// C++ warnings from ECM's `KDECompilerSettings`, which does not cover sources compiled by
/// `cc`. The same build also compiles the C++ that CXX-Qt and qmlcachegen generate.
const KDE_WARNINGS: &[&str] = &[
    "-Wall",
    "-Wextra",
    "-Wcast-align",
    "-Wchar-subscripts",
    "-Wformat-security",
    "-Wno-long-long",
    "-Wpointer-arith",
    "-Wundef",
    "-Wnon-virtual-dtor",
    "-Woverloaded-virtual",
    "-Winit-self",
    "-Wreturn-type",
    "-Wvla",
    "-Wdate-time",
    "-Wsuggest-override",
    "-pedantic",
    "-Wzero-as-null-pointer-constant",
    "-Wmissing-include-dirs",
];

/// Qt API restrictions that ECM's `KDECompilerSettings` enables for KDE code.
const KDE_DEFINES: &[&str] = &[
    "QT_NO_CAST_TO_ASCII",
    "QT_NO_CAST_FROM_ASCII",
    "QT_NO_URL_CAST_FROM_STRING",
    "QT_NO_CAST_FROM_BYTEARRAY",
    "QT_USE_QSTRINGBUILDER",
    "QT_NO_NARROWING_CONVERSIONS_IN_CONNECT",
    "QT_NO_KEYWORDS",
    "QT_NO_FOREACH",
    "QT_STRICT_ITERATORS",
];

fn kde_compiler_settings(cc: &mut cc::Build) {
    for flag in KDE_WARNINGS {
        cc.flag(flag);
    }
    // GCC-only in ECM; Clang rejects the option.
    cc.flag_if_supported("-Wlogical-op");
    // qmlcachegen emits `Q_GLOBAL_STATIC(Type, name)`, which C++17 `-pedantic` reports.
    cc.flag_if_supported("-Wno-c++20-extensions");
    for define in KDE_DEFINES {
        cc.define(define, None);
    }
    // Development builds fail on any warning. Release builds tolerate warnings that a
    // distribution's newer compiler may introduce.
    if env::var("PROFILE").as_deref() == Ok("debug") {
        cc.warnings_into_errors(true);
    }
}

/// Every component in `ui/qml`, sorted. A file declaring `pragma Singleton` is
/// registered as a singleton.
fn qml_files() -> Result<Vec<QmlFile>, Box<dyn std::error::Error>> {
    // Rebuilds the module when a component is added or removed.
    println!("cargo:rerun-if-changed=ui/qml");
    sources("ui/qml", "qml")?
        .into_iter()
        .map(|path| {
            let singleton = fs::read_to_string(&path)?
                .lines()
                .any(|line| line.trim() == "pragma Singleton");
            Ok(QmlFile::from(path).singleton(singleton))
        })
        .collect()
}

/// Every `.cpp` in `src/native`, plus the headers that declare a `Q_OBJECT`, which moc
/// must process.
fn native_cpp_files() -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=src/native");
    let mut files = sources("src/native", "cpp")?;
    for header in sources("src/native", "h")? {
        if fs::read_to_string(&header)?.contains("Q_OBJECT") {
            files.push(header);
        }
    }
    files.sort();
    Ok(files)
}

/// Files with `extension` directly in `directory`, sorted, as paths relative to the crate.
fn sources(directory: &str, extension: &str) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|found| found == extension) {
            files.push(Path::new(directory).join(path.file_name().ok_or("unnamed source")?));
        }
    }
    files.sort();
    Ok(files)
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
