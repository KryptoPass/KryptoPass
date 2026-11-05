use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt::{CxxQtType, Threading, casting::Upcast};
use cxx_qt_lib::{QDateTime, QObjectMutPtr, QString, QStringList, QTimeZone, QUrl};
use kryptopass_app::{VaultSession, qr_payload};
use kryptopass_core::vault::{
    Conflict, FolderContents, FolderEdit, IconChoice, Item, ItemEdit, ItemMeta, KeySettings,
    NewFolder, NewItem, Recipients, VaultError, VersionId,
};
use kryptopass_core::{
    DiffKind, DiffTarget, EntryIcon, EntryIconAsset, FieldValue, FolderAppearance, FolderId,
    ItemContent, KeyFingerprint, RecipientSet, SecretId, SecretLinkError, Side, generate_totp,
    inspect_ssh_key, parse_totp_input,
};
use kryptopass_secure_memory::{SensitiveBytes, SensitiveText};
use uuid::Uuid;

use crate::infrastructure::GnuPgOpenPgp;
use crate::storage::paths as app_paths;
use crate::vault_access::{open_existing_session, open_session};

use super::secrets_projection::{
    DetailView, EntryDraft, OpenedEntry, SshKeyMaterial, StoreSnapshot, content_field,
    content_projection, draft_matches_session, earlier_versions, editor_projection, field_text,
    kind_of, new_editor_projection, prepare_save, snapshot_revision, store_snapshot, template_of,
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
        #[qproperty(QString, conflict_json, cxx_name = "conflictJson", READ, NOTIFY)]
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
        #[cxx_name = "openConflict"]
        fn open_conflict(self: Pin<&mut Self>, id: &QString);
        #[qinvokable]
        #[cxx_name = "closeConflict"]
        fn close_conflict(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "toggleConflictValue"]
        fn toggle_conflict_value(self: Pin<&mut Self>, key: &QString);
        #[qinvokable]
        #[cxx_name = "resolveConflict"]
        fn resolve_conflict(self: Pin<&mut Self>, choices_json: &QString);
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
    staged_icon: Option<EntryIconAsset>,
    saving: bool,
    locked: bool,
    unlocking: bool,
    setup_required: bool,
    primary_key: QString,
    forget_on_lock: bool,
    primary: Option<PrimaryKey>,
    /// Invalidates an in-flight unlock check when the vault is locked again.
    unlock_operation: u64,
    session_blocked: bool,
    selected_entry: Option<SecretId>,
    selected_revision: Option<u64>,
    detail_content: Option<ItemContent>,
    /// Earlier versions of the selected item, newest first.
    detail_versions: Vec<(VersionId, u64)>,
    /// The earlier version open in the history view.
    history_item: Option<Item>,
    conflict_json: QString,
    /// The conflict open for merging; its dialog reads it through `conflict_json`.
    conflict: Option<Conflict>,
    conflict_revealed: std::collections::BTreeSet<String>,
    password_revealed: bool,
    revealed_fields: std::collections::BTreeSet<String>,
    qr_source: Option<SensitiveText>,
    /// The content the editor opened; saving keeps what the form does not show.
    editor_content: Option<ItemContent>,
    staged_ssh: Option<(String, SshKeyMaterial)>,
    prepare_operation: u64,
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
            detail_versions: Vec::new(),
            history_item: None,
            conflict_json: QString::default(),
            conflict: None,
            conflict_revealed: std::collections::BTreeSet::new(),
            password_revealed: false,
            revealed_fields: std::collections::BTreeSet::new(),
            qr_source: None,
            editor_content: None,
            staged_ssh: None,
            prepare_operation: 0,
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

impl Drop for SecretsControllerRust {
    fn drop(&mut self) {
        crate::search::set_locked(true);
    }
}

/// The key new items go to, and whether locking forgets its passphrase.
#[derive(Clone)]
struct PrimaryKey {
    fingerprint: KeyFingerprint,
    forget_on_lock: bool,
}

#[derive(Clone, Copy)]
struct EditorSession {
    id: Option<SecretId>,
    revision: Option<u64>,
}

struct MutationResult {
    snapshot: Result<StoreSnapshot, ()>,
    signal: MutationSignal,
    editor_operation: Option<u64>,
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
            super::native_ffi::ffi::install_vault_bus(receiver);
        }
    }
}

impl qobject::SecretsController {
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
                let primary = read_primary_key(&open_session()?)?;
                let setup = resolve_setup_state(&snapshot, primary.is_some())?;
                Ok((snapshot, setup, primary))
            },
            move |mut controller,
                  result: anyhow::Result<(
                StoreSnapshot,
                Option<app_paths::SetupState>,
                Option<PrimaryKey>,
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
                    Err(error) => {
                        tracing::warn!(error = %format!("{error:#}"), "the vault did not load");
                        controller.as_mut().fail("No se pudo cargar la bóveda.");
                    }
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
        self.as_mut().cancel_unlock();
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
        let state = app_paths::SetupState {
            version: SETUP_VERSION,
        };
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
        if write_primary_key(&key).is_err() {
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
        let key = PrimaryKey {
            fingerprint,
            forget_on_lock,
        };
        if write_primary_key(&key).is_err() {
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

    fn apply_primary_key(mut self: Pin<&mut Self>, key: Option<PrimaryKey>) {
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
            move || -> anyhow::Result<Option<LoadedDetail>> {
                let detail = load_detail(secret_id)?;
                if cancel_epoch.load(Ordering::Acquire) == operation {
                    Ok(Some(detail))
                } else {
                    drop(detail);
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
                    Ok(Some(loaded)) => {
                        let detail = content_projection(
                            secret_id,
                            &loaded.content,
                            &DetailView {
                                reveal_password: false,
                                unix_seconds: unix_seconds(),
                                revealed_fields: &std::collections::BTreeSet::new(),
                                qr_source: None,
                                versions: &loaded.versions,
                            },
                        );
                        match detail {
                            Ok(detail) => {
                                controller.as_mut().clear_detail_error();
                                {
                                    let mut state = controller.as_mut().rust_mut();
                                    state.detail_content = Some(loaded.content);
                                    state.detail_versions = loaded.versions;
                                    state.selected_revision = Some(loaded.revision);
                                }
                                controller
                                    .as_mut()
                                    .set_detail_json(QString::from(detail.as_str()));
                                controller
                                    .as_mut()
                                    .set_status(QString::from("Elemento cargado"));
                            }
                            Err(error) => {
                                tracing::warn!(error = %format!("{error:#}"), "item detail projection failed");
                                controller
                                    .as_mut()
                                    .fail_detail("No se pudo mostrar el contenido del elemento.");
                            }
                        }
                    }
                    Ok(None) => (),
                    Err(error) => {
                        tracing::warn!(error = %format!("{error:#}"), "item did not open");
                        let message = if is_unreadable(&error) {
                            "Este elemento está cifrado para una clave que no está en este dispositivo."
                        } else {
                            "No se pudo abrir el contenido del elemento."
                        };
                        controller.as_mut().fail_detail(message);
                    }
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
                                controller.as_mut().rust_mut().editor_content =
                                    Some(entry.content.clone());
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
            let staged = state
                .staged_ssh
                .as_ref()
                .filter(|(staged_token, _)| staged_token == token)
                .map(|(_, key)| key.clone());
            let has_saved_key = state.editor_content.is_some();
            if token.is_empty() && !has_saved_key {
                self.as_mut()
                    .fail("Importa una clave SSH antes de guardar el elemento.");
                return;
            }
            match staged {
                Some(key) => draft.attach_ssh(key),
                None if token.is_empty() => {}
                None => {
                    self.as_mut()
                        .fail("Importa una clave SSH antes de guardar el elemento.");
                    return;
                }
            }
        }
        let base = self.as_ref().rust().editor_content.clone();
        let prepared = match prepare_save(&mut draft, session.id, session.revision, base) {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::debug!(error = %format!("{error:#}"), "draft refused before saving");
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
                let mut session = open_session()?;
                let vault = session.vault_mut();
                let icon = prepared.icon_update.and_then(|icon| match icon {
                    Some(EntryIcon::Theme { name }) => Some(IconChoice::Theme(name)),
                    Some(EntryIcon::Image { asset_id }) => staged_icon
                        .filter(|asset| asset.id == asset_id)
                        .map(IconChoice::Image),
                    Some(EntryIcon::Favicon { .. }) | None => Some(IconChoice::Automatic),
                });
                let id = match (prepared.id, prepared.expected_revision) {
                    (Some(id), Some(revision)) => {
                        let mut edit = ItemEdit::new(id, revision)
                            .title(prepared.title)
                            .folder(prepared.folder)
                            .content(prepared.content);
                        if let Some(icon) = icon {
                            edit = edit.icon(icon);
                        }
                        if let Some(link) = prepared.link_update {
                            edit = edit.link(link);
                        }
                        let meta = vault.update_item(edit)?;
                        if meta.state.recipients != prepared.recipients {
                            vault.change_recipients(id, meta.revision, prepared.recipients)?;
                        }
                        id
                    }
                    _ => {
                        vault
                            .create_item(NewItem {
                                folder: prepared.folder,
                                title: prepared.title,
                                content: prepared.content,
                                recipients: Recipients::Explicit(prepared.recipients),
                                icon: icon.unwrap_or(IconChoice::Automatic),
                                link: prepared.link_update.flatten(),
                            })?
                            .id
                    }
                };
                drop(session);
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
                        let icon = EntryIcon::Image { asset_id: asset.id };
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
                    content_projection(
                        id,
                        content,
                        &DetailView {
                            reveal_password: state.password_revealed,
                            unix_seconds: unix_seconds(),
                            revealed_fields: &state.revealed_fields,
                            qr_source: state.qr_source.as_ref().map(|value| value.as_str()),
                            versions: &state.detail_versions,
                        },
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
            .map(|(value, _)| value);
        if let Some(value) = value {
            self.as_mut().request_copy(value);
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
            let content = state
                .detail_content
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("no item is open"))?;
            let payload = if kind_of(content.template().as_str()) == "totp" {
                super::secrets_projection::totp_uri(content)?
            } else {
                let (_templates, template) = template_of(content.template().as_str())?;
                qr_payload(template, content)?
                    .ok_or_else(|| anyhow::anyhow!("QR unavailable for this item"))?
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
                let private_key = SensitiveText::from_utf8(raw)?;
                anyhow::ensure!(
                    private_key.len() <= 256 * 1024,
                    "SSH key exceeds size limit"
                );
                let info = inspect_ssh_key(&private_key)
                    .ok_or_else(|| anyhow::anyhow!("not an OpenSSH private key"))?;
                Ok((
                    info,
                    SshKeyMaterial {
                        private_key,
                        passphrase,
                    },
                ))
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
                Ok((info, key)) => {
                    let token = Uuid::new_v4().to_string();
                    let json = serde_json::json!({"importToken": token, "publicKey": info.public_key,
                        "fingerprint": info.fingerprint, "algorithm": info.algorithm}).to_string();
                    controller.as_mut().rust_mut().staged_ssh = Some((token, key));
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
        {
            let mut state = self.as_mut().rust_mut();
            let is_login = state
                .detail_content
                .as_ref()
                .is_some_and(|content| kind_of(content.template().as_str()) == "login");
            if state.selected_entry.is_none() || !is_login {
                return;
            }
            state.password_revealed = !state.password_revealed;
        }
        self.publish_detail();
    }

    pub fn copy_username(mut self: Pin<&mut Self>) {
        self.as_mut().copy_login_field(&["username", "email"]);
    }

    pub fn copy_password(mut self: Pin<&mut Self>) {
        self.as_mut().copy_login_field(&["password"]);
    }

    fn copy_login_field(mut self: Pin<&mut Self>, fields: &[&str]) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let value = self
            .as_ref()
            .rust()
            .detail_content
            .as_ref()
            .and_then(|content| {
                fields
                    .iter()
                    .map(|field| field_text(content, field))
                    .find(|value| !value.is_empty())
            });
        if let Some(value) = value {
            self.as_mut().request_copy(value);
        }
    }

    pub fn open_history_version(mut self: Pin<&mut Self>, index: i32) {
        self.as_mut().close_history_version();
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let (id, selection, version) = {
            let object = self.as_ref();
            let state = object.rust();
            let (Some(id), Some(version)) =
                (state.selected_entry, state.detail_versions.get(index))
            else {
                return;
            };
            (id, state.selection_operation, *version)
        };
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<Item> {
                Ok(open_session()?.vault().open_version(id, version.0)?)
            },
            move |mut controller, result| {
                if controller.as_ref().rust().selection_operation != selection
                    || *controller.locked()
                {
                    return;
                }
                let item = match result {
                    Ok(item) => item,
                    Err(error) => {
                        tracing::warn!(error = %format!("{error:#}"), "item version did not open");
                        controller
                            .as_mut()
                            .fail_detail("No se pudo abrir esa versión del elemento.");
                        return;
                    }
                };
                #[derive(serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                struct VersionProjection<'a> {
                    at: u64,
                    title: &'a str,
                    username: SensitiveText,
                    urls: &'a [SensitiveText],
                    notes: SensitiveText,
                    has_password: bool,
                    has_totp: bool,
                }
                let projection = super::secrets_projection::sensitive_json(&VersionProjection {
                    at: version.1,
                    title: &item.meta.state.title,
                    username: field_text(&item.content, "username"),
                    urls: item.content.value("urls").map_or(&[], FieldValue::texts),
                    notes: field_text(&item.content, "notes"),
                    has_password: !field_text(&item.content, "password").is_empty(),
                    has_totp: !field_text(&item.content, "totp").is_empty(),
                });
                if let Ok(projection) = projection {
                    controller.as_mut().rust_mut().history_version_index = Some(index);
                    controller.as_mut().rust_mut().history_item = Some(item);
                    controller.set_version_json(QString::from(projection.as_str()));
                }
            },
        );
    }

    pub fn close_history_version(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().history_version_index = None;
        self.as_mut().rust_mut().history_item = None;
        self.set_version_json(QString::default());
    }

    pub fn copy_history_value(self: Pin<&mut Self>, field: &QString) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(item) = state.history_item.as_ref() else {
                return;
            };
            match field.to_string().as_str() {
                "title" => SensitiveText::from(item.meta.state.title.clone()),
                "urls" => {
                    let mut text = SensitiveText::new();
                    let urls = item
                        .content
                        .value("urls")
                        .map_or(&[][..], FieldValue::texts);
                    for (index, url) in urls.iter().enumerate() {
                        if index != 0 {
                            text.push('\n');
                        }
                        text.push_str(url);
                    }
                    text
                }
                name @ ("password" | "username" | "notes" | "totp") => {
                    field_text(&item.content, name)
                }
                _ => return,
            }
        };
        if value.is_empty() {
            return;
        }
        self.clipboard_requested(&QString::from(value.as_str()));
    }

    pub fn export_activity(self: Pin<&mut Self>) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let report = activity_report(&self.as_ref().rust().detail_versions);
        match report.and_then(|report| write_activity_report(&report)) {
            Ok(path) => self.activity_exported(&QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()))),
            Err(_) => self.fail("No se pudo generar el archivo de actividad. Comprueba el espacio disponible e inténtalo de nuevo."),
        }
    }

    pub fn refresh_totp(self: Pin<&mut Self>) {
        if *self.locked() || *self.busy() {
            return;
        }
        let has_code = self
            .as_ref()
            .rust()
            .detail_content
            .as_ref()
            .is_some_and(|content| {
                matches!(kind_of(content.template().as_str()), "login" | "totp")
            });
        if has_code {
            self.publish_detail();
        }
    }

    pub fn copy_totp(mut self: Pin<&mut Self>, next: bool) {
        if !self.as_ref().can_read_detail() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(content) = state.detail_content.as_ref() else {
                return;
            };
            let field = if kind_of(content.template().as_str()) == "login" {
                "totp"
            } else {
                "uri"
            };
            let Ok(Some(config)) = parse_totp_input(&field_text(content, field)) else {
                return;
            };
            let Ok(mut snapshot) = generate_totp(&config, unix_seconds()) else {
                return;
            };
            if next {
                std::mem::take(&mut snapshot.next)
            } else {
                std::mem::take(&mut snapshot.current)
            }
        };
        self.as_mut().request_copy(value);
    }

    /// Usage events are no longer recorded, so a finished copy changes nothing.
    pub fn confirm_copy(self: Pin<&mut Self>, _copied: bool) {}

    fn can_copy_login_value(self: Pin<&Self>) -> bool {
        let state = self.rust();
        !*self.locked()
            && !*self.busy()
            && !*self.saving()
            && state.editor_session.is_none()
            && state.selected_revision.is_some()
            && state
                .detail_content
                .as_ref()
                .is_some_and(|content| kind_of(content.template().as_str()) == "login")
    }

    fn request_copy(mut self: Pin<&mut Self>, value: SensitiveText) {
        {
            let state = self.as_ref();
            let state = state.rust();
            if state.selected_entry.is_none()
                || state.selected_revision.is_none()
                || state.editor_session.is_some()
            {
                return;
            }
        }
        let value = QString::from(value.as_str());
        self.as_mut().clipboard_requested(&value);
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
                let mut session = open_session()?;
                let vault = session.vault_mut();
                let saved_id = match folder_id {
                    Some(id) => {
                        vault
                            .update_folder(FolderEdit::new(id).name(name).appearance(appearance))?;
                        id
                    }
                    None => {
                        vault
                            .create_folder(NewFolder {
                                name,
                                parent: parent_id,
                                appearance,
                                default_recipients: None,
                            })?
                            .id
                    }
                };
                drop(session);
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
                step_folder(&mut open_session()?, id, direction)?;
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
                open_session()?.vault_mut().move_folder(id, parent, before)?;
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
                let mut session = open_session()?;
                let before_revision = item_meta(&session, id)?.revision;
                session
                    .vault_mut()
                    .update_item(ItemEdit::new(id, before_revision).folder(folder_id))?;
                drop(session);
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
                let mut session = open_session()?;
                let revision = item_meta(&session, id)?.revision;
                session.vault_mut().remove_item(id, revision)?;
                drop(session);
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
                let mut session = open_session()?;
                let affected_selection =
                    selected_id
                        .filter(|_| !delete_contents)
                        .and_then(|selected| {
                            item_meta(&session, selected)
                                .ok()
                                .filter(|entry| entry.state.folder == Some(id))
                                .map(|entry| (selected, entry.revision))
                        });
                session.vault_mut().remove_folder(
                    id,
                    if delete_contents {
                        FolderContents::Delete
                    } else {
                        FolderContents::Reparent
                    },
                )?;
                drop(session);
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

    /// Opens the merge of a conflicted item. Decrypting may ask for a passphrase.
    pub fn open_conflict(mut self: Pin<&mut Self>, id: &QString) {
        if *self.locked() || *self.busy() || *self.saving() {
            return;
        }
        let Some(id) = parse_secret_id(&id.to_string()) else {
            self.as_mut().fail("El elemento seleccionado no es válido.");
            return;
        };
        self.as_mut().close_conflict();
        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Abriendo las dos versiones…"));
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<Option<Conflict>> {
                Ok(open_session()?.vault().open_conflict(id)?)
            },
            |mut controller, result| {
                controller.as_mut().set_busy(false);
                match result {
                    Ok(Some(conflict)) => {
                        controller.as_mut().rust_mut().conflict = Some(conflict);
                        controller.as_mut().publish_conflict();
                        controller
                            .as_mut()
                            .set_status(QString::from("Revisa las diferencias"));
                    }
                    Ok(None) => controller
                        .as_mut()
                        .fail("Este elemento ya no tiene conflictos."),
                    Err(error) => {
                        tracing::warn!(error = %format!("{error:#}"), "conflict did not open");
                        let message = match error.downcast_ref::<VaultError>() {
                            Some(VaultError::ConflictUnreadable) => {
                                "Una de las versiones usa una clave que no está en este dispositivo. Resuélvelo desde un dispositivo que la tenga."
                            }
                            _ => "No se pudieron abrir las versiones en conflicto.",
                        };
                        controller.as_mut().fail(message);
                    }
                }
            },
        );
    }

    pub fn close_conflict(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.conflict = None;
            state.conflict_revealed.clear();
        }
        self.set_conflict_json(QString::default());
    }

    /// Shows or hides the two values of a concealed difference.
    pub fn toggle_conflict_value(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        {
            let mut state = self.as_mut().rust_mut();
            if !state.conflict_revealed.remove(&key) {
                state.conflict_revealed.insert(key);
            }
        }
        self.publish_conflict();
    }

    /// Saves the merge with one side per difference, as `{"key": "ours"|"theirs"}`.
    pub fn resolve_conflict(mut self: Pin<&mut Self>, choices_json: &QString) {
        if *self.busy() || self.as_ref().rust().conflict.is_none() {
            return;
        }
        let Ok(picked) = serde_json::from_str::<std::collections::HashMap<String, String>>(
            &choices_json.to_string(),
        ) else {
            self.as_mut()
                .fail("La selección de la combinación no es válida.");
            return;
        };
        let choices = {
            let object = self.as_ref();
            let Some(conflict) = object.rust().conflict.as_ref() else {
                return;
            };
            conflict
                .differences
                .iter()
                .filter_map(|difference| {
                    let side = match picked.get(&conflict_key(&difference.target))?.as_str() {
                        "ours" => Side::Ours,
                        "theirs" => Side::Theirs,
                        _ => return None,
                    };
                    Some((difference.target.clone(), side))
                })
                .collect::<std::collections::HashMap<_, _>>()
        };
        let Some(conflict) = self.as_mut().rust_mut().conflict.take() else {
            return;
        };
        let operation = self.as_ref().rust().editor_operation;
        self.as_mut().set_conflict_json(QString::default());
        self.as_mut().start_mutation(
            "Guardando la combinación…",
            true,
            "No se pudo guardar la combinación. El conflicto sigue abierto.",
            move || {
                let meta = open_session()?
                    .vault_mut()
                    .resolve_conflict(&conflict, &choices)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::EntrySaved(meta.id.0.to_string()),
                    editor_operation: Some(operation),
                })
            },
        );
    }

    fn publish_conflict(mut self: Pin<&mut Self>) {
        let projection = {
            let object = self.as_ref();
            let state = object.rust();
            state
                .conflict
                .as_ref()
                .map(|conflict| conflict_projection(conflict, &state.conflict_revealed))
        };
        match projection {
            Some(Ok(projection)) => self
                .as_mut()
                .set_conflict_json(QString::from(projection.as_str())),
            Some(Err(error)) => {
                tracing::warn!(error = %format!("{error:#}"), "conflict projection failed");
                self.as_mut().rust_mut().conflict = None;
                self.as_mut()
                    .fail("No se pudieron mostrar las versiones en conflicto.");
            }
            None => {}
        }
    }

    fn set_conflict_json(mut self: Pin<&mut Self>, value: QString) {
        if self.conflict_json() != &value {
            self.as_mut().rust_mut().conflict_json = value;
            self.conflict_json_changed();
        }
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
        match generate_password_value(
            length,
            lowercase,
            uppercase,
            digits,
            symbols,
            &excluded.to_string(),
        ) {
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
                let mut session = open_session()?;
                let before_revision = item_meta(&session, id)?.revision;
                let mut edit = ItemEdit::new(id, before_revision);
                if let Some(favorite) = favorite {
                    edit = edit.favorite(favorite);
                }
                if let Some(trashed) = trashed {
                    edit = edit.trashed(trashed);
                }
                session.vault_mut().update_item(edit)?;
                drop(session);
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
                    tracing::warn!(error = %format!("{error:#}"), "vault change failed");
                    let vault_error = error.downcast_ref::<VaultError>();
                    let message = match vault_error {
                        Some(VaultError::Link(SecretLinkError::Occupied)) => {
                            "El elemento elegido ya tiene un vínculo. Elige otro o quita su vínculo primero."
                        }
                        Some(VaultError::Link(SecretLinkError::InvalidTarget)) => {
                            "El elemento vinculado ya no está disponible. Elige otro o selecciona Sin vínculo."
                        }
                        Some(VaultError::Invalid(_)) => {
                            "Revisa los campos: falta alguno obligatorio o tiene un formato no válido. Las fechas van como AAAA-MM-DD."
                        }
                        Some(VaultError::StaleRevision) => {
                            "Este elemento cambió mientras lo editabas. Ábrelo de nuevo para no perder esos cambios."
                        }
                        _ => failure_message,
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
        state.detail_versions.clear();
        state.selected_entry = None;
        state.selected_revision = None;
        state.password_revealed = false;
        state.revealed_fields.clear();
        state.qr_source = None;
        self.as_mut().set_detail_json(QString::default());
    }

    fn clear_sensitive_state_contents(mut self: Pin<&mut Self>) {
        crate::search::set_locked(true);
        self.as_mut().close_conflict();
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
            state.editor_content = None;
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
            crate::search::set_locked(value);
            kryptopass_browser::set_locked(value, false);
            self.as_mut().rust_mut().locked = value;
            self.as_mut().locked_changed();
            self.as_mut().refresh_browser_pairing();
        }
    }
}

fn load_editor_entry(id: SecretId) -> anyhow::Result<OpenedEntry> {
    let session = open_session()?;
    let item = session.vault().open_item(id)?;
    let related_entry = session.vault().list_links()?.into_iter().find_map(|link| {
        if link.first == id {
            Some(link.second)
        } else if link.second == id {
            Some(link.first)
        } else {
            None
        }
    });
    let Item { meta, content, .. } = item;
    Ok(OpenedEntry {
        id,
        folder: meta.state.folder,
        title: meta.state.title,
        revision: meta.revision,
        recipients: meta
            .state
            .recipients
            .as_slice()
            .iter()
            .map(|key| key.as_str().to_owned())
            .collect(),
        icon: meta.state.icon,
        related_entry,
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
            let metadata = open_session()?.vault().list_items()?;
            if metadata.is_empty() {
                return Ok(());
            }
            metadata
                .iter()
                .flat_map(|entry| entry.state.recipients.as_slice().iter())
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
    // What arrived through Sync while locked becomes listable with the key open.
    let indexed = crate::vault_access::open_background()
        .and_then(|mut vault| Ok(kryptopass_app::index_received(&mut vault)?));
    if let Err(error) = indexed {
        tracing::warn!(error = %format!("{error:#}"), "versions were not indexed after unlocking");
    }
    anyhow::ensure!(
        plaintext.as_slice() == challenge.as_slice(),
        "unlock challenge mismatch"
    );
    Ok(())
}

struct LoadedDetail {
    content: ItemContent,
    revision: u64,
    versions: Vec<(VersionId, u64)>,
}

fn load_detail(id: SecretId) -> anyhow::Result<LoadedDetail> {
    let session = open_session()?;
    let Item { meta, content, .. } = session.vault().open_item(id)?;
    let versions = earlier_versions(meta.head, session.vault().item_history(id)?);
    Ok(LoadedDetail {
        content,
        revision: meta.revision,
        versions,
    })
}

/// Whether opening failed only because no key on this device can read the item.
fn is_unreadable(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<VaultError>(),
        Some(
            VaultError::Cipher(kryptopass_core::vault::CipherError::NotARecipient)
                | VaultError::ItemNotFound
        )
    )
}

/// The stable name of a difference, for the dialog to answer with.
fn conflict_key(target: &DiffTarget) -> String {
    match target {
        DiffTarget::Title => "title".to_owned(),
        DiffTarget::Folder => "folder".to_owned(),
        DiffTarget::Favorite => "favorite".to_owned(),
        DiffTarget::Trashed => "trashed".to_owned(),
        DiffTarget::Icon => "icon".to_owned(),
        DiffTarget::Field(field) => format!("field:{}", field.as_str()),
        DiffTarget::Custom(id) => format!("custom:{id}"),
    }
}

/// What the merge dialog shows. Concealed values stay out until revealed, and
/// values come from the versions the conflict already holds.
fn conflict_projection(
    conflict: &Conflict,
    revealed: &std::collections::BTreeSet<String>,
) -> anyhow::Result<SensitiveText> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        key: String,
        label: SensitiveText,
        needs_review: bool,
        concealed: bool,
        revealed: bool,
        suggested: &'static str,
        ours: SensitiveText,
        theirs: SensitiveText,
    }
    #[derive(serde::Serialize)]
    struct Projection {
        id: String,
        title: String,
        rows: Vec<Row>,
    }
    let (templates, template) = template_of(conflict.ours.meta.state.template.as_str())?;
    let form = kryptopass_app::Form::build(
        templates.library(),
        template,
        &crate::vault_access::language(),
    );
    let yes_no = |value: bool| crate::i18n::text(if value { "Sí" } else { "No" });
    let value_of = |item: &Item, target: &DiffTarget| -> SensitiveText {
        let state = &item.meta.state;
        match target {
            DiffTarget::Title => state.title.clone().into(),
            DiffTarget::Folder => state
                .folder
                .map_or_else(String::new, |folder| folder.0.to_string())
                .into(),
            DiffTarget::Favorite => yes_no(state.favorite).into(),
            DiffTarget::Trashed => yes_no(state.trashed).into(),
            DiffTarget::Icon => crate::i18n::text("Icono elegido").into(),
            DiffTarget::Field(field) => {
                let mut text = SensitiveText::new();
                let values = item
                    .content
                    .value(field.as_str())
                    .map_or(&[][..], FieldValue::texts);
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        text.push('\n');
                    }
                    text.push_str(value);
                }
                text
            }
            DiffTarget::Custom(id) => item
                .content
                .custom()
                .iter()
                .find(|custom| custom.id() == *id)
                .map(|custom| custom.value.clone())
                .unwrap_or_default(),
        }
    };
    let label_of = |target: &DiffTarget| -> SensitiveText {
        match target {
            DiffTarget::Title => crate::i18n::text("Nombre").into(),
            DiffTarget::Folder => crate::i18n::text("Carpeta").into(),
            DiffTarget::Favorite => crate::i18n::text("Favorito").into(),
            DiffTarget::Trashed => crate::i18n::text("En la papelera").into(),
            DiffTarget::Icon => crate::i18n::text("Icono").into(),
            DiffTarget::Field(field) => form
                .sections
                .iter()
                .flat_map(|section| &section.fields)
                .find(|candidate| candidate.id == *field)
                .map_or_else(
                    || field.as_str().to_owned(),
                    |candidate| candidate.label.clone(),
                )
                .into(),
            DiffTarget::Custom(id) => [&conflict.ours, &conflict.theirs]
                .iter()
                .find_map(|item| {
                    item.content
                        .custom()
                        .iter()
                        .find(|custom| custom.id() == *id)
                        .map(|custom| custom.key.clone())
                })
                .unwrap_or_default(),
        }
    };
    let rows = conflict
        .differences
        .iter()
        .map(|difference| {
            let key = conflict_key(&difference.target);
            let show = !difference.concealed || revealed.contains(&key);
            let value = |item: &Item| {
                if show {
                    value_of(item, &difference.target)
                } else {
                    SensitiveText::new()
                }
            };
            Row {
                label: label_of(&difference.target),
                needs_review: difference.kind == DiffKind::Conflict,
                concealed: difference.concealed,
                revealed: difference.concealed && show,
                suggested: match difference.suggested {
                    Some(Side::Ours) => "ours",
                    Some(Side::Theirs) => "theirs",
                    _ => "",
                },
                ours: value(&conflict.ours),
                theirs: value(&conflict.theirs),
                key,
            }
        })
        .collect();
    super::secrets_projection::sensitive_json(&Projection {
        id: conflict.ours.meta.id.0.to_string(),
        title: conflict.ours.meta.state.title.clone(),
        rows,
    })
}

fn item_meta(session: &VaultSession, id: SecretId) -> anyhow::Result<ItemMeta> {
    session
        .vault()
        .list_items()?
        .into_iter()
        .find(|meta| meta.id == id)
        .ok_or_else(|| anyhow::anyhow!("secret does not exist"))
}

fn step_folder(session: &mut VaultSession, id: FolderId, direction: i32) -> anyhow::Result<()> {
    anyhow::ensure!([-1, 1].contains(&direction), "invalid folder direction");
    let folders = session.vault().list_folders()?;
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
        session
            .vault_mut()
            .move_folder(id, folder.parent_id, before)?;
    }
    Ok(())
}

/// The vault's primary key: its single default recipient, with its settings.
fn read_primary_key(session: &VaultSession) -> anyhow::Result<Option<PrimaryKey>> {
    let Some(recipients) = session.vault().default_recipients()? else {
        return Ok(None);
    };
    let Some(fingerprint) = recipients.as_slice().first().cloned() else {
        return Ok(None);
    };
    let forget_on_lock = session.vault().settings_of(&fingerprint)?.forget_on_lock;
    Ok(Some(PrimaryKey {
        fingerprint,
        forget_on_lock,
    }))
}

fn write_primary_key(key: &PrimaryKey) -> anyhow::Result<()> {
    let mut session = open_session()?;
    let vault = session.vault_mut();
    vault.set_default_recipients(Some(RecipientSet::new(vec![key.fingerprint.clone()])?))?;
    let mut settings = KeySettings::new(key.fingerprint.clone());
    settings.forget_on_lock = key.forget_on_lock;
    vault.set_key_settings(settings)?;
    Ok(())
}

fn generate_password_value(
    length: usize,
    lowercase: bool,
    uppercase: bool,
    digits: bool,
    symbols: bool,
    excluded: &str,
) -> anyhow::Result<SensitiveText> {
    use kp_template::CharClass;
    use kryptopass_generator::{Settings, generate};
    let classes: Vec<CharClass> = [
        (lowercase, CharClass::Lower),
        (uppercase, CharClass::Upper),
        (digits, CharClass::Digits),
        (symbols, CharClass::Symbols),
    ]
    .into_iter()
    .filter_map(|(enabled, class)| enabled.then_some(class))
    .collect();
    let mut settings = Settings::new(length, &classes)?;
    settings.set_excluded(excluded)?;
    Ok(generate(&settings)?)
}

fn load_store_snapshot() -> anyhow::Result<StoreSnapshot> {
    let snapshot = read_store_snapshot()?;
    crate::infrastructure::browser::configure(&crate::vault_access::vault_path()?)?;
    Ok(snapshot)
}

/// Bumped when the vault format changes: a mark from before it belongs to a
/// vault this version does not open, so the first run must be offered again.
const SETUP_VERSION: u32 = 2;

/// Setup is pending on a fresh installation; a vault with data predates it.
fn resolve_setup_state(
    snapshot: &StoreSnapshot,
    has_primary: bool,
) -> anyhow::Result<Option<app_paths::SetupState>> {
    let app_id = crate::app_info::APP.desktop_file_id;
    match app_paths::setup_state(app_id) {
        Ok(Some(state)) if state.version >= SETUP_VERSION => return Ok(Some(state)),
        Ok(_) => {}
        // A damaged preference must not lock the user out of the vault.
        Err(error) => {
            tracing::warn!(%error, "ignoring unreadable setup state");
            return Ok(Some(app_paths::SetupState {
                version: SETUP_VERSION,
            }));
        }
    }
    if snapshot.entries.is_empty() && snapshot.folders.is_empty() && !has_primary {
        return Ok(None);
    }
    let state = app_paths::SetupState {
        version: SETUP_VERSION,
    };
    app_paths::save_setup_state(app_id, &state)?;
    Ok(Some(state))
}

fn snapshot_of(session: &VaultSession) -> anyhow::Result<StoreSnapshot> {
    let vault = session.vault();
    Ok(store_snapshot(
        vault.list_folders()?,
        vault.list_items()?,
        vault.list_links()?,
        vault.opaque_items()?,
    ))
}

fn read_store_snapshot() -> anyhow::Result<StoreSnapshot> {
    snapshot_of(&open_session()?)
}

fn activate_store_snapshot(
    path: std::path::PathBuf,
) -> anyhow::Result<(StoreSnapshot, bool, Option<PrimaryKey>)> {
    let session = open_existing_session(path.clone())?;
    let snapshot = snapshot_of(&session)?;
    let primary = read_primary_key(&session)?;
    drop(session);
    let durable = app_paths::set_active_vault_path(crate::app_info::APP.desktop_file_id, &path)?;
    crate::infrastructure::browser::configure(&path)?;
    crate::infrastructure::entry_icons::bind_store(&path);
    Ok((snapshot, durable, primary))
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

fn activity_report(versions: &[(VersionId, u64)]) -> anyhow::Result<String> {
    let zone = QTimeZone::system_time_zone();
    let zone = zone
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("local time zone unavailable"))?;
    let mut report = String::from(
        "Actividad en KryptoPass\nVersiones anteriores, de la más reciente a la más antigua.\nNo incluye contraseñas ni valores de credenciales.\n\n",
    );
    for (_, at) in versions {
        let date = i64::try_from(*at)
            .ok()
            .map(|timestamp| QDateTime::from_secs_since_epoch(timestamp, zone))
            .filter(QDateTime::is_valid)
            .map(|date| {
                date.format_enum(cxx_qt_lib::DateFormat::ISODate)
                    .to_string()
            })
            .unwrap_or_else(|| format!("Unix {at}"));
        report.push_str(&format!("{date}  Datos editados\n"));
    }
    Ok(report)
}
