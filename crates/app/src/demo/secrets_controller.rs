//! Explicit UI demonstration controller; no real vault or operating-system adapter.
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList, QUrl};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QStringList, entry_records, cxx_name = "entryRecords", READ, NOTIFY)]
        #[qproperty(QStringList, folder_records, cxx_name = "folderRecords", READ, NOTIFY)]
        #[qproperty(QString, detail_json, cxx_name = "detailJson", READ, NOTIFY)]
        #[qproperty(QString, version_json, cxx_name = "versionJson", READ, NOTIFY)]
        #[qproperty(QString, editor_json, cxx_name = "editorJson", READ, NOTIFY)]
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        #[qproperty(QString, browser_access, cxx_name = "browserAccess", READ, NOTIFY)]
        #[qproperty(QString, browser_pairing, cxx_name = "browserPairing", READ, NOTIFY)]
        #[qproperty(QString, status, cxx_name = "status", READ, NOTIFY)]
        #[qproperty(bool, busy, cxx_name = "busy", READ, NOTIFY)]
        #[qproperty(bool, detail_busy, cxx_name = "detailBusy", READ, NOTIFY)]
        #[qproperty(bool, editor_busy, cxx_name = "editorBusy", READ, NOTIFY)]
        #[qproperty(bool, icon_busy, cxx_name = "iconBusy", READ, NOTIFY)]
        #[qproperty(bool, saving, cxx_name = "saving", READ, NOTIFY)]
        #[qproperty(bool, locked, cxx_name = "locked", READ, NOTIFY)]
        type SecretsController = super::SecretsControllerRust;
        #[qinvokable]
        #[cxx_name = "refreshBrowserPairing"]
        fn refresh_browser_pairing(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "requestBrowserUsername"]
        fn request_browser_username(self: Pin<&mut Self>, token: &QString, entry_id: &QString);
        #[qinvokable]
        #[cxx_name = "decideBrowserAccess"]
        fn decide_browser_access(
            self: Pin<&mut Self>,
            token: &QString,
            entry_id: &QString,
            approve: bool,
        );
        #[qinvokable]
        #[cxx_name = "decideBrowserPairing"]
        fn decide_browser_pairing(self: Pin<&mut Self>, token: &QString, approve: bool);
        #[qinvokable]
        #[cxx_name = "loadStore"]
        fn load_store(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "setActiveVault"]
        fn set_active_vault(self: Pin<&mut Self>, path: &QUrl);
        #[qinvokable]
        #[cxx_name = "lockVault"]
        fn lock_vault(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "unlockVault"]
        fn unlock_vault(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "clearSensitiveState"]
        fn clear_sensitive_state(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "selectEntry"]
        fn select_entry(self: Pin<&mut Self>, id: &QString);
        #[qinvokable]
        #[cxx_name = "openEntry"]
        fn open_entry(self: Pin<&mut Self>, id: &QString);
        #[qinvokable]
        #[cxx_name = "saveEntry"]
        fn save_entry(self: Pin<&mut Self>, draft_json: &QString);
        #[qinvokable]
        #[cxx_name = "prepareEntryIcon"]
        fn prepare_entry_icon(self: Pin<&mut Self>, path: &QUrl);
        #[qinvokable]
        #[cxx_name = "prepareSshKey"]
        fn prepare_ssh_key(self: Pin<&mut Self>, path: &QUrl, passphrase: &QString);
        #[qinvokable]
        #[cxx_name = "prepareTotpQr"]
        fn prepare_totp_qr(self: Pin<&mut Self>, path: &QUrl);
        #[qinvokable]
        #[cxx_name = "copyField"]
        fn copy_field(self: Pin<&mut Self>, field: &QString);
        #[qinvokable]
        #[cxx_name = "toggleField"]
        fn toggle_field(self: Pin<&mut Self>, field: &QString);
        #[qinvokable]
        #[cxx_name = "showQr"]
        fn show_qr(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "clearEditor"]
        fn clear_editor(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "togglePassword"]
        fn toggle_password(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "copyUsername"]
        fn copy_username(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "copyPassword"]
        fn copy_password(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "openHistoryVersion"]
        fn open_history_version(self: Pin<&mut Self>, index: i32);
        #[qinvokable]
        #[cxx_name = "closeHistoryVersion"]
        fn close_history_version(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "copyHistoryValue"]
        fn copy_history_value(self: Pin<&mut Self>, field: &QString);
        #[qinvokable]
        #[cxx_name = "exportActivity"]
        fn export_activity(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "refreshTotp"]
        fn refresh_totp(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "copyTotp"]
        fn copy_totp(self: Pin<&mut Self>, next: bool);
        #[qinvokable]
        #[cxx_name = "confirmCopy"]
        fn confirm_copy(self: Pin<&mut Self>, copied: bool);
        #[qinvokable]
        #[cxx_name = "saveFolder"]
        fn save_folder(
            self: Pin<&mut Self>,
            id: &QString,
            name: &QString,
            parent_id: &QString,
            icon: &QString,
            color: &QString,
        );
        #[qinvokable]
        #[cxx_name = "stepFolder"]
        fn step_folder(self: Pin<&mut Self>, id: &QString, direction: i32);
        #[qinvokable]
        #[cxx_name = "moveFolder"]
        fn move_folder(
            self: Pin<&mut Self>,
            id: &QString,
            parent_id: &QString,
            before_id: &QString,
        );
        #[qinvokable]
        #[cxx_name = "moveEntry"]
        fn move_entry(self: Pin<&mut Self>, id: &QString, folder_id: &QString);
        #[qinvokable]
        #[cxx_name = "setFavorite"]
        fn set_favorite(self: Pin<&mut Self>, id: &QString, favorite: bool);
        #[qinvokable]
        #[cxx_name = "setTrashed"]
        fn set_trashed(self: Pin<&mut Self>, id: &QString, trashed: bool);
        #[qinvokable]
        #[cxx_name = "removeEntry"]
        fn remove_entry(self: Pin<&mut Self>, id: &QString);
        #[qinvokable]
        #[cxx_name = "removeFolder"]
        fn remove_folder(self: Pin<&mut Self>, id: &QString);
        #[qinvokable]
        #[cxx_name = "generatePassword"]
        fn generate_password(
            self: Pin<&mut Self>,
            length: i32,
            lowercase: bool,
            uppercase: bool,
            digits: bool,
            symbols: bool,
            excluded: &QString,
        );
        #[qsignal]
        #[cxx_name = "iconPrepared"]
        fn icon_prepared(self: Pin<&mut Self>, json: &QString);
        #[qsignal]
        #[cxx_name = "sshKeyPrepared"]
        fn ssh_key_prepared(self: Pin<&mut Self>, json: &QString);
        #[qsignal]
        #[cxx_name = "totpQrPrepared"]
        fn totp_qr_prepared(self: Pin<&mut Self>, json: &QString);
        #[qsignal]
        #[cxx_name = "editorReady"]
        fn editor_ready(self: Pin<&mut Self>);
        #[qsignal]
        #[cxx_name = "entrySaved"]
        fn entry_saved(self: Pin<&mut Self>, id: &QString);
        #[qsignal]
        #[cxx_name = "folderSaved"]
        fn folder_saved(self: Pin<&mut Self>, id: &QString);
        #[qsignal]
        #[cxx_name = "clipboardRequested"]
        fn clipboard_requested(self: Pin<&mut Self>, value: &QString);
        #[qsignal]
        #[cxx_name = "activityExported"]
        fn activity_exported(self: Pin<&mut Self>, path: &QUrl);
        #[qsignal]
        #[cxx_name = "historyRecordFailed"]
        fn history_record_failed(self: Pin<&mut Self>);
        #[qsignal]
        #[cxx_name = "passwordGenerated"]
        fn password_generated(self: Pin<&mut Self>, value: &QString);
        #[qsignal]
        #[cxx_name = "operationFailed"]
        fn operation_failed(self: Pin<&mut Self>);
        #[qsignal]
        #[cxx_name = "operationSucceeded"]
        fn operation_succeeded(self: Pin<&mut Self>);
        #[qsignal]
        #[cxx_name = "vaultLocked"]
        fn vault_locked(self: Pin<&mut Self>);
        #[qsignal]
        #[cxx_name = "vaultActivated"]
        fn vault_activated(self: Pin<&mut Self>);
    }
}

#[derive(Default)]
pub struct SecretsControllerRust {
    entry_records: QStringList,
    folder_records: QStringList,
    detail_json: QString,
    version_json: QString,
    editor_json: QString,
    error_message: QString,
    browser_access: QString,
    browser_pairing: QString,
    status: QString,
    busy: bool,
    detail_busy: bool,
    editor_busy: bool,
    icon_busy: bool,
    saving: bool,
    locked: bool,
    store: super::store::DemoStore,
}

impl qobject::SecretsController {
    fn reject(mut self: Pin<&mut Self>) {
        self.as_mut()
            .publish_error_message(QString::from(super::UNAVAILABLE));
        self.operation_failed();
    }
    fn publish_entry_records(mut self: Pin<&mut Self>, value: QStringList) {
        if self.entry_records() != &value {
            self.as_mut().rust_mut().entry_records = value;
            self.entry_records_changed();
        }
    }
    fn publish_folder_records(mut self: Pin<&mut Self>, value: QStringList) {
        if self.folder_records() != &value {
            self.as_mut().rust_mut().folder_records = value;
            self.folder_records_changed();
        }
    }
    fn publish_detail_json(mut self: Pin<&mut Self>, value: QString) {
        if self.detail_json() != &value {
            self.as_mut().rust_mut().detail_json = value;
            self.detail_json_changed();
        }
    }
    fn publish_version_json(mut self: Pin<&mut Self>, value: QString) {
        if self.version_json() != &value {
            self.as_mut().rust_mut().version_json = value;
            self.version_json_changed();
        }
    }
    fn publish_editor_json(mut self: Pin<&mut Self>, value: QString) {
        if self.editor_json() != &value {
            self.as_mut().rust_mut().editor_json = value;
            self.editor_json_changed();
        }
    }
    fn publish_error_message(mut self: Pin<&mut Self>, value: QString) {
        if self.error_message() != &value {
            self.as_mut().rust_mut().error_message = value;
            self.error_message_changed();
        }
    }
    fn publish_status(mut self: Pin<&mut Self>, value: QString) {
        if self.status() != &value {
            self.as_mut().rust_mut().status = value;
            self.status_changed();
        }
    }
    fn publish_locked(mut self: Pin<&mut Self>, value: bool) {
        if self.locked() != &value {
            self.as_mut().rust_mut().locked = value;
            self.locked_changed();
        }
    }
    pub fn refresh_browser_pairing(self: Pin<&mut Self>) {
        // The demo has no browser server to poll.
        let _ = self;
    }
    pub fn request_browser_username(self: Pin<&mut Self>, _token: &QString, _entry_id: &QString) {
        self.reject();
    }
    pub fn decide_browser_access(
        self: Pin<&mut Self>,
        _token: &QString,
        _entry_id: &QString,
        _approve: bool,
    ) {
        self.reject();
    }
    pub fn decide_browser_pairing(self: Pin<&mut Self>, _token: &QString, _approve: bool) {
        self.reject();
    }
    pub fn load_store(mut self: Pin<&mut Self>) {
        let result = self.as_mut().rust_mut().store.load();
        if let Err(error) = result {
            self.publish_error_message(QString::from(error.to_string()));
            return;
        }
        let entries = self.rust().store.entries();
        let folders = self.rust().store.folders();
        self.as_mut()
            .publish_entry_records(entries.iter().map(QString::from).collect());
        self.as_mut()
            .publish_folder_records(folders.iter().map(QString::from).collect());
        self.publish_status(QString::from("Demostración Rust · datos ficticios"));
        crate::startup::ffi::log_message(&QString::from(
            "KryptoPass: QML -> Rust loadStore; metadata published",
        ));
    }
    pub fn set_active_vault(self: Pin<&mut Self>, _path: &QUrl) {
        self.reject();
    }
    pub fn lock_vault(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().store.set_locked(true);
        self.as_mut().publish_locked(true);
        self.as_mut().clear_sensitive_state();
        self.vault_locked();
    }
    pub fn unlock_vault(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().store.set_locked(false);
        self.as_mut().publish_locked(false);
        let detail = self.rust().store.detail().unwrap_or_default();
        self.publish_detail_json(QString::from(detail));
    }
    pub fn clear_sensitive_state(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().store.set_locked(true);
        self.as_mut().publish_locked(true);
        self.as_mut().publish_detail_json(QString::default());
        self.as_mut().publish_version_json(QString::default());
        self.publish_editor_json(QString::default());
    }
    pub fn select_entry(mut self: Pin<&mut Self>, id: &QString) {
        self.as_mut().rust_mut().store.select(&id.to_string());
        let detail = self.rust().store.detail().unwrap_or_default();
        self.publish_detail_json(QString::from(detail));
        crate::startup::ffi::log_message(&QString::from(
            "KryptoPass: QML -> Rust selectEntry; detail projection published",
        ));
    }
    pub fn open_entry(self: Pin<&mut Self>, _id: &QString) {
        self.reject();
    }
    pub fn save_entry(self: Pin<&mut Self>, _draft_json: &QString) {
        self.reject();
    }
    pub fn prepare_entry_icon(self: Pin<&mut Self>, _path: &QUrl) {
        self.reject();
    }
    pub fn prepare_ssh_key(self: Pin<&mut Self>, _path: &QUrl, _passphrase: &QString) {
        self.reject();
    }
    pub fn prepare_totp_qr(self: Pin<&mut Self>, _path: &QUrl) {
        self.reject();
    }
    pub fn copy_field(self: Pin<&mut Self>, _field: &QString) {
        self.reject();
    }
    pub fn toggle_field(self: Pin<&mut Self>, _field: &QString) {
        self.reject();
    }
    pub fn show_qr(self: Pin<&mut Self>) {
        self.reject();
    }
    pub fn clear_editor(self: Pin<&mut Self>) {
        self.publish_editor_json(QString::default());
    }
    pub fn toggle_password(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().store.toggle_password();
        let detail = self.rust().store.detail().unwrap_or_default();
        self.publish_detail_json(QString::from(detail));
    }
    pub fn copy_username(self: Pin<&mut Self>) {
        self.reject();
    }
    pub fn copy_password(self: Pin<&mut Self>) {
        self.reject();
    }
    pub fn open_history_version(self: Pin<&mut Self>, _index: i32) {
        self.reject();
    }
    pub fn close_history_version(self: Pin<&mut Self>) {
        self.publish_version_json(QString::default());
    }
    pub fn copy_history_value(self: Pin<&mut Self>, _field: &QString) {
        self.reject();
    }
    pub fn export_activity(self: Pin<&mut Self>) {
        self.reject();
    }
    pub fn refresh_totp(self: Pin<&mut Self>) {
        // The demo dataset contains no TOTP values.
        let _ = self;
    }
    pub fn copy_totp(self: Pin<&mut Self>, _next: bool) {
        self.reject();
    }
    pub fn confirm_copy(self: Pin<&mut Self>, copied: bool) {
        if !copied {
            self.reject();
        }
    }
    pub fn save_folder(
        self: Pin<&mut Self>,
        _id: &QString,
        _name: &QString,
        _parent_id: &QString,
        _icon: &QString,
        _color: &QString,
    ) {
        self.reject();
    }
    pub fn step_folder(self: Pin<&mut Self>, _id: &QString, _direction: i32) {
        self.reject();
    }
    pub fn move_folder(
        self: Pin<&mut Self>,
        _id: &QString,
        _parent_id: &QString,
        _before_id: &QString,
    ) {
        self.reject();
    }
    pub fn move_entry(self: Pin<&mut Self>, _id: &QString, _folder_id: &QString) {
        self.reject();
    }
    pub fn set_favorite(self: Pin<&mut Self>, _id: &QString, _favorite: bool) {
        self.reject();
    }
    pub fn set_trashed(self: Pin<&mut Self>, _id: &QString, _trashed: bool) {
        self.reject();
    }
    pub fn remove_entry(self: Pin<&mut Self>, _id: &QString) {
        self.reject();
    }
    pub fn remove_folder(self: Pin<&mut Self>, _id: &QString) {
        self.reject();
    }
    pub fn generate_password(
        self: Pin<&mut Self>,
        _length: i32,
        _lowercase: bool,
        _uppercase: bool,
        _digits: bool,
        _symbols: bool,
        _excluded: &QString,
    ) {
        self.reject();
    }
}
