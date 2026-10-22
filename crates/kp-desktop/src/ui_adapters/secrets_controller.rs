use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt::{CxxQtType, Threading, casting::Upcast};
use cxx_qt_lib::{QDateTime, QObjectMutPtr, QString, QStringList, QTimeZone, QUrl};
use kryptopass_core::{
    FolderAppearance, FolderId, KeyFingerprint, LoginHistoryEventKind, OpenPgp, PasswordOptions,
    RecipientSet, SecretContent, SecretId, SecretsService,
    generate_password as generate_secure_password, generate_totp,
};
use kryptopass_secure_memory::{SensitiveBytes, SensitiveText};
use uuid::Uuid;

use crate::infrastructure::GnuPgOpenPgp;
use crate::storage::{SqliteStorage, paths as app_paths};

use super::secrets_projection::{
    EntryDraft, OpenedEntry, StoreSnapshot, content_field, content_projection,
    content_projection_with_state, draft_matches_session, editor_projection, new_editor_projection,
    prepare_save, snapshot_revision, store_snapshot_with_links,
};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qstringlist.h");
        include!("cxx-qt-lib/qurl.h");

        type QString = cxx_qt_lib::QString;
        type QStringList = cxx_qt_lib::QStringList;
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        /// Coordinates secret-store operations and exposes short-lived view projections.
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
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(bool, detail_busy, cxx_name = "detailBusy", READ, NOTIFY)]
        #[qproperty(bool, editor_busy, cxx_name = "editorBusy", READ, NOTIFY)]
        #[qproperty(bool, icon_busy, cxx_name = "iconBusy", READ, NOTIFY)]
        #[qproperty(bool, saving, READ, NOTIFY)]
        #[qproperty(bool, locked, READ, NOTIFY)]
        #[qproperty(bool, unlocking, READ, NOTIFY)]
        #[qproperty(bool, setup_required, cxx_name = "setupRequired", READ, NOTIFY)]
        #[qproperty(QString, primary_key, cxx_name = "primaryKey", READ, NOTIFY)]
        #[qproperty(bool, forget_on_lock, cxx_name = "forgetOnLock", READ, NOTIFY)]
        type SecretsController = super::SecretsControllerRust;

        #[qinvokable]
        #[cxx_name = "matchesSearch"]
        fn matches_search(self: Pin<&mut Self>, query: &QString, metadata: &QString) -> bool;

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
        #[cxx_name = "lockForSession"]
        fn lock_for_session(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "setSessionBlocked"]
        fn set_session_blocked(self: Pin<&mut Self>, blocked: bool);
        #[qinvokable]
        #[cxx_name = "unlockVault"]
        fn unlock_vault(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "completeSetup"]
        fn complete_setup(self: Pin<&mut Self>, primary_key: &QString, dedicated: bool);
        #[qinvokable]
        #[cxx_name = "setPrimaryKey"]
        fn set_primary_key(self: Pin<&mut Self>, primary_key: &QString);
        #[qinvokable]
        #[cxx_name = "setForgetOnLock"]
        fn set_forget_on_lock(self: Pin<&mut Self>, enabled: bool);
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
        fn remove_folder(self: Pin<&mut Self>, id: &QString, delete_contents: bool);
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

    impl cxx_qt::Threading for SecretsController {}
    impl cxx_qt::Initialize for SecretsController {}
}

/// State owned by the QObject; decrypted values have zeroizing domain types.
pub struct SecretsControllerRust {
    metadata_search: kryptopass_core::search::MetadataSearch,
    browser_pairing: QString,
    browser_access: QString,
    entry_records: QStringList,
    folder_records: QStringList,
    detail_json: QString,
    editor_json: QString,
    version_json: QString,
    history_version_index: Option<usize>,
    error_message: QString,
    /// Keep an overlapping detail-read failure from hiding a store-operation error.
    operation_error_active: bool,
    status: QString,
    busy: bool,
    detail_busy: bool,
    editor_busy: bool,
    icon_busy: bool,
    staged_icon: Option<kryptopass_core::EntryIconAsset>,
    saving: bool,
    locked: bool,
    unlocking: bool,
    setup_required: bool,
    primary_key: QString,
    forget_on_lock: bool,
    primary: Option<crate::storage::PrimaryKey>,
    /// Invalidates an in-flight unlock check when the vault is locked again.
    unlock_operation: u64,
    session_blocked: bool,
    selected_entry: Option<SecretId>,
    selected_revision: Option<u64>,
    detail_content: Option<SecretContent>,
    password_revealed: bool,
    revealed_fields: std::collections::BTreeSet<String>,
    qr_source: Option<SensitiveText>,
    editor_ssh: Option<kryptopass_core::SshKeyContent>,
    staged_ssh: Option<(String, kryptopass_core::SshKeyContent)>,
    prepare_operation: u64,
    pending_copy: Option<PendingCopy>,
    editor_session: Option<EditorSession>,
    load_operation: u64,
    active_vault_operation: u64,
    selection_operation: u64,
    selection_worker_active: bool,
    selection_cancel_epoch: Arc<AtomicU64>,
    /// Rapid UI changes replace this request while the active decrypt finishes.
    pending_selection: Option<(SecretId, u64)>,
    editor_operation: u64,
    editor_cancel_epoch: Arc<AtomicU64>,
}

impl Default for SecretsControllerRust {
    fn default() -> Self {
        Self {
            metadata_search: kryptopass_core::search::MetadataSearch::default(),
            browser_pairing: QString::default(),
            browser_access: QString::default(),
            entry_records: QStringList::default(),
            folder_records: QStringList::default(),
            detail_json: QString::default(),
            editor_json: QString::default(),
            version_json: QString::default(),
            history_version_index: None,
            error_message: QString::default(),
            operation_error_active: false,
            status: QString::from(crate::i18n::text("Bóveda no cargada")),
            busy: false,
            detail_busy: false,
            editor_busy: false,
            icon_busy: false,
            staged_icon: None,
            saving: false,
            locked: true,
            unlocking: false,
            setup_required: false,
            primary_key: QString::default(),
            forget_on_lock: false,
            primary: None,
            unlock_operation: 0,
            session_blocked: false,
            selected_entry: None,
            selected_revision: None,
            detail_content: None,
            password_revealed: false,
            revealed_fields: std::collections::BTreeSet::new(),
            qr_source: None,
            editor_ssh: None,
            staged_ssh: None,
            prepare_operation: 0,
            pending_copy: None,
            editor_session: None,
            load_operation: 0,
            active_vault_operation: 0,
            selection_operation: 0,
            selection_worker_active: false,
            selection_cancel_epoch: Arc::new(AtomicU64::new(0)),
            pending_selection: None,
            editor_operation: 0,
            editor_cancel_epoch: Arc::new(AtomicU64::new(0)),
        }
    }
}

#[derive(Clone, Copy)]
struct EditorSession {
    id: Option<SecretId>,
    revision: Option<u64>,
}

#[derive(Clone, Copy)]
struct PendingCopy {
    id: SecretId,
    selection_operation: u64,
    kind: CopyKind,
}

#[derive(Clone, Copy)]
enum CopyKind {
    Username,
    Password,
    Totp,
    NextTotp,
}

struct MutationResult {
    snapshot: Result<StoreSnapshot, ()>,
    signal: MutationSignal,
    editor_operation: Option<u64>,
}

struct CopyHistoryResult {
    content: SecretContent,
    write_revision: u64,
    observed_revision: Option<u64>,
    snapshot: Result<StoreSnapshot, ()>,
}

enum MutationSignal {
    None,
    EntrySaved(String),
    FolderSaved(String),
    EntryRemoved(SecretId),
    MetadataChanged(SecretId, u64),
}

impl cxx_qt::Initialize for qobject::SecretsController {
    fn initialize(self: Pin<&mut Self>) {
        let base: Pin<&mut cxx_qt::QObject> = self.upcast_pin();
        // The monitor is a QObject child and cannot outlive this receiver.
        unsafe {
            let receiver =
                QObjectMutPtr::from_raw(std::ptr::from_mut(Pin::get_unchecked_mut(base)));
            super::native_ffi::ffi::install_session_lock_monitor(receiver);
        }
    }
}

impl qobject::SecretsController {
    pub fn matches_search(self: Pin<&mut Self>, query: &QString, metadata: &QString) -> bool {
        self.rust_mut()
            .metadata_search
            .matches(&query.to_string(), &metadata.to_string())
    }

    pub fn request_browser_username(mut self: Pin<&mut Self>, token: &QString, entry_id: &QString) {
        if !*self.locked() {
            kryptopass_browser::request_username(&token.to_string(), &entry_id.to_string());
            self.as_mut().refresh_browser_pairing();
        }
    }

    pub fn decide_browser_access(
        mut self: Pin<&mut Self>,
        token: &QString,
        entry_id: &QString,
        approve: bool,
    ) {
        kryptopass_browser::decide_access(
            &token.to_string(),
            &entry_id.to_string(),
            approve && !*self.locked(),
        );
        self.as_mut().refresh_browser_pairing();
    }

    pub fn refresh_browser_pairing(mut self: Pin<&mut Self>) {
        let value = QString::from(kryptopass_browser::pending_access().as_str());
        if self.browser_access() != &value {
            self.as_mut().rust_mut().browser_access = value;
            self.as_mut().browser_access_changed();
        }
        self.as_mut()
            .set_browser_pairing(QString::from(kryptopass_browser::pending_pairing()));
    }

    pub fn decide_browser_pairing(mut self: Pin<&mut Self>, token: &QString, approve: bool) {
        kryptopass_browser::decide_pairing(&token.to_string(), approve && !*self.locked());
        self.as_mut().refresh_browser_pairing();
    }

    pub fn load_store(mut self: Pin<&mut Self>) {
        if *self.busy() {
            return;
        }

        let operation = {
            let mut state = self.as_mut().rust_mut();
            state.load_operation = state.load_operation.wrapping_add(1);
            state.load_operation
        };
        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut().set_status(QString::from("Cargando bóveda…"));

        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            || {
                let snapshot = load_store_snapshot()?;
                let setup = resolve_setup_state(&snapshot)?;
                let primary = open_storage()?.primary_key()?;
                Ok((snapshot, setup, primary))
            },
            move |mut controller,
                  result: anyhow::Result<(
                StoreSnapshot,
                Option<app_paths::SetupState>,
                Option<crate::storage::PrimaryKey>,
            )>| {
                if controller.as_ref().rust().load_operation != operation {
                    return;
                }

                controller.as_mut().set_busy(false);
                match result {
                    Ok((snapshot, setup, primary)) => {
                        controller.as_mut().apply_setup_state(setup.is_some());
                        controller.as_mut().apply_primary_key(primary);
                        controller.as_mut().publish_snapshot(snapshot);
                        controller
                            .as_mut()
                            .set_status(QString::from("Bóveda cargada"));
                    }
                    Err(_) => controller.as_mut().fail("No se pudo cargar la bóveda."),
                }
            },
        );
    }

    pub fn set_active_vault(mut self: Pin<&mut Self>, path: &QUrl) {
        if *self.busy() || *self.saving() {
            self.as_mut()
                .fail("Espera a que termine la operación actual antes de cambiar de bóveda.");
            return;
        }

        let Some(local_path) = path.to_local_file() else {
            self.as_mut()
                .fail("Selecciona un archivo local de bóveda existente.");
            return;
        };
        let path = std::path::PathBuf::from(local_path.to_string());
        if !path.is_absolute() {
            self.as_mut()
                .fail("Selecciona un archivo local de bóveda existente.");
            return;
        }

        let operation = {
            let mut state = self.as_mut().rust_mut();
            state.active_vault_operation = state.active_vault_operation.wrapping_add(1);
            state.load_operation = state.load_operation.wrapping_add(1);
            state.active_vault_operation
        };
        self.as_mut().clear_sensitive_state_contents();
        self.as_mut().set_locked(true);
        self.as_mut().vault_locked();
        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Validando la bóveda restaurada…"));

        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || activate_store_snapshot(path),
            move |mut controller, result| {
                if controller.as_ref().rust().active_vault_operation != operation {
                    return;
                }
                controller.as_mut().set_busy(false);
                match result {
                    Ok((snapshot, durable, primary)) => {
                        controller.as_mut().apply_primary_key(primary);
                        controller.as_mut().publish_snapshot(snapshot);
                        let status = if durable {
                            "Bóveda restaurada lista. Desbloquéala para acceder a sus secretos."
                        } else {
                            "Bóveda seleccionada, pero no se pudo confirmar la persistencia de la preferencia."
                        };
                        controller.as_mut().set_status(QString::from(status));
                        controller.as_mut().vault_activated();
                    }
                    Err(_) => controller
                        .as_mut()
                        .fail("No se pudo abrir la bóveda restaurada. La bóveda activa no cambió."),
                }
            },
        );
    }

    pub fn lock_vault(mut self: Pin<&mut Self>) {
        if *self.busy() || *self.saving() {
            self.as_mut()
                .fail("Espera a que termine el guardado antes de bloquear la bóveda.");
            return;
        }
        self.as_mut().cancel_unlock();
        self.as_mut().clear_sensitive_state_contents();
        self.as_mut().clear_error();
        self.as_mut().set_locked(true);
        self.as_mut().forget_primary_passphrase();
        self.as_mut().set_status(QString::from("Bóveda bloqueada"));
        self.as_mut().vault_locked();
    }

    pub fn unlock_vault(mut self: Pin<&mut Self>) {
        if self.as_ref().rust().session_blocked {
            self.as_mut()
                .fail("Desbloquea la sesión del escritorio antes de acceder a la bóveda.");
            return;
        }
        if *self.busy() || *self.saving() {
            self.as_mut()
                .fail("Espera a que termine la operación antes de acceder a la bóveda.");
            return;
        }
        if !*self.locked() || *self.unlocking() {
            return;
        }
        self.as_mut().clear_error();
        let operation = {
            let mut state = self.as_mut().rust_mut();
            state.unlock_operation = state.unlock_operation.wrapping_add(1);
            state.unlock_operation
        };
        self.as_mut().set_unlocking(true);
        self.as_mut()
            .set_status(QString::from("Comprobando tu clave OpenPGP…"));
        let primary = self
            .as_ref()
            .rust()
            .primary
            .as_ref()
            .map(|key| key.fingerprint.clone());
        // The vault stays covered until GnuPG proves access; pinentry may ask here.
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || verify_vault_access(primary.as_ref()),
            move |mut controller, result| {
                if controller.as_ref().rust().unlock_operation != operation {
                    return;
                }
                controller.as_mut().set_unlocking(false);
                if controller.as_ref().rust().session_blocked {
                    return;
                }
                match result {
                    Ok(()) => {
                        controller.as_mut().set_locked(false);
                        controller.as_mut().set_status(QString::from(
                            "Bóveda disponible. El contenido se abrirá al seleccionarlo.",
                        ));
                    }
                    Err(error) if error.is::<NoUnlockKey>() => controller.as_mut().fail(
                        "No hay en este equipo una clave privada de esta bóveda. Importa tu clave para desbloquearla.",
                    ),
                    Err(_) => controller
                        .as_mut()
                        .fail("No se pudo verificar tu clave. La bóveda sigue bloqueada."),
                }
            },
        );
    }

    /// Finishes the first run, recording the chosen key as the vault's primary key.
    ///
    /// A key created only for KryptoPass is forgotten on lock; a personal key is
    /// left alone until the user opts in, because the cache is shared by its keygrip.
    pub fn complete_setup(mut self: Pin<&mut Self>, primary_key: &QString, dedicated: bool) {
        if !primary_key.to_string().trim().is_empty()
            && !self.as_mut().store_primary_key(primary_key, dedicated)
        {
            return;
        }
        let state = app_paths::SetupState { version: 1 };
        if app_paths::save_setup_state(crate::app_info::APP.desktop_file_id, &state).is_err() {
            self.as_mut()
                .fail("No se pudo guardar la configuración inicial.");
            return;
        }
        self.apply_setup_state(true);
    }

    /// Changes the default recipient for new items; existing items keep their keys.
    pub fn set_primary_key(mut self: Pin<&mut Self>, primary_key: &QString) {
        self.as_mut().store_primary_key(primary_key, false);
    }

    pub fn set_forget_on_lock(mut self: Pin<&mut Self>, enabled: bool) {
        let Some(mut key) = self.as_ref().rust().primary.clone() else {
            return;
        };
        key.forget_on_lock = enabled;
        if open_storage()
            .and_then(|mut storage| storage.set_primary_key(&key))
            .is_err()
        {
            self.as_mut()
                .fail("No se pudo guardar la preferencia de la clave principal.");
            return;
        }
        self.apply_primary_key(Some(key));
    }

    fn store_primary_key(
        mut self: Pin<&mut Self>,
        primary_key: &QString,
        forget_on_lock: bool,
    ) -> bool {
        let Ok(fingerprint) = KeyFingerprint::parse(&primary_key.to_string()) else {
            self.as_mut()
                .fail("La clave elegida para la bóveda no es válida.");
            return false;
        };
        let key = crate::storage::PrimaryKey {
            fingerprint,
            forget_on_lock,
        };
        if open_storage()
            .and_then(|mut storage| storage.set_primary_key(&key))
            .is_err()
        {
            self.as_mut()
                .fail("No se pudo guardar la clave principal de la bóveda.");
            return false;
        }
        self.apply_primary_key(Some(key));
        true
    }

    fn apply_setup_state(mut self: Pin<&mut Self>, completed: bool) {
        if *self.setup_required() == completed {
            self.as_mut().rust_mut().setup_required = !completed;
            self.as_mut().setup_required_changed();
        }
    }

    fn apply_primary_key(mut self: Pin<&mut Self>, key: Option<crate::storage::PrimaryKey>) {
        let value = QString::from(key.as_ref().map_or("", |key| key.fingerprint.as_str()));
        let forget = key.as_ref().is_some_and(|key| key.forget_on_lock);
        self.as_mut().rust_mut().primary = key;
        if *self.primary_key() != value {
            self.as_mut().rust_mut().primary_key = value;
            self.as_mut().primary_key_changed();
        }
        if *self.forget_on_lock() != forget {
            self.as_mut().rust_mut().forget_on_lock = forget;
            self.forget_on_lock_changed();
        }
    }

    /// Clears gpg-agent's cache for the primary key when the vault asks for it.
    fn forget_primary_passphrase(self: Pin<&mut Self>) {
        let Some(key) = self
            .as_ref()
            .rust()
            .primary
            .clone()
            .filter(|key| key.forget_on_lock)
        else {
            return;
        };
        std::thread::spawn(move || {
            if let Err(error) = GnuPgOpenPgp.forget_cached_passphrase(&key.fingerprint) {
                tracing::warn!(%error, "could not make gpg-agent forget the primary key");
            }
        });
    }

    fn cancel_unlock(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.unlock_operation = state.unlock_operation.wrapping_add(1);
        }
        self.set_unlocking(false);
    }

    pub fn set_session_blocked(mut self: Pin<&mut Self>, blocked: bool) {
        self.as_mut().rust_mut().session_blocked = blocked;
    }

    pub fn lock_for_session(mut self: Pin<&mut Self>) {
        // In-flight writes may finish, but access and read projections expire now.
        self.as_mut().cancel_unlock();
        self.as_mut().set_locked(true);
        self.as_mut().forget_primary_passphrase();
        self.as_mut().clear_sensitive_state_contents();
        unsafe { super::native_ffi::ffi::clear_owned() };
        self.as_mut().clear_error();
        self.as_mut().set_status(QString::from("Bóveda bloqueada"));
        self.as_mut().vault_locked();
    }

    pub fn clear_sensitive_state(mut self: Pin<&mut Self>) {
        if *self.busy() || *self.saving() {
            return;
        }
        self.as_mut().cancel_unlock();
        self.as_mut().clear_sensitive_state_contents();
        self.as_mut().set_locked(true);
        self.as_mut().forget_primary_passphrase();
        self.as_mut().vault_locked();
    }

    pub fn select_entry(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        let operation = {
            let mut state = self.as_mut().rust_mut();
            state.selection_operation = state.selection_operation.wrapping_add(1);
            state
                .selection_cancel_epoch
                .store(state.selection_operation, Ordering::Release);
            state.selection_operation
        };
        self.as_mut().clear_detail();
        self.as_mut().clear_detail_error();

        if *self.locked() {
            self.as_mut().rust_mut().pending_selection = None;
            self.as_mut().set_detail_busy(false);
            return;
        }

        if id.is_empty() {
            self.as_mut().rust_mut().pending_selection = None;
            self.as_mut().set_detail_busy(false);
            return;
        }

        let Some(secret_id) = parse_secret_id(&id) else {
            self.as_mut().rust_mut().pending_selection = None;
            self.as_mut().set_detail_busy(false);
            self.as_mut()
                .fail_detail("No se pudo abrir el elemento seleccionado.");
            return;
        };
        self.as_mut().rust_mut().selected_entry = Some(secret_id);
        self.as_mut().set_detail_busy(true);

        if self.as_ref().rust().selection_worker_active {
            self.as_mut().rust_mut().pending_selection = Some((secret_id, operation));
            return;
        }

        self.as_mut().rust_mut().selection_worker_active = true;
        self.start_selection_read(secret_id, operation);
    }

    fn start_selection_read(self: Pin<&mut Self>, secret_id: SecretId, operation: u64) {
        let cancel_epoch = self.as_ref().rust().selection_cancel_epoch.clone();
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<Option<(SecretContent, u64)>> {
                let (content, revision) = load_detail_content(secret_id)?;
                if cancel_epoch.load(Ordering::Acquire) == operation {
                    Ok(Some((content, revision)))
                } else {
                    drop(content);
                    Ok(None)
                }
            },
            move |mut controller, result| {
                let pending = {
                    let mut state = controller.as_mut().rust_mut();
                    state.selection_worker_active = false;
                    state.pending_selection.take()
                };

                if let Some((pending_id, pending_operation)) = pending {
                    controller.as_mut().rust_mut().selection_worker_active = true;
                    controller
                        .as_mut()
                        .start_selection_read(pending_id, pending_operation);
                    return;
                }

                if controller.as_ref().rust().selection_operation != operation
                    || controller.as_ref().rust().selected_entry != Some(secret_id)
                {
                    return;
                }

                controller.as_mut().set_detail_busy(false);
                match result {
                    Ok(Some((content, revision))) => {
                        let detail = content_projection(secret_id, &content, false, unix_seconds());
                        match detail {
                            Ok(detail) => {
                                controller.as_mut().clear_detail_error();
                                {
                                    let mut state = controller.as_mut().rust_mut();
                                    state.detail_content = Some(content);
                                    state.selected_revision = Some(revision);
                                }
                                controller
                                    .as_mut()
                                    .set_detail_json(QString::from(detail.as_str()));
                                controller
                                    .as_mut()
                                    .set_status(QString::from("Elemento cargado"));
                            }
                            Err(_) => controller
                                .as_mut()
                                .fail_detail("No se pudo mostrar el contenido del elemento."),
                        }
                    }
                    Ok(None) => (),
                    Err(_) => controller
                        .as_mut()
                        .fail_detail("No se pudo abrir el contenido del elemento."),
                }
            },
        );
    }

    pub fn open_entry(mut self: Pin<&mut Self>, id: &QString) {
        if *self.busy() || *self.saving() || *self.locked() {
            return;
        }
        let id = id.to_string();
        let operation = self.as_mut().begin_editor_operation();
        self.as_mut().clear_editor_projection();
        self.as_mut().set_editor_busy(false);
        self.as_mut().clear_error();

        if id.is_empty() {
            let draft = match new_editor_projection() {
                Ok(draft) => draft,
                Err(_) => {
                    self.as_mut()
                        .fail_detail("No se pudo preparar el elemento para editarlo.");
                    return;
                }
            };
            self.as_mut().rust_mut().editor_session = Some(EditorSession {
                id: None,
                revision: None,
            });
            self.as_mut().set_editor_json(QString::from(draft.as_str()));
            self.as_mut().set_editor_busy(false);
            self.as_mut().editor_ready();
            return;
        }

        let Some(secret_id) = parse_secret_id(&id) else {
            self.as_mut()
                .fail_detail("No se pudo abrir el elemento para editarlo.");
            return;
        };
        self.as_mut().set_editor_busy(true);
        self.as_mut()
            .set_status(QString::from("Abriendo elemento…"));

        let cancel_epoch = self.as_ref().rust().editor_cancel_epoch.clone();
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<Option<OpenedEntry>> {
                let entry = load_editor_entry(secret_id)?;
                if cancel_epoch.load(Ordering::Acquire) == operation {
                    Ok(Some(entry))
                } else {
                    drop(entry);
                    Ok(None)
                }
            },
            move |mut controller, result| {
                if controller.as_ref().rust().editor_operation != operation {
                    return;
                }

                controller.as_mut().set_editor_busy(false);
                match result {
                    Ok(Some(entry)) => {
                        let draft = editor_projection(&entry);
                        match draft {
                            Ok(draft) => {
                                if let SecretContent::SshKey(content) = &entry.content {
                                    controller.as_mut().rust_mut().editor_ssh =
                                        Some(content.clone());
                                }
                                controller.as_mut().rust_mut().editor_session =
                                    Some(EditorSession {
                                        id: Some(entry.id),
                                        revision: Some(entry.revision),
                                    });
                                controller
                                    .as_mut()
                                    .set_editor_json(QString::from(draft.as_str()));
                                controller
                                    .as_mut()
                                    .set_status(QString::from("Elemento listo para editar"));
                                controller.as_mut().editor_ready();
                            }
                            Err(_) => controller
                                .as_mut()
                                .fail_detail("No se pudo preparar el elemento para editarlo."),
                        }
                    }
                    Ok(None) => (),
                    Err(_) => controller
                        .as_mut()
                        .fail_detail("No se pudo abrir el elemento para editarlo."),
                }
            },
        );
    }

    pub fn save_entry(mut self: Pin<&mut Self>, draft_json: &QString) {
        if *self.locked() {
            self.as_mut()
                .fail("Desbloquea la bóveda antes de guardar un elemento.");
            return;
        }
        if *self.busy() {
            return;
        }

        let Some(session) = self.as_ref().rust().editor_session else {
            self.as_mut()
                .fail("No hay un borrador abierto para guardar.");
            return;
        };
        if *self.editor_busy() || *self.icon_busy() {
            return;
        }

        let draft_json = SensitiveText::from(draft_json.to_string());
        if draft_json.len() > kryptopass_core::MAX_CONTENT_BYTES {
            self.as_mut()
                .fail("El borrador supera el tamaño permitido.");
            return;
        }
        let mut draft: EntryDraft = match serde_json::from_str(draft_json.as_str()) {
            Ok(draft) => draft,
            Err(_) => {
                self.as_mut().fail("El borrador del elemento no es válido.");
                return;
            }
        };

        if !draft_matches_session(&draft, session.id, session.revision) {
            self.as_mut()
                .fail("El borrador ya no corresponde al elemento abierto.");
            return;
        }

        if let Some(token) = draft.ssh_import_token() {
            let object = self.as_ref();
            let state = object.rust();
            let content = if token.is_empty() {
                state.editor_ssh.clone()
            } else {
                state
                    .staged_ssh
                    .as_ref()
                    .filter(|(staged_token, _)| staged_token == token)
                    .map(|(_, content)| content.clone())
            };
            let Some(content) = content else {
                self.as_mut()
                    .fail("Importa una clave SSH antes de guardar el elemento.");
                return;
            };
            draft.attach_ssh(content);
        }
        let prepared = match prepare_save(&mut draft, session.id, session.revision) {
            Ok(prepared) => prepared,
            Err(_) => {
                self.as_mut()
                    .fail("Revisa los datos del elemento y quién tiene acceso.");
                return;
            }
        };
        let operation = self.as_ref().rust().editor_operation;
        let staged_icon = self.as_ref().rust().staged_icon.clone();
        self.as_mut().start_mutation(
            "Guardando elemento…",
            true,
            "No se pudo guardar el elemento.",
            move || {
                let mut service = open_service()?;
                let request = kryptopass_core::SaveContentRequest {
                    id: prepared.id,
                    folder_id: prepared.folder,
                    title: prepared.title,
                    recipients: prepared.recipients,
                    content: prepared.content,
                    expected_revision: prepared.expected_revision,
                };
                let appearance = prepared.icon_update.map(|icon| {
                    let asset = staged_icon.filter(|asset| {
                        icon == Some(kryptopass_core::EntryIcon::Image { asset_id: asset.id })
                    });
                    kryptopass_core::EntryAppearanceUpdate { icon, asset }
                });
                let id = if let Some(related) = prepared.link_update {
                    service.save_content_with_link(request, appearance, related)?
                } else if let Some(appearance) = appearance {
                    service.save_content_with_appearance(request, appearance)?
                } else {
                    service.save_content(
                        request.id,
                        request.folder_id,
                        request.title,
                        request.recipients,
                        request.content,
                        request.expected_revision,
                    )?
                };
                let snapshot = snapshot_after_mutation();
                Ok(MutationResult {
                    snapshot,
                    signal: MutationSignal::EntrySaved(id.0.to_string()),
                    editor_operation: Some(operation),
                })
            },
        );
    }

    pub fn prepare_entry_icon(mut self: Pin<&mut Self>, path: &QUrl) {
        if *self.locked()
            || *self.busy()
            || *self.editor_busy()
            || *self.icon_busy()
            || self.as_ref().rust().editor_session.is_none()
        {
            return;
        }
        let Some(path) = path.to_local_file() else {
            self.as_mut().fail("Elige una imagen local.");
            return;
        };
        let operation = self.as_ref().rust().editor_operation;
        self.as_mut().set_icon_busy(true);
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = crate::infrastructure::entry_icons::prepare_image(std::path::Path::new(
                &path.to_string(),
            ));
            let _ = qt_thread.queue(move |mut controller| {
                if controller.as_ref().rust().editor_operation != operation { return; }
                controller.as_mut().set_icon_busy(false);
                match result {
                    Ok(asset) => {
                        crate::infrastructure::entry_icons::stage(&asset);
                        let icon = kryptopass_core::EntryIcon::Image { asset_id: asset.id };
                        let json = serde_json::json!({ "icon": icon, "iconSource": crate::infrastructure::entry_icons::source_for(Some(&icon)) }).to_string();
                        controller.as_mut().rust_mut().staged_icon = Some(asset);
                        controller.as_mut().icon_prepared(&QString::from(json));
                    }
                    Err(_) => controller.as_mut().fail("No se pudo usar esa imagen. Elige un PNG, JPEG o WebP estático de hasta 4 MB, con lados de hasta 4096 píxeles y un máximo de 4 millones de píxeles."),
                }
            });
        });
    }

    fn can_read_detail(self: Pin<&Self>) -> bool {
        let state = self.rust();
        !*self.locked()
            && !*self.busy()
            && !*self.saving()
            && !*self.detail_busy()
            && state.editor_session.is_none()
            && state.selected_revision.is_some()
            && state.detail_content.is_some()
    }

    fn publish_detail(mut self: Pin<&mut Self>) {
        let projection = {
            let object = self.as_ref();
            let state = object.rust();
            state
                .selected_entry
                .zip(state.detail_content.as_ref())
                .map(|(id, content)| {
                    content_projection_with_state(
                        id,
                        content,
                        state.password_revealed,
                        unix_seconds(),
                        &state.revealed_fields,
                        state.qr_source.as_ref().map(|value| value.as_str()),
                    )
                })
        };
        if let Some(Ok(projection)) = projection {
            self.as_mut()
                .set_detail_json(QString::from(projection.as_str()));
        }
    }

    pub fn toggle_field(mut self: Pin<&mut Self>, field: &QString) {
        if !self.as_ref().can_read_detail() {
            return;
        }
        let id = field.to_string();
        let allowed = self
            .as_ref()
            .rust()
            .detail_content
            .as_ref()
            .and_then(|content| content_field(content, &id))
            .is_some_and(|(_, concealed)| concealed);
        if !allowed {
            return;
        }
        {
            let mut state = self.as_mut().rust_mut();
            if !state.revealed_fields.remove(&id) {
                state.revealed_fields.insert(id);
            }
        }
        self.publish_detail();
    }

    pub fn copy_field(mut self: Pin<&mut Self>, field: &QString) {
        if !self.as_ref().can_read_detail() {
            return;
        }
        let value = self
            .as_ref()
            .rust()
            .detail_content
            .as_ref()
            .and_then(|content| content_field(content, &field.to_string()))
            .filter(|(value, _)| !value.is_empty())
            .map(|(value, _)| value.into_owned());
        if let Some(value) = value {
            self.as_mut().request_copy(CopyKind::Password, value);
        }
    }

    pub fn show_qr(mut self: Pin<&mut Self>) {
        if !self.as_ref().can_read_detail() {
            return;
        }
        if self.as_ref().rust().qr_source.is_some() {
            self.as_mut().rust_mut().qr_source = None;
            self.publish_detail();
            return;
        }
        let result = (|| -> anyhow::Result<SensitiveText> {
            let object = self.as_ref();
            let state = object.rust();
            let payload = match state.detail_content.as_ref() {
                Some(SecretContent::Wifi(value)) => kryptopass_core::wifi_qr_payload(value)?,
                Some(SecretContent::Totp(value)) => {
                    kryptopass_core::format_totp_input(&value.config)?
                }
                _ => anyhow::bail!("QR unavailable for this item"),
            };
            super::secret_qr::source(&payload)
        })();
        match result {
            Ok(source) => {
                self.as_mut().rust_mut().qr_source = Some(source);
                self.publish_detail();
            }
            Err(_) => self.as_mut().fail(
                "No se pudo generar el QR. El contenido puede superar la capacidad del código.",
            ),
        }
    }

    pub fn prepare_ssh_key(mut self: Pin<&mut Self>, path: &QUrl, passphrase: &QString) {
        if *self.locked()
            || *self.busy()
            || *self.saving()
            || *self.editor_busy()
            || self.as_ref().rust().editor_session.is_none()
        {
            return;
        }
        let Some(path) = path.to_local_file() else {
            self.as_mut()
                .fail("Selecciona un archivo local de clave privada SSH.");
            return;
        };
        let path = std::path::PathBuf::from(path.to_string());
        let passphrase = SensitiveText::from(passphrase.to_string());
        let (editor_operation, prepare_operation) = {
            let mut state = self.as_mut().rust_mut();
            state.prepare_operation = state.prepare_operation.wrapping_add(1);
            (state.editor_operation, state.prepare_operation)
        };
        self.as_mut().clear_error();
        self.as_mut().set_editor_busy(true);
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<_> {
                use std::io::Read;
                let file = std::fs::File::open(path)?;
                anyhow::ensure!(file.metadata()?.is_file(), "select a regular key file");
                let mut raw = SensitiveBytes::default();
                std::io::copy(&mut file.take(256 * 1024 + 1), &mut raw)?;
                let bytes = SensitiveText::from_utf8(raw)?;
                anyhow::ensure!(bytes.len() <= 256 * 1024, "SSH key exceeds size limit");
                kryptopass_core::SshKeyContent::import(bytes, passphrase)
            },
            move |mut controller, result| {
                let object = controller.as_ref();
                let state = object.rust();
                if state.editor_operation != editor_operation
                    || state.prepare_operation != prepare_operation
                    || state.editor_session.is_none()
                    || *controller.locked()
                {
                    return;
                }
                controller.as_mut().set_editor_busy(false);
                match result {
                Ok(content) => {
                    let token = Uuid::new_v4().to_string();
                    let json = serde_json::json!({"importToken": token, "publicKey": content.public_key,
                        "fingerprint": content.fingerprint, "algorithm": content.algorithm}).to_string();
                    controller.as_mut().rust_mut().staged_ssh = Some((token, content));
                    controller.as_mut().ssh_key_prepared(&QString::from(json));
                }
                Err(_) => controller.as_mut().fail("No se pudo importar la clave SSH. Usa una clave privada OpenSSH válida y revisa su frase de contraseña."),
            }
            },
        );
    }

    pub fn prepare_totp_qr(mut self: Pin<&mut Self>, path: &QUrl) {
        if *self.locked()
            || *self.busy()
            || *self.saving()
            || *self.editor_busy()
            || self.as_ref().rust().editor_session.is_none()
        {
            return;
        }
        let Some(path) = path.to_local_file() else {
            self.as_mut()
                .fail("Selecciona una imagen local con el QR del autenticador.");
            return;
        };
        let path = std::path::PathBuf::from(path.to_string());
        let (editor_operation, prepare_operation) = {
            let mut state = self.as_mut().rust_mut();
            state.prepare_operation = state.prepare_operation.wrapping_add(1);
            (state.editor_operation, state.prepare_operation)
        };
        self.as_mut().clear_error();
        self.as_mut().set_editor_busy(true);
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || super::secret_qr::read_totp(&path),
            move |mut controller, result| {
                let object = controller.as_ref();
                let state = object.rust();
                if state.editor_operation != editor_operation
                    || state.prepare_operation != prepare_operation
                    || state.editor_session.is_none()
                    || *controller.locked()
                {
                    return;
                }
                controller.as_mut().set_editor_busy(false);
                match result {
                Ok(uri) => {
                    #[derive(serde::Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct PreparedTotp<'a> {
                        totp_input: &'a str,
                    }
                    let prepared = PreparedTotp { totp_input: uri.as_str() };
                    let Ok(json) = super::secrets_projection::sensitive_json(&prepared) else {
                        controller.as_mut().fail("No se pudo preparar la configuración TOTP.");
                        return;
                    };

                    controller.as_mut().totp_qr_prepared(&QString::from(json.as_str()));
                }
                Err(_) => controller.as_mut().fail("No se pudo leer el autenticador. Elige una imagen PNG o JPEG de hasta 8 MB con un único QR TOTP válido."),
            }
            },
        );
    }

    pub fn clear_editor(mut self: Pin<&mut Self>) {
        if *self.saving() {
            return;
        }
        self.as_mut().begin_editor_operation();
        self.as_mut().clear_editor_projection();
        self.as_mut().set_editor_busy(false);
    }

    pub fn toggle_password(mut self: Pin<&mut Self>) {
        if *self.locked() || *self.busy() {
            return;
        }
        let (id, revealed) = {
            let mut state = self.as_mut().rust_mut();
            let Some(id) = state.selected_entry else {
                return;
            };
            if !matches!(state.detail_content.as_ref(), Some(SecretContent::Login(_))) {
                return;
            }
            state.password_revealed = !state.password_revealed;
            (id, state.password_revealed)
        };

        let projection = {
            let object = self.as_ref();
            let state = object.rust();
            state
                .detail_content
                .as_ref()
                .map(|content| content_projection(id, content, revealed, unix_seconds()))
        };
        if let Some(Ok(detail)) = projection {
            self.as_mut()
                .set_detail_json(QString::from(detail.as_str()));
        }
    }

    pub fn copy_username(mut self: Pin<&mut Self>) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(content) = state.detail_content.as_ref() else {
                return;
            };
            match content {
                SecretContent::Login(login) if !login.username.is_empty() => login.username.clone(),
                _ => return,
            }
        };
        self.as_mut().request_copy(CopyKind::Username, value);
    }

    pub fn copy_password(mut self: Pin<&mut Self>) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(content) = state.detail_content.as_ref() else {
                return;
            };
            match content {
                SecretContent::Login(login) if !login.password.is_empty() => login.password.clone(),
                _ => return,
            }
        };
        self.as_mut().request_copy(CopyKind::Password, value);
    }

    pub fn open_history_version(mut self: Pin<&mut Self>, index: i32) {
        self.as_mut().close_history_version();
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let projection = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(SecretContent::Login(login)) = state.detail_content.as_ref() else {
                return;
            };
            let Some(version) = login.history.version_content(index) else {
                return;
            };
            let Some((at, password_only)) = login.history.version_metadata().get(index).copied()
            else {
                return;
            };
            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct VersionProjection<'a> {
                at: u64,
                password_only: bool,
                title: Option<&'a str>,
                username: &'a str,
                urls: &'a [SensitiveText],
                notes: &'a str,
                has_password: bool,
                has_totp: bool,
            }
            let projection = VersionProjection {
                at,
                password_only,
                title: login.history.version_title(index),
                username: &version.username,
                urls: &version.urls,
                notes: &version.notes,
                has_password: !version.password.is_empty(),
                has_totp: version.totp.is_some(),
            };
            super::secrets_projection::sensitive_json(&projection)
        };
        if let Ok(projection) = projection {
            self.as_mut().rust_mut().history_version_index = Some(index);
            self.set_version_json(QString::from(projection.as_str()));
        }
    }

    pub fn close_history_version(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().history_version_index = None;
        self.set_version_json(QString::default());
    }

    pub fn copy_history_value(mut self: Pin<&mut Self>, field: &QString) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(index) = state.history_version_index else {
                return;
            };
            let Some(SecretContent::Login(login)) = state.detail_content.as_ref() else {
                return;
            };
            let Some(version) = login.history.version_content(index) else {
                return;
            };
            match field.to_string().as_str() {
                "password" => version.password.clone(),
                "username" => version.username.clone(),
                "title" => SensitiveText::from(
                    login
                        .history
                        .version_title(index)
                        .unwrap_or_default()
                        .to_owned(),
                ),
                "urls" => {
                    let mut text = SensitiveText::new();
                    for (index, url) in version.urls.iter().enumerate() {
                        if index != 0 {
                            text.push('\n');
                        }
                        text.push_str(url);
                    }
                    text
                }
                "notes" => version.notes.clone(),
                "totp" => {
                    let Some(config) = version.totp.as_ref() else {
                        return;
                    };
                    let Ok(value) = kryptopass_core::format_totp_input(config) else {
                        return;
                    };
                    value
                }
                _ => return,
            }
        };
        if value.is_empty() {
            return;
        }
        self.as_mut().rust_mut().pending_copy = None;
        self.clipboard_requested(&QString::from(value.as_str()));
    }

    pub fn export_activity(self: Pin<&mut Self>) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let report = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(SecretContent::Login(login)) = state.detail_content.as_ref() else {
                return;
            };
            activity_report(&login.history)
        };
        match report.and_then(|report| write_activity_report(&report)) {
            Ok(path) => self.activity_exported(&QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()))),
            Err(_) => self.fail("No se pudo generar el archivo de actividad. Comprueba el espacio disponible e inténtalo de nuevo."),
        }
    }

    pub fn refresh_totp(mut self: Pin<&mut Self>) {
        if *self.locked() || *self.busy() {
            return;
        }
        let (id, projection) = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(id) = state.selected_entry else {
                return;
            };
            let Some(content) = state.detail_content.as_ref() else {
                return;
            };
            if !matches!(content, SecretContent::Login(_) | SecretContent::Totp(_)) {
                return;
            }
            (
                id,
                content_projection_with_state(
                    id,
                    content,
                    state.password_revealed,
                    unix_seconds(),
                    &state.revealed_fields,
                    state.qr_source.as_ref().map(|value| value.as_str()),
                ),
            )
        };
        if let Ok(projection) = projection {
            if self.as_ref().rust().selected_entry == Some(id) {
                self.as_mut()
                    .set_detail_json(QString::from(projection.as_str()));
            }
        }
    }

    pub fn copy_totp(mut self: Pin<&mut Self>, next: bool) {
        if !self.as_ref().can_read_detail() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let config = match state.detail_content.as_ref() {
                Some(SecretContent::Login(login)) => login.totp.as_ref(),
                Some(SecretContent::Totp(value)) => Some(&value.config),
                _ => None,
            };
            let Some(config) = config else {
                return;
            };
            let Ok(mut snapshot) = generate_totp(config, unix_seconds()) else {
                return;
            };
            if next {
                std::mem::take(&mut snapshot.next)
            } else {
                std::mem::take(&mut snapshot.current)
            }
        };
        self.as_mut().request_copy(
            if next {
                CopyKind::NextTotp
            } else {
                CopyKind::Totp
            },
            value,
        );
    }

    pub fn confirm_copy(mut self: Pin<&mut Self>, copied: bool) {
        let pending = self.as_mut().rust_mut().pending_copy.take();
        if !copied {
            return;
        }
        let Some(pending) = pending else {
            return;
        };
        let valid = {
            let object = self.as_ref();
            let state = object.rust();
            !*self.locked()
                && state.selection_operation == pending.selection_operation
                && state.selected_entry == Some(pending.id)
                && state.editor_session.is_none()
                && state.selected_revision.is_some()
                && state.detail_content.is_some()
        };
        if !valid || *self.busy() {
            return;
        }
        if matches!(
            self.as_ref().rust().detail_content,
            Some(SecretContent::Login(_))
        ) {
            self.as_mut().start_copy_history_record(pending);
        }
    }

    fn can_copy_login_value(self: Pin<&Self>) -> bool {
        let state = self.rust();
        !*self.locked()
            && !*self.busy()
            && !*self.saving()
            && state.editor_session.is_none()
            && state.selected_revision.is_some()
            && matches!(state.detail_content.as_ref(), Some(SecretContent::Login(_)))
    }

    fn request_copy(mut self: Pin<&mut Self>, kind: CopyKind, value: SensitiveText) {
        {
            let mut state = self.as_mut().rust_mut();
            let Some(id) = state.selected_entry else {
                return;
            };
            if state.selected_revision.is_none() || state.editor_session.is_some() {
                return;
            }
            let pending = PendingCopy {
                id,
                selection_operation: state.selection_operation,
                kind,
            };
            state.pending_copy = Some(pending);
        }
        let value = QString::from(value.as_str());
        self.as_mut().clipboard_requested(&value);
    }

    fn start_copy_history_record(mut self: Pin<&mut Self>, pending: PendingCopy) {
        let copy_snapshot = {
            let object = self.as_ref();
            let state = object.rust();
            state
                .selected_revision
                .zip(state.detail_content.as_ref().cloned())
        };
        let Some((revision, content_snapshot)) = copy_snapshot else {
            self.as_mut().history_record_failed();
            return;
        };
        let kind = match pending.kind {
            CopyKind::Username => LoginHistoryEventKind::CopyUsername,
            CopyKind::Password => LoginHistoryEventKind::CopyPassword,
            CopyKind::Totp => LoginHistoryEventKind::CopyTotp,
            CopyKind::NextTotp => LoginHistoryEventKind::CopyNextTotp,
        };
        let timestamp = unix_seconds();
        self.as_mut().set_busy(true);

        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<CopyHistoryResult> {
                let mut service = open_service()?;
                let content = service.record_login_event(
                    pending.id,
                    revision,
                    content_snapshot,
                    kind,
                    timestamp,
                )?;
                let write_revision = revision
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("secret revision is exhausted"))?;
                let observed_revision = find_entry_metadata(pending.id)
                    .ok()
                    .map(|metadata| metadata.revision);
                let snapshot = snapshot_after_mutation();
                Ok(CopyHistoryResult {
                    content,
                    write_revision,
                    observed_revision,
                    snapshot,
                })
            },
            move |mut controller, result| {
                controller.as_mut().set_busy(false);
                match result {
                    Ok(saved) => {
                        let (has_snapshot, snapshot_revision) = match saved.snapshot {
                            Ok(snapshot) => {
                                let revision = snapshot_revision(&snapshot, pending.id);
                                controller.as_mut().publish_snapshot(snapshot);
                                (true, revision)
                            }
                            Err(()) => (false, None),
                        };

                        let still_selected = {
                            let object = controller.as_ref();
                            let state = object.rust();
                            state.selected_entry == Some(pending.id)
                                && state.selection_operation == pending.selection_operation
                                && !*controller.as_ref().locked()
                        };
                        if !still_selected {
                            return;
                        }

                        let revision_matches_snapshot = match snapshot_revision {
                            Some(snapshot_revision) => {
                                snapshot_revision == saved.write_revision
                                    && saved.observed_revision == Some(saved.write_revision)
                            }
                            None => {
                                !has_snapshot
                                    && saved.observed_revision == Some(saved.write_revision)
                            }
                        };
                        if !revision_matches_snapshot {
                            controller.as_mut().history_record_failed();
                            return;
                        }

                        let revision = snapshot_revision.unwrap_or(saved.write_revision);
                        let detail = content_projection(
                            pending.id,
                            &saved.content,
                            controller.as_ref().rust().password_revealed,
                            unix_seconds(),
                        );
                        if let Ok(detail) = detail {
                            {
                                let mut state = controller.as_mut().rust_mut();
                                state.selected_revision = Some(revision);
                                state.detail_content = Some(saved.content);
                            }
                            controller
                                .as_mut()
                                .set_detail_json(QString::from(detail.as_str()));
                        } else {
                            controller.as_mut().history_record_failed();
                        }
                    }
                    Err(_) => {
                        controller.as_mut().history_record_failed();
                    }
                }
            },
        );
    }

    pub fn save_folder(
        mut self: Pin<&mut Self>,
        id: &QString,
        name: &QString,
        parent_id: &QString,
        icon: &QString,
        color: &QString,
    ) {
        if *self.busy() {
            return;
        }
        let id = id.to_string();
        let name = name.to_string();
        let parent_id = parent_id.to_string();
        let icon = icon.to_string();
        let color = color.to_string();
        let appearance = match FolderAppearance::new(icon, color) {
            Ok(appearance) => appearance,
            Err(_) => {
                self.as_mut()
                    .fail("El icono o el color de la carpeta no son válidos.");
                return;
            }
        };
        let folder_id = if id.is_empty() {
            None
        } else {
            match parse_folder_id(&id) {
                Some(id) => Some(id),
                None => {
                    self.as_mut().fail("La carpeta seleccionada no es válida.");
                    return;
                }
            }
        };
        let parent_id = if folder_id.is_some() || parent_id.is_empty() {
            None
        } else {
            match parse_folder_id(&parent_id) {
                Some(id) => Some(id),
                None => {
                    self.as_mut().fail("La carpeta de destino no es válida.");
                    return;
                }
            }
        };

        self.as_mut().start_mutation(
            "Guardando carpeta…",
            false,
            "No se pudo guardar la carpeta.",
            move || {
                let mut service = open_service()?;
                let saved_id = match folder_id {
                    Some(id) => {
                        service.update_folder_details(id, name, appearance)?;
                        id
                    }
                    None => {
                        service.create_folder_with_appearance(name, parent_id, None, appearance)?
                    }
                };
                let snapshot = snapshot_after_mutation();
                Ok(MutationResult {
                    snapshot,
                    signal: MutationSignal::FolderSaved(saved_id.0.to_string()),
                    editor_operation: None,
                })
            },
        );
    }

    pub fn step_folder(mut self: Pin<&mut Self>, id: &QString, direction: i32) {
        if *self.busy() || *self.locked() || ![-1, 1].contains(&direction) {
            return;
        }
        let Some(id) = parse_folder_id(&id.to_string()) else {
            self.as_mut().fail("La carpeta seleccionada no es válida.");
            return;
        };
        self.as_mut().start_mutation(
            "Ordenando carpeta…",
            false,
            "No se pudo cambiar el orden de la carpeta.",
            move || {
                let mut service = open_service()?;
                step_folder_in_service(&mut service, id, direction)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::None,
                    editor_operation: None,
                })
            },
        );
    }

    pub fn move_folder(
        mut self: Pin<&mut Self>,
        id: &QString,
        parent_id: &QString,
        before_id: &QString,
    ) {
        if *self.busy() || *self.locked() {
            return;
        }
        let Some(id) = parse_folder_id(&id.to_string()) else {
            self.as_mut().fail("La carpeta seleccionada no es válida.");
            return;
        };
        let optional_id = |value: &QString| -> anyhow::Result<Option<FolderId>> {
            let value = value.to_string();
            if value.is_empty() {
                Ok(None)
            } else {
                parse_folder_id(&value)
                    .map(Some)
                    .ok_or_else(|| anyhow::anyhow!("invalid folder"))
            }
        };
        let (Ok(parent), Ok(before)) = (optional_id(parent_id), optional_id(before_id)) else {
            self.as_mut().fail("La carpeta de destino no es válida.");
            return;
        };
        self.as_mut().start_mutation(
            "Moviendo carpeta…", false,
            "No se pudo mover la carpeta. No puede colocarse dentro de sí misma ni de sus descendientes.",
            move || {
                open_service()?.move_folder(id, parent, before)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::None,
                    editor_operation: None,
                })
            },
        );
    }

    pub fn move_entry(mut self: Pin<&mut Self>, id: &QString, folder_id: &QString) {
        if *self.busy() {
            return;
        }
        let Some(id) = parse_secret_id(&id.to_string()) else {
            self.as_mut().fail("El elemento seleccionado no es válido.");
            return;
        };
        let folder_id = folder_id.to_string();
        let folder_id = if folder_id.is_empty() {
            None
        } else {
            match parse_folder_id(&folder_id) {
                Some(id) => Some(id),
                None => {
                    self.as_mut().fail("La carpeta de destino no es válida.");
                    return;
                }
            }
        };

        self.as_mut().start_mutation(
            "Moviendo elemento…",
            false,
            "No se pudo mover el elemento.",
            move || {
                let mut service = open_service()?;
                let before_revision = find_entry_metadata(id)?.revision;
                service.move_secret(id, folder_id)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::MetadataChanged(id, before_revision),
                    editor_operation: None,
                })
            },
        );
    }

    pub fn set_favorite(mut self: Pin<&mut Self>, id: &QString, favorite: bool) {
        self.as_mut().update_flags(id, Some(favorite), None);
    }

    pub fn set_trashed(mut self: Pin<&mut Self>, id: &QString, trashed: bool) {
        self.as_mut().update_flags(id, None, Some(trashed));
    }

    pub fn remove_entry(mut self: Pin<&mut Self>, id: &QString) {
        if *self.busy() {
            return;
        }
        let Some(id) = parse_secret_id(&id.to_string()) else {
            self.as_mut().fail("El elemento seleccionado no es válido.");
            return;
        };

        // Invalidate an outstanding read before deleting, rather than waiting
        // for the mutation callback to remove its plaintext projection.
        self.as_mut().invalidate_selection();
        self.as_mut().start_mutation(
            "Eliminando elemento…",
            false,
            "No se pudo eliminar el elemento.",
            move || {
                let mut service = open_service()?;
                service.remove_secret(id)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::EntryRemoved(id),
                    editor_operation: None,
                })
            },
        );
    }

    pub fn remove_folder(mut self: Pin<&mut Self>, id: &QString, delete_contents: bool) {
        if *self.busy() {
            return;
        }
        let Some(id) = parse_folder_id(&id.to_string()) else {
            self.as_mut().fail("La carpeta seleccionada no es válida.");
            return;
        };

        let selected_id = self.as_ref().rust().selected_entry;
        if delete_contents {
            // Cancel outstanding reads before their source entries can disappear.
            self.as_mut().invalidate_selection();
        }
        self.as_mut().start_mutation(
            "Eliminando carpeta…",
            false,
            "No se pudo eliminar la carpeta.",
            move || {
                let mut service = open_service()?;
                let affected_selection =
                    selected_id
                        .filter(|_| !delete_contents)
                        .and_then(|selected| {
                            find_entry_metadata(selected)
                                .ok()
                                .filter(|entry| entry.folder == Some(id))
                                .map(|entry| (selected, entry.revision))
                        });
                if delete_contents {
                    service.remove_folder_tree(id)?;
                } else {
                    service.remove_folder(id)?;
                }
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: affected_selection
                        .map_or(MutationSignal::None, |(selected, revision)| {
                            MutationSignal::MetadataChanged(selected, revision)
                        }),
                    editor_operation: None,
                })
            },
        );
    }

    pub fn generate_password(
        mut self: Pin<&mut Self>,
        length: i32,
        lowercase: bool,
        uppercase: bool,
        digits: bool,
        symbols: bool,
        excluded: &QString,
    ) {
        if *self.locked() {
            self.as_mut()
                .fail("Desbloquea la bóveda antes de generar una contraseña.");
            return;
        }
        let Ok(length) = usize::try_from(length) else {
            self.as_mut().fail("No se pudo generar la contraseña.");
            return;
        };
        let options = PasswordOptions {
            length,
            lowercase,
            uppercase,
            digits,
            symbols,
            excluded: excluded.to_string(),
        };
        match generate_secure_password(&options) {
            Ok(password) => {
                let password = QString::from(password.as_str());
                self.password_generated(&password);
            }
            Err(_) => self.as_mut().fail("No se pudo generar la contraseña."),
        }
    }

    fn update_flags(
        mut self: Pin<&mut Self>,
        id: &QString,
        favorite: Option<bool>,
        trashed: Option<bool>,
    ) {
        if *self.busy() {
            return;
        }
        let Some(id) = parse_secret_id(&id.to_string()) else {
            self.as_mut().fail("El elemento seleccionado no es válido.");
            return;
        };

        self.as_mut().start_mutation(
            "Actualizando elemento…",
            false,
            "No se pudo actualizar el elemento.",
            move || {
                let mut service = open_service()?;
                let before_revision = find_entry_metadata(id)?.revision;
                service.update_flags(id, favorite, trashed)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::MetadataChanged(id, before_revision),
                    editor_operation: None,
                })
            },
        );
    }

    fn start_mutation<Work>(
        mut self: Pin<&mut Self>,
        status: &'static str,
        saving: bool,
        failure_message: &'static str,
        work: Work,
    ) where
        Work: FnOnce() -> anyhow::Result<MutationResult> + Send + 'static,
    {
        if *self.locked() {
            self.as_mut()
                .fail("Desbloquea la bóveda antes de modificarla.");
            return;
        }
        if *self.busy() {
            return;
        }
        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut().set_saving(saving);
        self.as_mut().set_status(QString::from(status));

        crate::qt::jobs::run_blocking(self.qt_thread(), work, move |mut controller, result| {
            controller.as_mut().set_busy(false);
            controller.as_mut().set_saving(false);
            match result {
                Ok(result) => {
                    if let MutationSignal::MetadataChanged(id, before_revision) = &result.signal {
                        let next_revision = before_revision.checked_add(1);
                        let stored_revision = result
                            .snapshot
                            .as_ref()
                            .ok()
                            .and_then(|snapshot| snapshot_revision(snapshot, *id));
                        let mut state = controller.as_mut().rust_mut();
                        if state.selected_entry == Some(*id)
                            && state.selected_revision == Some(*before_revision)
                            && next_revision.is_some()
                            && stored_revision == next_revision
                        {
                            state.selected_revision = next_revision;
                        }
                    }
                    match result.snapshot {
                        Ok(snapshot) => {
                            controller.as_mut().publish_snapshot(snapshot);
                            controller
                                .as_mut()
                                .set_status(QString::from("Cambios guardados"));
                        }
                        Err(()) => {
                            controller.as_mut().set_operation_error(QString::from(
                                "Cambio guardado; no se pudo actualizar la lista. Vuelve a cargarla.",
                            ));
                            controller.as_mut().set_status(QString::from(
                                "Cambio guardado; no se pudo actualizar la lista.",
                            ));
                        }
                    }
                    match result.signal {
                        MutationSignal::None | MutationSignal::MetadataChanged(_, _) => {}
                        MutationSignal::EntrySaved(id) => {
                            if result.editor_operation
                                == Some(controller.as_ref().rust().editor_operation)
                            {
                                controller.as_mut().clear_editor_projection();
                                controller.as_mut().set_editor_busy(false);
                                let id = QString::from(id);
                                controller.as_mut().entry_saved(&id);
                            }
                        }
                        MutationSignal::FolderSaved(id) => {
                            let id = QString::from(id);
                            controller.as_mut().folder_saved(&id);
                        }
                        MutationSignal::EntryRemoved(id) => {
                            if controller.as_ref().rust().selected_entry == Some(id) {
                                controller.as_mut().invalidate_selection();
                            }
                        }
                    }
                    controller.as_mut().operation_succeeded();
                }
                Err(error) => {
                    let message = match error.downcast_ref::<kryptopass_core::SecretLinkError>() {
                        Some(kryptopass_core::SecretLinkError::Occupied) => {
                            "El elemento elegido ya tiene un vínculo. Elige otro o quita su vínculo primero."
                        }
                        Some(kryptopass_core::SecretLinkError::InvalidTarget) => {
                            "El elemento vinculado ya no está disponible. Elige otro o selecciona Sin vínculo."
                        }
                        None => failure_message,
                    };
                    controller.as_mut().fail(message);
                }
            }
        });
    }

    fn publish_snapshot(mut self: Pin<&mut Self>, snapshot: StoreSnapshot) {
        let hosts = snapshot
            .favicon_hosts
            .into_iter()
            .map(QString::from)
            .collect();
        crate::ui_adapters::native_ffi::ffi::prefetch_favicons(&hosts);
        let entries = snapshot
            .entries
            .into_iter()
            .map(|record| QString::from(record.as_str()))
            .collect();
        let folders = snapshot
            .folders
            .into_iter()
            .map(|record| QString::from(record.as_str()))
            .collect();

        if self.entry_records() != &entries {
            self.as_mut().rust_mut().entry_records = entries;
            self.as_mut().entry_records_changed();
        }
        if self.folder_records() != &folders {
            self.as_mut().rust_mut().folder_records = folders;
            self.folder_records_changed();
        }
    }

    fn set_detail_json(mut self: Pin<&mut Self>, value: QString) {
        if self.detail_json() != &value {
            self.as_mut().rust_mut().detail_json = value;
            self.detail_json_changed();
        }
    }

    fn set_version_json(mut self: Pin<&mut Self>, value: QString) {
        if self.version_json() != &value {
            self.as_mut().rust_mut().version_json = value;
            self.version_json_changed();
        }
    }

    fn set_editor_json(mut self: Pin<&mut Self>, value: QString) {
        if self.editor_json() != &value {
            self.as_mut().rust_mut().editor_json = value;
            self.editor_json_changed();
        }
    }

    fn set_error_message(mut self: Pin<&mut Self>, value: QString) {
        let value = crate::i18n::translate(&value);
        if self.error_message() != &value {
            self.as_mut().rust_mut().error_message = value;
            self.error_message_changed();
        }
    }

    fn set_status(mut self: Pin<&mut Self>, value: QString) {
        let value = crate::i18n::translate(&value);
        if self.status() != &value {
            self.as_mut().rust_mut().status = value;
            self.status_changed();
        }
    }

    fn clear_error(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().operation_error_active = false;
        self.set_error_message(QString::default());
    }

    fn fail(mut self: Pin<&mut Self>, message: &'static str) {
        self.as_mut().set_operation_error(QString::from(message));
        self.as_mut().set_status(QString::from(message));
        self.operation_failed();
    }

    fn fail_detail(mut self: Pin<&mut Self>, message: &'static str) {
        if !self.as_ref().rust().operation_error_active {
            self.as_mut().set_error_message(QString::from(message));
        }
        self.as_mut().set_status(QString::from(message));
        self.operation_failed();
    }

    fn clear_detail_error(mut self: Pin<&mut Self>) {
        if !self.as_ref().rust().operation_error_active {
            self.as_mut().set_error_message(QString::default());
        }
    }

    fn set_operation_error(mut self: Pin<&mut Self>, value: QString) {
        self.as_mut().rust_mut().operation_error_active = true;
        self.set_error_message(value);
    }

    fn clear_detail(mut self: Pin<&mut Self>) {
        self.as_mut().close_history_version();
        let mut state = self.as_mut().rust_mut();
        state.detail_content = None;
        state.selected_entry = None;
        state.selected_revision = None;
        state.password_revealed = false;
        state.revealed_fields.clear();
        state.qr_source = None;
        state.pending_copy = None;
        self.as_mut().set_detail_json(QString::default());
    }

    fn clear_sensitive_state_contents(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.selection_operation = state.selection_operation.wrapping_add(1);
            state
                .selection_cancel_epoch
                .store(state.selection_operation, Ordering::Release);
            state.pending_selection = None;
            state.editor_operation = state.editor_operation.wrapping_add(1);
            state
                .editor_cancel_epoch
                .store(state.editor_operation, Ordering::Release);
            state.editor_session = None;
        }
        self.as_mut().clear_detail();
        self.as_mut().set_detail_busy(false);
        self.as_mut().clear_editor_projection();
        self.as_mut().set_editor_busy(false);
        tracing::debug!(state = ?kryptopass_secure_memory::memory_diagnostics(), "Sensitive memory state cleared");
    }

    fn invalidate_selection(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.selection_operation = state.selection_operation.wrapping_add(1);
            state
                .selection_cancel_epoch
                .store(state.selection_operation, Ordering::Release);
            state.pending_selection = None;
        }
        self.as_mut().clear_detail();
        self.as_mut().set_detail_busy(false);
    }

    fn clear_editor_projection(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().editor_session = None;
        {
            let mut state = self.as_mut().rust_mut();
            state.staged_icon = None;
            state.editor_ssh = None;
            state.staged_ssh = None;
            state.prepare_operation = state.prepare_operation.wrapping_add(1);
        }
        crate::infrastructure::entry_icons::clear_stage();
        self.as_mut().set_icon_busy(false);
        self.set_editor_json(QString::default());
    }

    fn begin_editor_operation(mut self: Pin<&mut Self>) -> u64 {
        let mut state = self.as_mut().rust_mut();
        state.editor_operation = state.editor_operation.wrapping_add(1);
        state
            .editor_cancel_epoch
            .store(state.editor_operation, Ordering::Release);
        state.editor_operation
    }
}

impl qobject::SecretsController {
    fn set_busy(mut self: Pin<&mut Self>, value: bool) {
        if *self.busy() != value {
            self.as_mut().rust_mut().busy = value;
            self.busy_changed();
        }
    }

    fn set_detail_busy(mut self: Pin<&mut Self>, value: bool) {
        if *self.detail_busy() != value {
            self.as_mut().rust_mut().detail_busy = value;
            self.detail_busy_changed();
        }
    }

    fn set_editor_busy(mut self: Pin<&mut Self>, value: bool) {
        if *self.editor_busy() != value {
            self.as_mut().rust_mut().editor_busy = value;
            self.editor_busy_changed();
        }
    }

    fn set_icon_busy(mut self: Pin<&mut Self>, value: bool) {
        if *self.icon_busy() != value {
            self.as_mut().rust_mut().icon_busy = value;
            self.icon_busy_changed();
        }
    }

    fn set_saving(mut self: Pin<&mut Self>, value: bool) {
        if *self.saving() != value {
            self.as_mut().rust_mut().saving = value;
            self.saving_changed();
        }
    }

    fn set_browser_pairing(mut self: Pin<&mut Self>, value: QString) {
        if self.browser_pairing() != &value {
            self.as_mut().rust_mut().browser_pairing = value;
            self.browser_pairing_changed();
        }
    }

    fn set_unlocking(mut self: Pin<&mut Self>, value: bool) {
        if *self.unlocking() != value {
            self.as_mut().rust_mut().unlocking = value;
            self.unlocking_changed();
        }
    }

    fn set_locked(mut self: Pin<&mut Self>, value: bool) {
        if *self.locked() != value {
            kryptopass_browser::set_locked(value, false);
            self.as_mut().rust_mut().locked = value;
            self.as_mut().locked_changed();
            self.as_mut().refresh_browser_pairing();
        }
    }
}

fn load_editor_entry(id: SecretId) -> anyhow::Result<OpenedEntry> {
    let service = open_service()?;
    let before = find_entry_metadata(id)?;
    let content = service.read_content(id)?;
    let after = find_entry_metadata(id)?;
    anyhow::ensure!(
        before.revision == after.revision,
        "secret changed while opening"
    );

    Ok(OpenedEntry {
        id,
        folder: after.folder,
        title: after.title,
        revision: after.revision,
        recipients: after.recipients,
        icon: after.icon,
        related_entry: after.related_entry,
        content,
    })
}

#[derive(Debug)]
struct NoUnlockKey;

impl std::fmt::Display for NoUnlockKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("no local secret key protects this vault")
    }
}

impl std::error::Error for NoUnlockKey {}

/// Proves access with a fresh in-memory challenge for the vault's local secret keys.
///
/// No entry is read, so a damaged credential or one encrypted only for an
/// absent key cannot block the vault. When the primary key is usable here the
/// challenge goes to it alone, so another cached key cannot skip a forgotten
/// passphrase. Without one, an empty vault has nothing to guard.
fn verify_vault_access(primary: Option<&KeyFingerprint>) -> anyhow::Result<()> {
    let crypto = GnuPgOpenPgp;
    let local = crypto.usable_secret_fingerprints()?;
    let owned: Vec<KeyFingerprint> = match primary.filter(|key| local.contains(key)) {
        Some(primary) => vec![primary.clone()],
        None => {
            let metadata = open_service()?.list_secret_metadata()?;
            if metadata.is_empty() {
                return Ok(());
            }
            metadata
                .iter()
                .flat_map(|entry| entry.recipients.as_slice().iter())
                .filter(|fingerprint| local.contains(fingerprint))
                .cloned()
                .collect()
        }
    };
    let recipients = RecipientSet::new(owned).map_err(|_| NoUnlockKey)?;
    let mut challenge = Vec::with_capacity(32);
    challenge.extend_from_slice(Uuid::new_v4().as_bytes());
    challenge.extend_from_slice(Uuid::new_v4().as_bytes());
    let ciphertext = crypto.encrypt(&challenge, &recipients)?;
    let plaintext = crypto.decrypt(&ciphertext)?;
    anyhow::ensure!(
        plaintext.as_slice() == challenge.as_slice(),
        "unlock challenge mismatch"
    );
    Ok(())
}

fn load_detail_content(id: SecretId) -> anyhow::Result<(SecretContent, u64)> {
    let service = open_service()?;
    let before = find_entry_metadata(id)?;
    let content = service.read_content(id)?;
    let after = find_entry_metadata(id)?;
    anyhow::ensure!(
        before.revision == after.revision,
        "secret changed while opening"
    );
    Ok((content, after.revision))
}

struct EntryMetadata {
    icon: Option<kryptopass_core::EntryIcon>,
    folder: Option<FolderId>,
    title: String,
    revision: u64,
    recipients: Vec<String>,
    related_entry: Option<SecretId>,
}

fn find_entry_metadata(id: SecretId) -> anyhow::Result<EntryMetadata> {
    let storage = open_storage()?;
    let (_, secrets, links) = storage.read_snapshot_with_links()?;
    let secret = secrets
        .into_iter()
        .find(|secret| secret.id == id)
        .ok_or_else(|| anyhow::anyhow!("secret does not exist"))?;
    let metadata = EntryMetadata {
        icon: secret.icon,
        folder: secret.folder_id,
        title: secret.title,
        revision: secret.revision,
        recipients: secret
            .recipients
            .as_slice()
            .iter()
            .map(|key| key.as_str().to_owned())
            .collect(),
        related_entry: links.iter().find_map(|link| {
            if link.first == id {
                Some(link.second)
            } else if link.second == id {
                Some(link.first)
            } else {
                None
            }
        }),
    };
    Ok(metadata)
}

fn step_folder_in_service(
    service: &mut SecretsService<SqliteStorage, GnuPgOpenPgp>,
    id: FolderId,
    direction: i32,
) -> anyhow::Result<()> {
    anyhow::ensure!([-1, 1].contains(&direction), "invalid folder direction");
    let folders = service.list_folders()?;
    let folder = folders
        .iter()
        .find(|folder| folder.id == id)
        .ok_or_else(|| anyhow::anyhow!("folder not found"))?;
    let mut siblings: Vec<_> = folders
        .iter()
        .filter(|candidate| candidate.parent_id == folder.parent_id)
        .collect();
    siblings.sort_by_key(|candidate| {
        (
            candidate.sort_position,
            candidate.name.as_str(),
            candidate.id.0,
        )
    });
    let index = siblings
        .iter()
        .position(|candidate| candidate.id == id)
        .ok_or_else(|| anyhow::anyhow!("folder not found"))?;
    let before = if direction < 0 {
        index.checked_sub(1).map(|previous| siblings[previous].id)
    } else {
        siblings.get(index + 2).map(|next| next.id)
    };
    if (direction < 0 && index > 0) || (direction > 0 && index + 1 < siblings.len()) {
        service.move_folder(id, folder.parent_id, before)?;
    }
    Ok(())
}

fn open_service() -> anyhow::Result<SecretsService<SqliteStorage, GnuPgOpenPgp>> {
    Ok(SecretsService::new(open_storage()?, GnuPgOpenPgp))
}

fn load_store_snapshot() -> anyhow::Result<StoreSnapshot> {
    let snapshot = read_store_snapshot()?;
    let path = app_paths::database_path(crate::app_info::APP.desktop_file_id)?;
    crate::infrastructure::browser::configure(&path)?;
    Ok(snapshot)
}

/// Setup is pending only on a fresh installation; a vault with data predates it.
fn resolve_setup_state(snapshot: &StoreSnapshot) -> anyhow::Result<Option<app_paths::SetupState>> {
    let app_id = crate::app_info::APP.desktop_file_id;
    match app_paths::setup_state(app_id) {
        Ok(Some(state)) => return Ok(Some(state)),
        Ok(None) => {}
        // A damaged preference must not lock the user out of the vault.
        Err(error) => {
            tracing::warn!(%error, "ignoring unreadable setup state");
            return Ok(Some(app_paths::SetupState { version: 1 }));
        }
    }
    if snapshot.entries.is_empty() && snapshot.folders.is_empty() {
        return Ok(None);
    }
    let state = app_paths::SetupState { version: 1 };
    app_paths::save_setup_state(app_id, &state)?;
    Ok(Some(state))
}

fn read_store_snapshot() -> anyhow::Result<StoreSnapshot> {
    let storage = open_storage()?;
    let (folders, secrets, links) = storage.read_snapshot_with_links()?;
    Ok(store_snapshot_with_links(folders, secrets, links))
}

fn activate_store_snapshot(
    path: std::path::PathBuf,
) -> anyhow::Result<(StoreSnapshot, bool, Option<crate::storage::PrimaryKey>)> {
    let storage = SqliteStorage::open_existing(&path)?;
    let (folders, secrets, links) = storage.read_snapshot_with_links()?;
    let primary = storage.primary_key()?;
    let durable = app_paths::set_active_vault_path(crate::app_info::APP.desktop_file_id, &path)?;
    crate::infrastructure::browser::configure(&path)?;
    crate::infrastructure::entry_icons::bind_store(&path);
    let snapshot = store_snapshot_with_links(folders, secrets, links);
    Ok((snapshot, durable, primary))
}

fn open_storage() -> anyhow::Result<SqliteStorage> {
    let app_id = crate::app_info::APP.desktop_file_id;
    if let Some(path) = app_paths::active_vault_path(app_id)? {
        let storage = SqliteStorage::open_existing(&path)?;
        crate::infrastructure::entry_icons::bind_store(&path);
        Ok(storage)
    } else {
        let path = app_paths::database_path(app_id)?;
        let storage = SqliteStorage::open(&path)?;
        crate::infrastructure::entry_icons::bind_store(&path);
        Ok(storage)
    }
}

fn snapshot_after_mutation() -> Result<StoreSnapshot, ()> {
    read_store_snapshot().map_err(|_| ())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_secret_id(value: &str) -> Option<SecretId> {
    Uuid::parse_str(value).ok().map(SecretId)
}

fn parse_folder_id(value: &str) -> Option<FolderId> {
    Uuid::parse_str(value).ok().map(FolderId)
}

fn write_activity_report(report: &str) -> anyhow::Result<std::path::PathBuf> {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kryptopass-actividad-{}.txt", Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(&path)?;
    if let Err(error) = file
        .write_all(report.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error.into());
    }
    Ok(path)
}

fn activity_report(history: &kryptopass_core::LoginHistory) -> anyhow::Result<String> {
    let zone = QTimeZone::system_time_zone();
    let zone = zone
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("local time zone unavailable"))?;
    let mut report = String::from(
        "Actividad en KryptoPass\nEventos conservados, del más reciente al más antiguo.\nNo incluye contraseñas ni valores de credenciales.\n\n",
    );
    for event in history.events() {
        let label = match event.kind {
            LoginHistoryEventKind::Create => "Credencial creada",
            LoginHistoryEventKind::LoginUse => "Credencial utilizada en KryptoPass",
            LoginHistoryEventKind::PasswordRotated => "Contraseña actualizada",
            LoginHistoryEventKind::Edited => "Datos editados",
            LoginHistoryEventKind::CopyUsername => "Usuario copiado",
            LoginHistoryEventKind::CopyPassword => "Contraseña copiada",
            LoginHistoryEventKind::CopyTotp => "Código de verificación copiado",
            LoginHistoryEventKind::CopyNextTotp => "Próximo código de verificación copiado",
        };
        let date = i64::try_from(event.timestamp)
            .ok()
            .map(|timestamp| QDateTime::from_secs_since_epoch(timestamp, zone))
            .filter(QDateTime::is_valid)
            .map(|date| {
                date.format_enum(cxx_qt_lib::DateFormat::ISODate)
                    .to_string()
            })
            .unwrap_or_else(|| format!("Unix {}", event.timestamp));
        report.push_str(&format!("{date}  {label}\n"));
    }
    Ok(report)
}

#[cfg(test)]
#[path = "../../tests/unit/ui_adapters/activity_export.rs"]
mod activity_export_tests;

#[cfg(test)]
#[path = "../../tests/unit/ui_adapters/folder_step.rs"]
mod folder_step_tests;
