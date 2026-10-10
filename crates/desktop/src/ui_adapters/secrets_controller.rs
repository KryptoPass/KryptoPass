use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList, QUrl};
use kryptopass_core::{
    FolderAppearance, FolderId, PasswordOptions, SecretContent, SecretId, SecretsService, Storage,
    generate_password as generate_secure_password,
};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::infrastructure::{GnuPgOpenPgp, SqliteStorage, app_paths};

use super::secrets_projection::{
    EntryDraft, OpenedEntry, StoreSnapshot, content_projection, draft_matches_session,
    editor_projection, new_editor_projection, prepare_save, store_snapshot,
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
        #[qproperty(QString, editor_json, cxx_name = "editorJson", READ, NOTIFY)]
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(bool, detail_busy, cxx_name = "detailBusy", READ, NOTIFY)]
        #[qproperty(bool, editor_busy, cxx_name = "editorBusy", READ, NOTIFY)]
        #[qproperty(bool, saving, READ, NOTIFY)]
        #[qproperty(bool, locked, READ, NOTIFY)]
        type SecretsController = super::SecretsControllerRust;

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
}

/// State owned by the QObject; decrypted values have zeroizing domain types.
pub struct SecretsControllerRust {
    entry_records: QStringList,
    folder_records: QStringList,
    detail_json: QString,
    editor_json: QString,
    error_message: QString,
    /// Keep an overlapping detail-read failure from hiding a store-operation error.
    operation_error_active: bool,
    status: QString,
    busy: bool,
    detail_busy: bool,
    editor_busy: bool,
    saving: bool,
    locked: bool,
    selected_entry: Option<SecretId>,
    detail_content: Option<SecretContent>,
    password_revealed: bool,
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
            entry_records: QStringList::default(),
            folder_records: QStringList::default(),
            detail_json: QString::default(),
            editor_json: QString::default(),
            error_message: QString::default(),
            operation_error_active: false,
            status: QString::from("Bóveda no cargada"),
            busy: false,
            detail_busy: false,
            editor_busy: false,
            saving: false,
            locked: true,
            selected_entry: None,
            detail_content: None,
            password_revealed: false,
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
}

impl qobject::SecretsController {
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
            load_store_snapshot,
            move |mut controller, result| {
                if controller.as_ref().rust().load_operation != operation {
                    return;
                }

                controller.as_mut().set_busy(false);
                match result {
                    Ok(snapshot) => {
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
                    Ok((snapshot, durable)) => {
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
        self.as_mut().clear_sensitive_state_contents();
        self.as_mut().clear_error();
        self.as_mut().set_locked(true);
        self.as_mut().set_status(QString::from("Bóveda bloqueada"));
        self.as_mut().vault_locked();
    }

    pub fn unlock_vault(mut self: Pin<&mut Self>) {
        if *self.busy() || *self.saving() {
            self.as_mut()
                .fail("Espera a que termine la operación antes de acceder a la bóveda.");
            return;
        }
        self.as_mut().clear_error();
        self.as_mut().set_locked(false);
        self.as_mut().set_status(QString::from(
            "Bóveda disponible. El contenido se abrirá al seleccionarlo.",
        ));
    }

    pub fn clear_sensitive_state(mut self: Pin<&mut Self>) {
        if *self.busy() || *self.saving() {
            return;
        }
        self.as_mut().clear_sensitive_state_contents();
        self.as_mut().set_locked(true);
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
            move || -> anyhow::Result<Option<SecretContent>> {
                let service = open_service()?;
                let content = service.read_content(secret_id)?;
                if cancel_epoch.load(Ordering::Acquire) == operation {
                    Ok(Some(content))
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
                    Ok(Some(content)) => {
                        let detail = content_projection(secret_id, &content, false);
                        match detail {
                            Ok(detail) => {
                                controller.as_mut().rust_mut().detail_content = Some(content);
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
        if *self.saving() || *self.locked() {
            return;
        }
        let id = id.to_string();
        let operation = self.as_mut().begin_editor_operation();
        self.as_mut().clear_editor_projection();
        self.as_mut().set_editor_busy(false);
        self.as_mut().clear_error();

        if id.is_empty() {
            self.as_mut().rust_mut().editor_session = Some(EditorSession {
                id: None,
                revision: None,
            });
            let draft = new_editor_projection();
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
        if *self.editor_busy() {
            return;
        }

        let draft_json = Zeroizing::new(draft_json.to_string());
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

        let prepared = match prepare_save(&mut draft, session.id, session.revision) {
            Ok(prepared) => prepared,
            Err(_) => {
                self.as_mut()
                    .fail("Revisa los datos del elemento y sus destinatarios.");
                return;
            }
        };
        let operation = self.as_ref().rust().editor_operation;
        self.as_mut().start_mutation(
            "Guardando elemento…",
            true,
            "No se pudo guardar el elemento.",
            move || {
                let mut service = open_service()?;
                let id = service.save_content(
                    prepared.id,
                    prepared.folder,
                    prepared.title,
                    prepared.recipients,
                    prepared.content,
                    prepared.expected_revision,
                )?;
                let snapshot = snapshot_after_mutation();
                Ok(MutationResult {
                    snapshot,
                    signal: MutationSignal::EntrySaved(id.0.to_string()),
                    editor_operation: Some(operation),
                })
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
                .map(|content| content_projection(id, content, revealed))
        };
        if let Some(Ok(detail)) = projection {
            self.as_mut()
                .set_detail_json(QString::from(detail.as_str()));
        }
    }

    pub fn copy_username(self: Pin<&mut Self>) {
        if *self.locked() || *self.busy() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(content) = state.detail_content.as_ref() else {
                return;
            };
            match content {
                SecretContent::Login(login) if !login.username.is_empty() => {
                    Zeroizing::new(login.username.clone())
                }
                _ => return,
            }
        };
        let value = QString::from(value.as_str());
        self.clipboard_requested(&value);
    }

    pub fn copy_password(self: Pin<&mut Self>) {
        if *self.locked() || *self.busy() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(content) = state.detail_content.as_ref() else {
                return;
            };
            match content {
                SecretContent::Login(login) if !login.password.is_empty() => {
                    Zeroizing::new(login.password.clone())
                }
                _ => return,
            }
        };
        let value = QString::from(value.as_str());
        self.clipboard_requested(&value);
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
                service.move_secret(id, folder_id)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::None,
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

    pub fn remove_folder(mut self: Pin<&mut Self>, id: &QString) {
        if *self.busy() {
            return;
        }
        let Some(id) = parse_folder_id(&id.to_string()) else {
            self.as_mut().fail("La carpeta seleccionada no es válida.");
            return;
        };

        self.as_mut().start_mutation(
            "Eliminando carpeta…",
            false,
            "No se pudo eliminar la carpeta.",
            move || {
                let mut service = open_service()?;
                service.remove_folder(id)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::None,
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
                service.update_flags(id, favorite, trashed)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::None,
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
                        MutationSignal::None => {}
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
                Err(_) => controller.as_mut().fail(failure_message),
            }
        });
    }

    fn publish_snapshot(mut self: Pin<&mut Self>, snapshot: StoreSnapshot) {
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

    fn set_editor_json(mut self: Pin<&mut Self>, value: QString) {
        if self.editor_json() != &value {
            self.as_mut().rust_mut().editor_json = value;
            self.editor_json_changed();
        }
    }

    fn set_error_message(mut self: Pin<&mut Self>, value: QString) {
        if self.error_message() != &value {
            self.as_mut().rust_mut().error_message = value;
            self.error_message_changed();
        }
    }

    fn set_status(mut self: Pin<&mut Self>, value: QString) {
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

    fn set_operation_error(mut self: Pin<&mut Self>, value: QString) {
        self.as_mut().rust_mut().operation_error_active = true;
        self.set_error_message(value);
    }

    fn clear_detail(mut self: Pin<&mut Self>) {
        let mut state = self.as_mut().rust_mut();
        state.detail_content = None;
        state.selected_entry = None;
        state.password_revealed = false;
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
        self.as_mut().set_editor_json(QString::default());
        self.as_mut().set_editor_busy(false);
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

    fn set_saving(mut self: Pin<&mut Self>, value: bool) {
        if *self.saving() != value {
            self.as_mut().rust_mut().saving = value;
            self.saving_changed();
        }
    }

    fn set_locked(mut self: Pin<&mut Self>, value: bool) {
        if *self.locked() != value {
            self.as_mut().rust_mut().locked = value;
            self.locked_changed();
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
        content,
    })
}

struct EntryMetadata {
    folder: Option<FolderId>,
    title: String,
    revision: u64,
    recipients: Vec<String>,
}

fn find_entry_metadata(id: SecretId) -> anyhow::Result<EntryMetadata> {
    let storage = open_storage()?;
    let mut secret = storage
        .find_secret(id)?
        .ok_or_else(|| anyhow::anyhow!("secret does not exist"))?;
    let metadata = EntryMetadata {
        folder: secret.folder_id,
        title: std::mem::take(&mut secret.title),
        revision: secret.revision,
        recipients: secret
            .recipients
            .as_slice()
            .iter()
            .map(|key| key.as_str().to_owned())
            .collect(),
    };
    secret.ciphertext.zeroize();
    Ok(metadata)
}

fn open_service() -> anyhow::Result<SecretsService<SqliteStorage, GnuPgOpenPgp>> {
    Ok(SecretsService::new(open_storage()?, GnuPgOpenPgp))
}

fn load_store_snapshot() -> anyhow::Result<StoreSnapshot> {
    read_store_snapshot()
}

fn read_store_snapshot() -> anyhow::Result<StoreSnapshot> {
    let storage = open_storage()?;
    let (folders, secrets) = storage.read_snapshot()?;
    Ok(store_snapshot(folders, secrets))
}

fn activate_store_snapshot(path: std::path::PathBuf) -> anyhow::Result<(StoreSnapshot, bool)> {
    let storage = SqliteStorage::open_existing(&path)?;
    let (folders, secrets) = storage.read_snapshot()?;
    let snapshot = store_snapshot(folders, secrets);
    let durable = app_paths::set_active_vault_path(crate::app_info::APP.desktop_file_id, &path)?;
    Ok((snapshot, durable))
}

fn open_storage() -> anyhow::Result<SqliteStorage> {
    let app_id = crate::app_info::APP.desktop_file_id;
    if let Some(path) = app_paths::active_vault_path(app_id)? {
        SqliteStorage::open_existing(path)
    } else {
        SqliteStorage::open(app_paths::database_path(app_id)?)
    }
}

fn snapshot_after_mutation() -> Result<StoreSnapshot, ()> {
    read_store_snapshot().map_err(|_| ())
}

fn parse_secret_id(value: &str) -> Option<SecretId> {
    Uuid::parse_str(value).ok().map(SecretId)
}

fn parse_folder_id(value: &str) -> Option<FolderId> {
    Uuid::parse_str(value).ok().map(FolderId)
}
