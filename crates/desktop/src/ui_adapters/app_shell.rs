use cxx_qt_lib::QString;
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");

        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// Shell demo state exposed to the QML presentation layer.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, bridge_status)]
        #[qproperty(QString, active_section)]
        #[qproperty(i32, health_checks)]
        #[qproperty(bool, bridge_ready)]
        /// Fixed at Default from Cargo.toml; no invokable mutates it.
        #[qproperty(QString, app_version, READ, CONSTANT)]
        type ShellController = super::ShellControllerRust;

        /// Records a bridge health check and publishes the verification status.
        #[qinvokable]
        fn run_health_check(self: Pin<&mut Self>);

        /// Applies the section requested by the QML shell and publishes the navigation status.
        #[qinvokable]
        fn select_section(self: Pin<&mut Self>, section: &QString);

        /// Restores the initial demo state: home section, zero checks, and ready bridge.
        #[qinvokable]
        fn reset_demo(self: Pin<&mut Self>);
    }
}

pub struct ShellControllerRust {
    bridge_status: QString,
    active_section: QString,
    health_checks: i32,
    bridge_ready: bool,
    app_version: QString,
}

impl Default for ShellControllerRust {
    fn default() -> Self {
        Self {
            bridge_status: QString::from("Backend Rust conectado"),
            active_section: QString::from("home"),
            health_checks: 0,
            bridge_ready: true,
            app_version: QString::from(crate::app_info::APP.version),
        }
    }
}

impl qobject::ShellController {
    pub fn run_health_check(mut self: Pin<&mut Self>) {
        let next_check = self.health_checks().saturating_add(1);

        self.as_mut().set_health_checks(next_check);
        self.as_mut().set_bridge_ready(true);
        self.set_bridge_status(QString::from(format!(
            "Puente Rust/QML verificado · prueba #{next_check}"
        )));
    }

    pub fn select_section(mut self: Pin<&mut Self>, section: &QString) {
        self.as_mut().set_active_section(section.clone());
        self.set_bridge_status(QString::from(format!(
            "Navegación gestionada por Rust · {section}"
        )));
    }

    pub fn reset_demo(mut self: Pin<&mut Self>) {
        self.as_mut().set_health_checks(0);
        self.as_mut().set_bridge_ready(true);
        self.as_mut().set_active_section(QString::from("home"));
        self.set_bridge_status(QString::from("Backend Rust conectado"));
    }
}
