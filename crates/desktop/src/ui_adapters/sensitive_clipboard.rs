use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use super::native_ffi::ffi as native_ffi;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");

        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// Temporary clipboard access for explicit copy actions.
        #[qobject]
        #[qml_element]
        #[qproperty(
            i32,
            auto_clear_seconds,
            cxx_name = "autoClearSeconds",
            READ = read_auto_clear_seconds,
            WRITE = write_auto_clear_seconds,
            NOTIFY
        )]
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        type SensitiveClipboard = super::SensitiveClipboardRust;

        #[qinvokable]
        #[cxx_name = "copySensitive"]
        fn copy_sensitive(self: Pin<&mut Self>, text: &QString);
        #[qinvokable]
        #[cxx_name = "clearOwned"]
        fn clear_owned(self: Pin<&mut Self>);

        fn read_auto_clear_seconds(self: &Self) -> i32;
        fn write_auto_clear_seconds(self: Pin<&mut Self>, seconds: i32);
    }
}

pub struct SensitiveClipboardRust {
    auto_clear_seconds: i32,
    error_message: QString,
}

impl Default for SensitiveClipboardRust {
    fn default() -> Self {
        let auto_clear_seconds = unsafe { native_ffi::auto_clear_seconds() };
        Self {
            auto_clear_seconds,
            error_message: QString::default(),
        }
    }
}

impl qobject::SensitiveClipboard {
    pub fn read_auto_clear_seconds(&self) -> i32 {
        self.rust().auto_clear_seconds
    }

    pub fn write_auto_clear_seconds(mut self: Pin<&mut Self>, seconds: i32) {
        let seconds = seconds.clamp(0, 24 * 60 * 60);
        if self.rust().auto_clear_seconds == seconds {
            return;
        }

        unsafe { native_ffi::set_auto_clear_seconds(seconds) };
        self.as_mut().rust_mut().auto_clear_seconds = seconds;
        self.as_mut().auto_clear_seconds_changed();
    }

    pub fn copy_sensitive(self: Pin<&mut Self>, text: &QString) {
        let copied =
            unsafe { native_ffi::copy_sensitive(text, self.as_ref().read_auto_clear_seconds()) };
        let error = if copied {
            QString::default()
        } else {
            QString::from("No se pudo copiar el contenido sensible al portapapeles.")
        };
        self.set_error_message(error);
    }

    pub fn clear_owned(self: Pin<&mut Self>) {
        unsafe { native_ffi::clear_owned() };
    }

    fn set_error_message(mut self: Pin<&mut Self>, value: QString) {
        if self.error_message() == &value {
            return;
        }
        self.as_mut().rust_mut().error_message = value;
        self.as_mut().error_message_changed();
    }
}
