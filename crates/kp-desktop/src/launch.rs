//! What a launch asked for, as `QCommandLineParser` read it in `native/command_line.cpp`.

use crate::ui_adapters::native_ffi::ffi;
use cxx_qt_lib::{QString, QStringList};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    pub new_entry: bool,
    pub search: bool,
    pub devices: bool,
    pub new_instance: bool,
    pub browser_integration: bool,
    /// File the window capture is saved to.
    pub screenshot: Option<String>,
    /// Absolute paths of the positional arguments.
    pub files: Vec<String>,
}

impl LaunchOptions {
    /// Browser hosts, screenshots and isolated profiles run beside the user's instance.
    pub fn separate_instance(&self) -> bool {
        self.browser_integration || self.new_instance || self.screenshot.is_some()
    }

    /// The `DesktopAction` id of the launcher action requested; empty when none.
    pub fn action_id(&self) -> &'static str {
        if self.new_entry {
            "newEntry"
        } else if self.search {
            "find"
        } else {
            ""
        }
    }

    fn from_json(json: &QString) -> Self {
        serde_json::from_str(&json.to_string()).unwrap_or_else(|error| {
            tracing::error!(%error, "Could not read the parsed command line");
            Self::default()
        })
    }
}

static STARTUP: OnceLock<LaunchOptions> = OnceLock::new();

/// Parses this process's arguments once. `--help`, `--version` and usage errors exit
/// inside; call it after `initialize_desktop` and before registering the single instance.
pub fn process() -> &'static LaunchOptions {
    STARTUP.get_or_init(|| LaunchOptions::from_json(&ffi::process_command_line()))
}

/// The options this process started with; empty before [`process`].
pub fn startup() -> &'static LaunchOptions {
    static EMPTY: LaunchOptions = LaunchOptions {
        new_entry: false,
        search: false,
        devices: false,
        new_instance: false,
        browser_integration: false,
        screenshot: None,
        files: Vec::new(),
    };
    STARTUP.get().unwrap_or(&EMPTY)
}

/// Options of a later launch forwarded by the single-instance service.
pub fn forwarded(arguments: &QStringList, working_directory: &QString) -> LaunchOptions {
    LaunchOptions::from_json(&ffi::parse_forwarded_command_line(
        arguments,
        working_directory,
    ))
}
