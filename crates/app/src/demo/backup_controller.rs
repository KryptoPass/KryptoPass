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
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        #[qproperty(QString, status, cxx_name = "status", READ, NOTIFY)]
        type BackupController = super::BackupControllerRust;
        #[qinvokable]
        #[cxx_name = "clearError"]
        fn clear_error(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "exportBackup"]
        fn export_backup(self: Pin<&mut Self>, path: &QUrl, recipients_json: &QString);
        #[qinvokable]
        #[cxx_name = "restoreBackup"]
        fn restore_backup(self: Pin<&mut Self>, input: &QUrl, output: &QUrl);
        #[qinvokable]
        #[cxx_name = "restoreManagedBackup"]
        fn restore_managed_backup(self: Pin<&mut Self>, input: &QUrl);
        #[qsignal]
        #[cxx_name = "backupExported"]
        fn backup_exported(self: Pin<&mut Self>, path: &QUrl);
        #[qsignal]
        #[cxx_name = "backupRestored"]
        fn backup_restored(self: Pin<&mut Self>, path: &QUrl);
        #[qsignal]
        #[cxx_name = "operationFailed"]
        fn operation_failed(self: Pin<&mut Self>);
    }
}

#[derive(Default)]
pub struct BackupControllerRust {
    busy: bool,
    error_message: QString,
    status: QString,
}

impl qobject::BackupController {
    fn reject(mut self: Pin<&mut Self>) {
        self.as_mut()
            .publish_error_message(QString::from(super::UNAVAILABLE));
        self.operation_failed();
    }
    fn publish_error_message(mut self: Pin<&mut Self>, value: QString) {
        if self.error_message() != &value {
            self.as_mut().rust_mut().error_message = value;
            self.error_message_changed();
        }
    }
    pub fn clear_error(self: Pin<&mut Self>) {
        self.publish_error_message(QString::default());
    }
    pub fn export_backup(self: Pin<&mut Self>, _path: &QUrl, _recipients_json: &QString) {
        self.reject();
    }
    pub fn restore_backup(self: Pin<&mut Self>, _input: &QUrl, _output: &QUrl) {
        self.reject();
    }
    pub fn restore_managed_backup(self: Pin<&mut Self>, _input: &QUrl) {
        self.reject();
    }
}
