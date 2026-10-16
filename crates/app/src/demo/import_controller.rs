//! Explicit UI demonstration controller; no real vault or operating-system adapter.
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, busy, cxx_name = "busy", READ, NOTIFY)]
        #[qproperty(bool, reviewed, cxx_name = "reviewed", READ, NOTIFY)]
        #[qproperty(QString, summary, cxx_name = "summary", READ, NOTIFY)]
        #[qproperty(QString, warnings, cxx_name = "warnings", READ, NOTIFY)]
        #[qproperty(QString, status, cxx_name = "status", READ, NOTIFY)]
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        type ImportController = super::ImportControllerRust;
        #[qinvokable]
        #[cxx_name = "reset"]
        fn reset(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "review"]
        fn review(self: Pin<&mut Self>, input: &QUrl);
        #[qinvokable]
        #[cxx_name = "importReviewed"]
        fn import_reviewed(self: Pin<&mut Self>, recipients_json: &QString);
        #[qsignal]
        #[cxx_name = "importCompleted"]
        fn import_completed(self: Pin<&mut Self>);
    }
}

#[derive(Default)]
pub struct ImportControllerRust {
    busy: bool,
    reviewed: bool,
    summary: QString,
    warnings: QString,
    status: QString,
    error_message: QString,
}

impl qobject::ImportController {
    fn publish_reviewed(mut self: Pin<&mut Self>, value: bool) {
        if self.reviewed() != &value {
            self.as_mut().rust_mut().reviewed = value;
            self.reviewed_changed();
        }
    }
    fn publish_summary(mut self: Pin<&mut Self>, value: QString) {
        if self.summary() != &value {
            self.as_mut().rust_mut().summary = value;
            self.summary_changed();
        }
    }
    fn publish_error_message(mut self: Pin<&mut Self>, value: QString) {
        if self.error_message() != &value {
            self.as_mut().rust_mut().error_message = value;
            self.error_message_changed();
        }
    }
    pub fn reset(mut self: Pin<&mut Self>) {
        self.as_mut().publish_error_message(QString::default());
        self.as_mut().publish_reviewed(false);
        self.publish_summary(QString::default());
    }
    pub fn review(self: Pin<&mut Self>, _input: &QUrl) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
    pub fn import_reviewed(self: Pin<&mut Self>, _recipients_json: &QString) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
}
