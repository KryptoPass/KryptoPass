use cxx_qt_lib::QString;
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");

        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Application state exposed to the QML presentation layer.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, bridge_status)]
        #[qproperty(QString, active_section)]
        #[qproperty(i32, health_checks)]
        #[qproperty(bool, bridge_ready)]
        type AppController = super::AppControllerRust;

        /// Runs a small round trip through the Rust/Qt bridge.
        #[qinvokable]
        #[cxx_name = "runHealthCheck"]
        fn run_health_check(self: Pin<&mut Self>);

        /// Changes the section selected by the QML shell.
        #[qinvokable]
        #[cxx_name = "selectSection"]
        fn select_section(self: Pin<&mut Self>, section: &QString);

        /// Restores the initial demonstration state.
        #[qinvokable]
        #[cxx_name = "resetDemo"]
        fn reset_demo(self: Pin<&mut Self>);
    }
}

pub struct AppControllerRust {
    bridge_status: QString,
    active_section: QString,
    health_checks: i32,
    bridge_ready: bool,
}

impl Default for AppControllerRust {
    fn default() -> Self {
        Self {
            bridge_status: QString::from("Backend Rust conectado"),
            active_section: QString::from("home"),
            health_checks: 0,
            bridge_ready: true,
        }
    }
}

impl qobject::AppController {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_ready_and_starts_at_home() {
        let controller = AppControllerRust::default();

        assert!(controller.bridge_ready);
        assert_eq!(controller.active_section.to_string(), "home");
        assert_eq!(controller.health_checks, 0);
        assert_eq!(
            controller.bridge_status.to_string(),
            "Backend Rust conectado"
        );
    }
}
