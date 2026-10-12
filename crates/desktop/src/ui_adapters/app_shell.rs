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
        /// Shell demo state exposed to the QML presentation layer.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, bridge_status, cxx_name = "bridgeStatus")]
        #[qproperty(QString, active_section, cxx_name = "activeSection")]
        #[qproperty(i32, health_checks, cxx_name = "healthChecks")]
        #[qproperty(bool, bridge_ready, cxx_name = "bridgeReady")]
        /// Fixed at Default from Cargo.toml; no invokable mutates it.
        #[qproperty(QString, app_version, cxx_name = "appVersion", READ, CONSTANT)]
        #[qproperty(QString, app_name, cxx_name = "appName", READ, CONSTANT)]
        #[qproperty(QString, app_description, cxx_name = "appDescription", READ, CONSTANT)]
        #[qproperty(QString, app_license, cxx_name = "appLicense", READ, CONSTANT)]
        #[qproperty(QString, app_authors, cxx_name = "appAuthors", READ, CONSTANT)]
        #[qproperty(QString, app_copyright, cxx_name = "appCopyright", READ, CONSTANT)]
        #[qproperty(QString, app_homepage, cxx_name = "appHomepage", READ, CONSTANT)]
        #[qproperty(QString, app_repository, cxx_name = "appRepository", READ, CONSTANT)]
        #[qproperty(QString, shortcuts_json, cxx_name = "shortcutsJson", READ, NOTIFY)]
        type ShellController = super::ShellControllerRust;

        #[qinvokable]
        #[cxx_name = "configureShortcuts"]
        fn configure_shortcuts(self: Pin<&mut Self>);

        /// Records a bridge health check and publishes the verification status.
        #[qinvokable]
        #[cxx_name = "runHealthCheck"]
        fn run_health_check(self: Pin<&mut Self>);

        /// Applies the section requested by the QML shell and publishes the navigation status.
        #[qinvokable]
        #[cxx_name = "selectSection"]
        fn select_section(self: Pin<&mut Self>, section: &QString);

        /// Restores the initial demo state: home section, zero checks, and ready bridge.
        #[qinvokable]
        #[cxx_name = "resetDemo"]
        fn reset_demo(self: Pin<&mut Self>);
    }
}

pub struct ShellControllerRust {
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

impl Default for ShellControllerRust {
    fn default() -> Self {
        Self {
            bridge_status: QString::from("Backend Rust conectado"),
            active_section: QString::from("home"),
            health_checks: 0,
            bridge_ready: true,
            app_version: QString::from(crate::app_info::APP.version),
            app_name: QString::from(crate::app_info::APP.display_name),
            app_description: QString::from(crate::i18n::text(crate::app_info::APP.description)),
            app_license: QString::from(crate::app_info::APP.license),
            app_authors: QString::from(crate::app_info::APP.authors),
            app_copyright: QString::from(crate::app_info::APP.copyright),
            app_homepage: QString::from(crate::app_info::APP.homepage),
            app_repository: QString::from(crate::app_info::APP.repository),
            shortcuts_json: crate::ui_adapters::native_ffi::ffi::configured_shortcuts(),
        }
    }
}

impl qobject::ShellController {
    pub fn configure_shortcuts(mut self: Pin<&mut Self>) {
        let value = crate::ui_adapters::native_ffi::ffi::configure_shortcuts();
        if self.shortcuts_json() != &value {
            self.as_mut().rust_mut().shortcuts_json = value;
            self.shortcuts_json_changed();
        }
    }

    pub fn run_health_check(mut self: Pin<&mut Self>) {
        let next_check = self.health_checks().saturating_add(1);

        self.as_mut().set_health_checks(next_check);
        self.as_mut().set_bridge_ready(true);
        self.set_bridge_status(QString::from(crate::i18n::format(
            "Puente Rust/QML verificado · prueba #%1",
            &[next_check.to_string()],
        )));
    }

    pub fn select_section(mut self: Pin<&mut Self>, section: &QString) {
        self.as_mut().set_active_section(section.clone());
        self.set_bridge_status(QString::from(crate::i18n::format(
            "Navegación gestionada por Rust · %1",
            &[section.to_string()],
        )));
    }

    pub fn reset_demo(mut self: Pin<&mut Self>) {
        self.as_mut().set_health_checks(0);
        self.as_mut().set_bridge_ready(true);
        self.as_mut().set_active_section(QString::from("home"));
        self.set_bridge_status(QString::from("Backend Rust conectado"));
    }
}
