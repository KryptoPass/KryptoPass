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
        /// Application metadata, shortcuts, and desktop-shell actions exposed to QML.
        #[qobject]
        #[qml_element]
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

        #[qinvokable]
        #[cxx_name = "showAboutDialog"]
        fn show_about_dialog(self: Pin<&mut Self>);
    }
}

pub struct ShellControllerRust {
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
    pub fn show_about_dialog(self: Pin<&mut Self>) {
        crate::ui_adapters::native_ffi::ffi::show_about_dialog();
    }

    pub fn configure_shortcuts(mut self: Pin<&mut Self>) {
        let value = crate::ui_adapters::native_ffi::ffi::configure_shortcuts();
        if self.shortcuts_json() != &value {
            self.as_mut().rust_mut().shortcuts_json = value;
            self.shortcuts_json_changed();
        }
    }
}
