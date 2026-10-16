#![cfg(unix)]

use std::{
    error::Error,
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new() -> Result<Self, Box<dyn Error>> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        for attempt in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "kryptopass-android-doctor-{}-{timestamp}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot allocate test directory",
        )
        .into())
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn install_tool_trap(directory: &Path, tool: &str) -> io::Result<()> {
    let path = directory.join(tool);
    fs::write(
        &path,
        "#!/bin/sh\nprintf '%s\\n' \"$0\" >> \"$ANDROID_TEST_TOOL_LOG\"\nexit 97\n",
    )?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[test]
fn android_doctor_reports_missing_qt_before_starting_build_tools() -> Result<(), Box<dyn Error>> {
    let temporary = TemporaryDirectory::new()?;
    let tools = temporary.0.join("bin");
    fs::create_dir(&tools)?;
    for tool in ["git", "cmake"] {
        install_tool_trap(&tools, tool)?;
    }

    let qt = temporary.0.join("missing-qt");
    let tool_log = temporary.0.join("unexpected-tool-invocation");
    let target = temporary.0.join("target");
    let mut command = Command::new(env!("CARGO_BIN_EXE_kryptopass-xtask"));
    command
        .args(["android", "doctor"])
        .arg("--qt")
        .arg(&qt)
        .current_dir(&temporary.0)
        .env("HOME", &temporary.0)
        .env("PATH", &tools)
        .env("CARGO_TARGET_DIR", &target)
        .env("ANDROID_TEST_TOOL_LOG", &tool_log);
    for (option, directory) in [
        ("--sdk", "missing-sdk"),
        ("--ndk", "missing-ndk"),
        ("--host-qt", "missing-host-qt"),
        ("--java", "missing-java"),
    ] {
        command.arg(option).arg(temporary.0.join(directory));
    }

    let output = command.output()?;
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!output.status.success(), "doctor succeeded: {diagnostic}");
    assert!(
        diagnostic.contains("missing prerequisite"),
        "missing prerequisite diagnostic was absent: {diagnostic}"
    );
    assert!(
        diagnostic.contains(qt.to_string_lossy().as_ref()),
        "diagnostic omitted the requested Qt path: {diagnostic}"
    );
    assert!(
        !tool_log.exists(),
        "doctor invoked git or cmake: {diagnostic}"
    );
    assert!(
        !target.exists(),
        "doctor created the private Cargo target directory: {diagnostic}"
    );
    Ok(())
}
