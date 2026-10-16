//! Explicit UI demonstration controller; no real vault or operating-system adapter.
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, bridge_status, cxx_name = "bridgeStatus", READ, NOTIFY)]
        #[qproperty(QString, active_section, cxx_name = "activeSection", READ, NOTIFY)]
        #[qproperty(i32, health_checks, cxx_name = "healthChecks", READ, NOTIFY)]
        #[qproperty(bool, bridge_ready, cxx_name = "bridgeReady", READ, NOTIFY)]
        #[qproperty(QString, app_version, cxx_name = "appVersion", READ, NOTIFY)]
        #[qproperty(QString, app_name, cxx_name = "appName", READ, NOTIFY)]
        #[qproperty(QString, app_description, cxx_name = "appDescription", READ, NOTIFY)]
        #[qproperty(QString, app_license, cxx_name = "appLicense", READ, NOTIFY)]
        #[qproperty(QString, app_authors, cxx_name = "appAuthors", READ, NOTIFY)]
        #[qproperty(QString, app_copyright, cxx_name = "appCopyright", READ, NOTIFY)]
        #[qproperty(QString, app_homepage, cxx_name = "appHomepage", READ, NOTIFY)]
        #[qproperty(QString, app_repository, cxx_name = "appRepository", READ, NOTIFY)]
        #[qproperty(QString, shortcuts_json, cxx_name = "shortcutsJson", READ, NOTIFY)]
        #[qproperty(
            QString,
            capabilities_json,
            cxx_name = "capabilitiesJson",
            READ,
            CONSTANT
        )]
        type ShellController = super::ShellControllerRust;
        #[qinvokable]
        #[cxx_name = "configureShortcuts"]
        fn configure_shortcuts(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "runHealthCheck"]
        fn run_health_check(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "selectSection"]
        fn select_section(self: Pin<&mut Self>, section: &QString);
        #[qinvokable]
        #[cxx_name = "resetDemo"]
        fn reset_demo(self: Pin<&mut Self>);
    }
}

pub struct ShellControllerRust {
    capabilities_json: QString,
    bridge_status: QString,
    active_section: QString,
    health_checks: i32,
    bridge_ready: bool,
    app_version: QString,
    app_name: QString,
    app_description: QString,
    app_license: QString,
    app_authors: QString,
    app_copyright: QString,
    app_homepage: QString,
    app_repository: QString,
    shortcuts_json: QString,
}

impl qobject::ShellController {
    fn publish_bridge_status(mut self: Pin<&mut Self>, value: QString) {
        if self.bridge_status() != &value {
            self.as_mut().rust_mut().bridge_status = value;
            self.bridge_status_changed();
        }
    }
    fn publish_active_section(mut self: Pin<&mut Self>, value: QString) {
        if self.active_section() != &value {
            self.as_mut().rust_mut().active_section = value;
            self.active_section_changed();
        }
    }
    fn publish_health_checks(mut self: Pin<&mut Self>, value: i32) {
        if self.health_checks() != &value {
            self.as_mut().rust_mut().health_checks = value;
            self.health_checks_changed();
        }
    }
    fn publish_bridge_ready(mut self: Pin<&mut Self>, value: bool) {
        if self.bridge_ready() != &value {
            self.as_mut().rust_mut().bridge_ready = value;
            self.bridge_ready_changed();
        }
    }
    pub fn configure_shortcuts(self: Pin<&mut Self>) {
        self.publish_bridge_status(QString::from(super::UNAVAILABLE));
    }
    pub fn run_health_check(mut self: Pin<&mut Self>) {
        let count = self.health_checks().saturating_add(1);
        self.as_mut().publish_health_checks(count);
        self.as_mut().publish_bridge_ready(true);
        self.publish_bridge_status(QString::from(format!(
            "Puente Rust/QML verificado · prueba #{count}"
        )));
    }
    pub fn select_section(self: Pin<&mut Self>, section: &QString) {
        self.publish_active_section(section.clone());
    }
    pub fn reset_demo(mut self: Pin<&mut Self>) {
        self.as_mut().publish_health_checks(0);
        self.as_mut().publish_active_section(QString::from("home"));
        self.as_mut().publish_bridge_ready(true);
        self.publish_bridge_status(QString::from("Backend Rust conectado"));
    }
}

impl Default for ShellControllerRust {
    fn default() -> Self {
        Self {
            capabilities_json: QString::from(
                r#"{"demo":true,"read":true,"vaults":false,"mutations":false,"clipboard":false,"keyring":false,"contacts":false,"import":false,"backup":false,"browser":false}"#,
            ),
            bridge_status: QString::from("Backend Rust conectado"),
            active_section: QString::from("home"),
            health_checks: 0,
            bridge_ready: true,
            app_version: QString::from(crate::app_info::APP.version),
            app_name: QString::from(crate::app_info::APP.display_name),
            app_description: QString::from(crate::app_info::APP.description),
            app_license: QString::from(crate::app_info::APP.license),
            app_authors: QString::from(crate::app_info::APP.authors),
            app_copyright: QString::from(crate::app_info::APP.copyright),
            app_homepage: QString::from(crate::app_info::APP.homepage),
            app_repository: QString::from(crate::app_info::APP.repository),
            shortcuts_json: QString::from(include_str!(concat!(
                env!("OUT_DIR"),
                "/shortcuts.json"
            ))),
        }
    }
}
