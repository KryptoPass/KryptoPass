//! Explicit UI demonstration controller; no real vault or operating-system adapter.
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status, cxx_name = "status", READ, NOTIFY)]
        #[qproperty(bool, busy, cxx_name = "busy", READ, NOTIFY)]
        #[qproperty(bool, operation_busy, cxx_name = "operationBusy", READ, NOTIFY)]
        #[qproperty(QStringList, key_summaries, cxx_name = "keySummaries", READ, NOTIFY)]
        #[qproperty(i32, key_count, cxx_name = "keyCount", READ, NOTIFY)]
        #[qproperty(QStringList, key_records, cxx_name = "keyRecords", READ, NOTIFY)]
        #[qproperty(QString, load_error, cxx_name = "loadError", READ, NOTIFY)]
        #[qproperty(QString, operation_error, cxx_name = "operationError", READ, NOTIFY)]
        #[qproperty(QString, online_settings, cxx_name = "onlineSettings", READ, NOTIFY)]
        type KeyringController = super::KeyringControllerRust;
        #[qinvokable]
        #[cxx_name = "saveOnlineSettings"]
        fn save_online_settings(self: Pin<&mut Self>, json: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "openOnlineService"]
        fn open_online_service(self: Pin<&mut Self>, index: i32, fingerprint: &QString);
        #[qinvokable]
        #[cxx_name = "refreshKeys"]
        fn refresh_keys(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "testAgentPrompt"]
        fn test_agent_prompt(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "runOperation"]
        fn run_operation(self: Pin<&mut Self>, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "clearOperationError"]
        fn clear_operation_error(self: Pin<&mut Self>);
        #[qsignal]
        #[cxx_name = "operationFinished"]
        fn operation_finished(self: Pin<&mut Self>, fingerprint: &QString, message: &QString);
        #[qsignal]
        #[cxx_name = "operationFailed"]
        fn operation_failed(self: Pin<&mut Self>, message: &QString);
        #[qsignal]
        #[cxx_name = "publicKeyReady"]
        fn public_key_ready(self: Pin<&mut Self>, armored: &QString);
    }
}

pub struct KeyringControllerRust {
    status: QString,
    busy: bool,
    operation_busy: bool,
    key_summaries: QStringList,
    key_count: i32,
    key_records: QStringList,
    load_error: QString,
    operation_error: QString,
    online_settings: QString,
}

impl qobject::KeyringController {
    fn reject(mut self: Pin<&mut Self>) {
        let message = QString::from(super::UNAVAILABLE);
        self.as_mut().publish_operation_error(message.clone());
        self.operation_failed(&message);
    }
    fn publish_status(mut self: Pin<&mut Self>, value: QString) {
        if self.status() != &value {
            self.as_mut().rust_mut().status = value;
            self.status_changed();
        }
    }
    fn publish_operation_error(mut self: Pin<&mut Self>, value: QString) {
        if self.operation_error() != &value {
            self.as_mut().rust_mut().operation_error = value;
            self.operation_error_changed();
        }
    }
    pub fn save_online_settings(self: Pin<&mut Self>, _json: &QString) -> bool {
        self.reject();
        false
    }
    pub fn open_online_service(self: Pin<&mut Self>, _index: i32, _fingerprint: &QString) {
        self.reject();
    }
    pub fn refresh_keys(self: Pin<&mut Self>) {
        self.publish_status(QString::from("Demostración · llavero no disponible"));
    }
    pub fn test_agent_prompt(self: Pin<&mut Self>) {
        self.reject();
    }
    pub fn run_operation(self: Pin<&mut Self>, _request_json: &QString) {
        self.reject();
    }
    pub fn clear_operation_error(self: Pin<&mut Self>) {
        self.publish_operation_error(QString::default());
    }
}

impl Default for KeyringControllerRust {
    fn default() -> Self {
        Self {
            status: Default::default(),
            busy: Default::default(),
            operation_busy: Default::default(),
            key_summaries: Default::default(),
            key_count: Default::default(),
            key_records: Default::default(),
            load_error: Default::default(),
            operation_error: Default::default(),
            online_settings: QString::from(r#"{"enabled":false,"services":[],"keyservers":[]}"#),
        }
    }
}
