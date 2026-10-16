//! Workspace commands that compose Cargo, CXX-Qt, and Qt's Android packaging tools.

use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const KIRIGAMI_TAG: &str = "v6.30.0";
const KIRIGAMI_COMMIT: &str = "33110f03299d7dfdea6735a1dff5df6858363ceb";
const QT_VERSION: &str = "6.12.0";
const NDK_VERSION: &str = "27.2.12479018";
const USAGE: &str = "cargo xtask android <build|doctor|apk-check|run|smoke|versions> [--qt PATH --sdk PATH --ndk PATH --host-qt PATH --java PATH --ecm PATH]\nBuild/doctor/apk-check accept paths or QT_ANDROID_ROOT, ANDROID_SDK_ROOT, ANDROID_NDK_ROOT, QT_HOST_PATH, JAVA_HOME, ECM_DIR. Run/smoke use host Qt only.";

fn main() {
    if let Err(error) = execute() {
        eprintln!("xtask: {error}");
        std::process::exit(1);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command.current_dir(root());
    command.env("LANG", "C.UTF-8").env("LC_ALL", "C.UTF-8");
    command
}

fn run(command: &mut Command) -> Result<()> {
    println!(
        "Running: {} {}",
        command.get_program().to_string_lossy(),
        command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let status = command.status()?;
    if !status.success() {
        return Err(format!("command failed with {status}").into());
    }
    Ok(())
}

fn query(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn path_from_env(name: &str, fallback: PathBuf) -> PathBuf {
    env::var_os(name).map(PathBuf::from).unwrap_or(fallback)
}

fn java_home() -> Result<PathBuf> {
    if let Some(home) = env::var_os("JAVA_HOME") {
        return Ok(home.into());
    }
    let studio = PathBuf::from("/opt/android-studio/jbr");
    if studio.join("bin/java").is_file() {
        return Ok(studio);
    }
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let java = directory.join("java");
        if java.is_file() {
            let java = java.canonicalize()?;
            if let Some(home) = java.parent().and_then(Path::parent) {
                return Ok(home.to_owned());
            }
        }
    }
    Err("set JAVA_HOME to a JDK 21 or newer".into())
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() != Some(contents) {
        fs::write(path, contents)?;
    }
    Ok(())
}

struct Android {
    qt: PathBuf,
    sdk: PathBuf,
    ndk: PathBuf,
    host_qt: PathBuf,
    java: PathBuf,
    ecm: PathBuf,
}

impl Android {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self> {
        let home = PathBuf::from(env::var_os("HOME").ok_or("HOME is missing")?);
        let mut config = Self {
            qt: path_from_env(
                "QT_ANDROID_ROOT",
                home.join(format!(
                    ".cache/kryptopass-qt/{QT_VERSION}/android_arm64_v8a"
                )),
            ),
            sdk: path_from_env("ANDROID_SDK_ROOT", home.join("Android/Sdk")),
            ndk: path_from_env("ANDROID_NDK_ROOT", PathBuf::new()),
            host_qt: path_from_env("QT_HOST_PATH", PathBuf::from("/usr")),
            java: PathBuf::new(),
            ecm: path_from_env("ECM_DIR", PathBuf::from("/usr/share/ECM/cmake")),
        };
        let mut args = args;
        while let Some(flag) = args.next() {
            let value = PathBuf::from(
                args.next()
                    .ok_or_else(|| format!("{flag} requires a path"))?,
            );
            match flag.as_str() {
                "--qt" => config.qt = value,
                "--sdk" => config.sdk = value,
                "--ndk" => config.ndk = value,
                "--host-qt" => config.host_qt = value,
                "--java" => config.java = value,
                "--ecm" => config.ecm = value,
                _ => return Err(format!("unknown option {flag}").into()),
            }
        }
        if config.ndk.as_os_str().is_empty() {
            config.ndk = config.sdk.join(format!("ndk/{NDK_VERSION}"));
        }
        Ok(config)
    }

    fn qmake(&self) -> PathBuf {
        let qt6 = self.host_qt.join("bin/qmake6");
        if qt6.is_file() {
            qt6
        } else {
            self.host_qt.join("bin/qmake")
        }
    }

    fn doctor(&mut self) -> Result<()> {
        for path in [
            self.qt.join("lib/cmake/Qt6/qt.toolchain.cmake"),
            self.ndk.join("build/cmake/android.toolchain.cmake"),
            self.sdk.join("platforms/android-36/android.jar"),
            self.sdk.join("build-tools/36.0.0/aapt2"),
            self.qmake(),
            self.ecm.join("ECMConfig.cmake"),
        ] {
            if !path.is_file() {
                return Err(format!("missing prerequisite: {}", path.display()).into());
            }
        }
        if self.java.as_os_str().is_empty() {
            self.java = java_home()?;
        }
        let java = query(command(self.java.join("bin/java")).arg("--version"))?;
        let major = java
            .split_whitespace()
            .nth(1)
            .and_then(|version| version.split('.').next())
            .and_then(|major| major.parse::<u32>().ok());
        if major.is_none_or(|major| major < 21) {
            return Err(format!(
                "JDK 21 or newer required; --java points to {} ({java})",
                self.java.display()
            )
            .into());
        }
        let host_version = query(command(self.qmake()).args(["-query", "QT_VERSION"]))?;
        let target_config = fs::read_to_string(self.qt.join("mkspecs/qconfig.pri"))?;
        let target_version = target_config
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "QT_VERSION").then(|| value.trim())
            })
            .ok_or("QT_VERSION missing from Android mkspecs/qconfig.pri")?;
        if host_version != target_version {
            return Err(format!("Qt host {host_version} differs from Android {target_version}; set --host-qt to the matching Qt installation").into());
        }
        for (tool, flag) in [
            ("git", "--version"),
            ("cmake", "--version"),
            ("ninja", "--version"),
            ("python3", "--version"),
        ] {
            query(command(tool).arg(flag))?;
        }
        let targets = query(command("rustup").args(["target", "list", "--installed"]))?;
        if !targets
            .lines()
            .any(|target| target == "aarch64-linux-android")
        {
            return Err("run rustup target add aarch64-linux-android".into());
        }
        let ndk = fs::read_to_string(self.ndk.join("source.properties"))?;
        let revision = ndk
            .lines()
            .find_map(|line| line.strip_prefix("Pkg.Revision = "))
            .ok_or("NDK revision missing")?;
        if revision != NDK_VERSION {
            eprintln!(
                "warning: NDK {revision} overrides Qt {QT_VERSION}'s reference NDK {NDK_VERSION}; validate the resulting native libraries"
            );
        }
        println!(
            "Android environment: Qt {target_version}, NDK {revision}, {}",
            java.lines().next().unwrap_or("")
        );
        println!(
            "Qt: {}\nHost Qt: {}\nSDK: {}\nJDK: {}",
            self.qt.display(),
            self.host_qt.display(),
            self.sdk.display(),
            self.java.display()
        );
        Ok(())
    }

    fn build(mut self) -> Result<()> {
        self.doctor()?;
        let root = root();
        let deps = root.join("target/android-ui-deps");
        let build = root.join("target/android-ui");
        fs::create_dir_all(&deps)?;
        let source = deps.join("kirigami");
        if !source.exists() {
            run(command("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "--branch",
                    KIRIGAMI_TAG,
                    "https://invent.kde.org/frameworks/kirigami.git",
                ])
                .arg(&source))?;
        }
        let revision = query(
            command("git")
                .arg("-C")
                .arg(&source)
                .args(["rev-parse", "HEAD"]),
        )?;
        if revision != KIRIGAMI_COMMIT {
            return Err(format!(
                "Kirigami {KIRIGAMI_TAG} must resolve to {KIRIGAMI_COMMIT}; found {revision} in {}",
                source.display()
            )
            .into());
        }
        let prefix = deps.join("kirigami-install");
        let shared = [
            format!(
                "-DCMAKE_TOOLCHAIN_FILE={}/lib/cmake/Qt6/qt.toolchain.cmake",
                self.qt.display()
            ),
            format!("-DQT_HOST_PATH={}", self.host_qt.display()),
            format!("-DANDROID_SDK_ROOT={}", self.sdk.display()),
            format!("-DANDROID_NDK_ROOT={}", self.ndk.display()),
            "-DANDROID_ABI=arm64-v8a".into(),
            "-DANDROID_PLATFORM=android-28".into(),
            "-DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON".into(),
        ];
        let kirigami_build = deps.join("kirigami-build");
        let mut configure = command("cmake");
        configure
            .arg("-S")
            .arg(&source)
            .arg("-B")
            .arg(&kirigami_build)
            .arg("-GNinja")
            .args(&shared)
            .arg(format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display()))
            .arg(format!("-DECM_DIR={}", self.ecm.display()))
            .args([
                "-DCMAKE_BUILD_TYPE=Release",
                "-DBUILD_TESTING=OFF",
                "-DDESKTOP_ENABLED=OFF",
                "-DBUILD_EXAMPLES=OFF",
                "-DCMAKE_DISABLE_FIND_PACKAGE_OpenMP=ON",
            ]);
        self.configure(&mut configure, &kirigami_build)?;
        run(command("cmake")
            .arg("--build")
            .arg(&kirigami_build)
            .args(["--parallel", "4"]))?;
        run(command("cmake").arg("--install").arg(&kirigami_build))?;

        // Query Android headers/libraries with the SDK's qt.conf format; execute tools on the host.
        let qmake = self.qmake();
        let host_tools = query(command(&qmake).args(["-query", "QT_HOST_BINS"]))?;
        let host_libexec = query(command(&qmake).args(["-query", "QT_HOST_LIBEXECS"]))?;
        let qt_conf = deps.join("rust-target-qt.conf");
        write_if_changed(
            &qt_conf,
            &format!(
                "[Paths]\nPrefix={}\nHostPrefix={}\nHostBinaries={host_tools}\nHostLibraryExecutables={host_libexec}\nTargetSpec=android-clang\nHostSpec=linux-g++\n",
                self.qt.display(),
                self.host_qt.display()
            ),
        )?;
        let wrapper = deps.join("rust-target-qmake");
        write_if_changed(
            &wrapper,
            &format!(
                "#!/bin/sh\nexec {} -qtconf {} \"$@\"\n",
                shell_quote(&qmake),
                shell_quote(&qt_conf)
            ),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755))?;
        }
        let qml = if prefix.join("lib/qml").exists() {
            prefix.join("lib/qml")
        } else {
            prefix.join("qml")
        };
        let mut configure = command("cmake");
        configure
            .args(["-S", "packaging/android-ui", "-B"])
            .arg(&build)
            .arg("-GNinja")
            .args(&shared)
            .args([
                "-DCMAKE_BUILD_TYPE=Debug",
                "-DRust_CARGO_TARGET=aarch64-linux-android",
            ])
            .arg(format!("-DPREVIEW_QMAKE={}", wrapper.display()))
            .arg(format!("-DCMAKE_PREFIX_PATH={}", prefix.display()))
            .arg(format!(
                "-DKF6Kirigami_DIR={}/lib/cmake/KF6Kirigami",
                prefix.display()
            ))
            .arg(format!(
                "-DKF6KirigamiPlatform_DIR={}/lib/cmake/KF6KirigamiPlatform",
                prefix.display()
            ))
            .arg(format!("-DKIRIGAMI_QML_PATH={}", qml.display()));
        self.configure(&mut configure, &build)?;
        run(command("cmake")
            .arg("--build")
            .arg(&build)
            .args(["--target", "apk", "--parallel", "4"])
            .env("JAVA_HOME", &self.java)
            .env("ANDROID_SDK_ROOT", &self.sdk))?;
        let apk = build.join("android-build/build/outputs/apk/debug/android-build-debug.apk");
        let artifact = root.join("target/kryptopass-ui-android-debug.apk");
        fs::copy(apk, &artifact)?;
        self.check_apk()?;
        println!("APK: {}", artifact.display());
        Ok(())
    }

    fn check_apk(&self) -> Result<()> {
        run(command("python3")
            .args([
                "packaging/android-ui/check_apk.py",
                "target/kryptopass-ui-android-debug.apk",
                "--ndk",
            ])
            .arg(&self.ndk)
            .arg("--sdk")
            .arg(&self.sdk))
    }

    fn configure(&self, configure: &mut Command, build: &Path) -> Result<()> {
        if let Ok(cache) = fs::read_to_string(build.join("CMakeCache.txt")) {
            for (key, value) in [
                ("ANDROID_NDK_ROOT", self.ndk.clone()),
                ("QT_HOST_PATH", self.host_qt.clone()),
                (
                    "CMAKE_TOOLCHAIN_FILE",
                    self.qt.join("lib/cmake/Qt6/qt.toolchain.cmake"),
                ),
            ] {
                let previous = cache.lines().find_map(|line| {
                    let (name, value) = line.split_once('=')?;
                    (name.split(':').next() == Some(key)).then_some(value)
                });
                if previous.is_some_and(|previous| previous != value.to_string_lossy()) {
                    println!(
                        "Toolchain changed ({key}); refreshing the generated CMake cache in {}",
                        build.display()
                    );
                    configure.arg("--fresh");
                    break;
                }
            }
        }
        run(configure)
    }
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn build_host() -> Result<()> {
    run(command("cmake").args([
        "-S",
        "packaging/android-ui",
        "-B",
        "target/android-ui-host",
        "-GNinja",
    ]))?;
    run(command("cmake").args(["--build", "target/android-ui-host", "--parallel", "2"]))?;
    Ok(())
}

fn run_host() -> Result<()> {
    build_host()?;
    let mut app = command("target/android-ui-host/kryptopass_ui_preview");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Let bacon terminate the running application directly before rebuilding.
        Err(app.exec().into())
    }
    #[cfg(not(unix))]
    run(&mut app)
}

fn smoke() -> Result<()> {
    build_host()?;
    let output = command("timeout")
        .args(["30s", "target/android-ui-host/kryptopass_ui_preview"])
        .arg("--smoke")
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QUICK_BACKEND", "software")
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .output()?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{log}");
    if !output.status.success()
        || !log.contains("QML -> Rust loadStore; metadata published")
        || !log.contains("bridge-smoke: PASS")
        || [
            "QQmlApplicationEngine failed",
            "Cannot assign",
            "ReferenceError",
            "TypeError",
            "not installed",
            "bridge-smoke: FAIL",
        ]
        .iter()
        .any(|error| log.contains(error))
    {
        return Err("Rust/QML smoke failed".into());
    }
    println!(
        "smoke: Rust startup, QML signals, selection, reveal, lock and unsupported operations verified"
    );
    Ok(())
}

fn execute() -> Result<()> {
    let mut args = env::args().skip(1);
    if env::args()
        .skip(1)
        .any(|arg| arg == "--help" || arg == "-h")
    {
        println!("{USAGE}");
        return Ok(());
    }
    if args.next().as_deref() != Some("android") {
        return Err(USAGE.into());
    }
    match args.next().as_deref() {
        Some("build") => Android::parse(args)?.build(),
        Some("doctor") => Android::parse(args)?.doctor(),
        Some("apk-check") => Android::parse(args)?.check_apk(),
        Some("run") if args.next().is_none() => run_host(),
        Some("smoke") if args.next().is_none() => smoke(),
        Some("versions") if args.next().is_none() => {
            println!("QT_VERSION={QT_VERSION}\nNDK_VERSION={NDK_VERSION}");
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}
