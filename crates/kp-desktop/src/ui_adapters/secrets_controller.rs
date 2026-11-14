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

mod browser;
mod clipboard;
mod conflicts;
mod detail;
mod editor;
mod generator;
mod history;
mod lifecycle;
mod organize;

use super::item_issues;
use super::secrets_projection::{
    DetailView, EntryDraft, OpenedEntry, SshKeyMaterial, StoreSnapshot, content_field,
    content_projection, draft_matches_session, earlier_versions, editor_projection, field_text,
    kind_of, new_editor_projection, prepare_save, snapshot_revision, store_snapshot, template_of,
};
use lifecycle::read_primary_key;

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
        #[qproperty(
            QString,
            editor_error_field,
            cxx_name = "editorErrorField",
            READ,
            NOTIFY
        )]
        #[qproperty(QString, load_error, cxx_name = "loadError", READ, NOTIFY)]
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
    /// Template field id the last editor refusal pointed at; empty when it named none.
    editor_error_field: QString,
    /// Why the vault list could not load; empty otherwise. Only the list reads it: a refused
    /// save or a failed move is an error of that action, shown where the action happened.
    load_error: QString,
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
            editor_error_field: QString::default(),
            load_error: QString::default(),
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
                    if let Some(invalid) = error.downcast_ref::<item_issues::InvalidItem>() {
                        controller.as_mut().set_editor_error_field(QString::from(
                            invalid.field.as_deref().unwrap_or_default(),
                        ));
                        controller
                            .as_mut()
                            .fail_message(QString::from(invalid.message.as_str()));
                        return;
                    }
                    let vault_error = error.downcast_ref::<VaultError>();
                    let message = match vault_error {
                        Some(VaultError::Link(SecretLinkError::Occupied)) => {
                            "El elemento elegido ya tiene un vínculo. Elige otro o quita su vínculo primero."
                        }
                        Some(VaultError::Link(SecretLinkError::InvalidTarget)) => {
                            "El elemento vinculado ya no está disponible. Elige otro o selecciona Sin vínculo."
                        }
                        Some(VaultError::Invalid(_)) => {
                            "El elemento no es válido. Revisa sus campos."
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

    fn set_editor_error_field(mut self: Pin<&mut Self>, value: QString) {
        if self.editor_error_field() != &value {
            self.as_mut().rust_mut().editor_error_field = value;
            self.editor_error_field_changed();
        }
    }

    fn set_load_error(mut self: Pin<&mut Self>, value: QString) {
        let value = crate::i18n::translate(&value);
        if self.load_error() != &value {
            self.as_mut().rust_mut().load_error = value;
            self.load_error_changed();
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

    /// Like [`Self::fail`], for a message built at runtime, such as which field was refused.
    fn fail_message(mut self: Pin<&mut Self>, message: QString) {
        self.as_mut().set_operation_error(message.clone());
        self.as_mut().set_status(message);
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
        self.as_mut().set_editor_json(QString::default());
        self.as_mut().set_editor_error_field(QString::default());
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

fn item_meta(session: &VaultSession, id: SecretId) -> anyhow::Result<ItemMeta> {
    session
        .vault()
        .list_items()?
        .into_iter()
        .find(|meta| meta.id == id)
        .ok_or_else(|| anyhow::anyhow!("secret does not exist"))
}

fn snapshot_of(session: &VaultSession) -> anyhow::Result<StoreSnapshot> {
    let vault = session.vault();
    // Unknown keys never make an item look foreign: it then shows as pending.
    let secret_keys = vault
        .secret_keys()
        .inspect_err(|error| tracing::warn!(error = %error, "secret keys unavailable for the list"))
        .ok();
    Ok(store_snapshot(
        vault.list_folders()?,
        vault.list_items()?,
        vault.list_links()?,
        vault.opaque_items()?,
        secret_keys.as_ref(),
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
