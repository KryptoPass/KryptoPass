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
        #[qproperty(i32, auto_clear_seconds, cxx_name = "autoClearSeconds", READ, WRITE = write_auto_clear_seconds, NOTIFY)]
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        type SensitiveClipboard = super::SensitiveClipboardRust;
        #[qinvokable]
        #[cxx_name = "copySensitive"]
        fn copy_sensitive(self: Pin<&mut Self>, text: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "clearOwned"]
        fn clear_owned(self: Pin<&mut Self>);

        #[cxx_name = "writeAutoClearSeconds"]
        fn write_auto_clear_seconds(self: Pin<&mut Self>, seconds: i32);
    }
}

#[derive(Default)]
pub struct SensitiveClipboardRust {
    auto_clear_seconds: i32,
    error_message: QString,
}

impl qobject::SensitiveClipboard {
    pub fn write_auto_clear_seconds(self: Pin<&mut Self>, _seconds: i32) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
    fn publish_error_message(mut self: Pin<&mut Self>, value: QString) {
        if self.error_message() != &value {
            self.as_mut().rust_mut().error_message = value;
            self.error_message_changed();
        }
    }
    pub fn copy_sensitive(self: Pin<&mut Self>, _text: &QString) -> bool {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
        false
    }
    pub fn clear_owned(self: Pin<&mut Self>) {
        // The demo never takes ownership of the system clipboard.
        let _ = self;
    }
}
