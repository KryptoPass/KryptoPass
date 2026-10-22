use crate::import::{PreparedImport, catalog, presentation, receive_import, review_export};
use crate::{backup::parse_selected_recipients, storage::paths as app_paths};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QUrl};
use kryptopass_core::{ImportFolderPolicy, ImportOptions};
use std::{path::PathBuf, pin::Pin};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qurl.h");
        type QString = cxx_qt_lib::QString;
        type QUrl = cxx_qt_lib::QUrl;
    }
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(bool, reviewed, READ, NOTIFY)]
        #[qproperty(QString, importers_json, cxx_name = "importersJson", READ, NOTIFY)]
        #[qproperty(QString, summary_json, cxx_name = "summaryJson", READ, NOTIFY)]
        #[qproperty(QString, warnings_json, cxx_name = "warningsJson", READ, NOTIFY)]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        type ImportController = super::ImportControllerRust;

        #[qinvokable]
        fn reset(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "clearSensitiveState"]
        fn clear_sensitive_state(self: Pin<&mut Self>);
        #[qinvokable]
        fn review(self: Pin<&mut Self>, input: &QUrl, importer_id: &QString);
        #[qinvokable]
        #[cxx_name = "importReviewed"]
        fn import_reviewed(self: Pin<&mut Self>, recipients_json: &QString);
        #[qsignal]
        #[cxx_name = "importCompleted"]
        fn import_completed(self: Pin<&mut Self>);
    }
    impl cxx_qt::Threading for ImportController {}
}

pub struct ImportControllerRust {
    busy: bool,
    reviewed: bool,
    importers_json: QString,
    summary_json: QString,
    warnings_json: QString,
    status: QString,
    error_message: QString,
    plan: Option<PreparedImport>,
    destination: Option<PathBuf>,
    review_operation: u64,
}

impl Default for ImportControllerRust {
    fn default() -> Self {
        let importers_json = catalog::json().unwrap_or_else(|_| {
            tracing::error!("Could not initialize importer catalog");
            "[]".into()
        });
        Self {
            busy: false,
            reviewed: false,
            importers_json: QString::from(importers_json),
            summary_json: QString::default(),
            warnings_json: QString::default(),
            status: QString::default(),
            error_message: QString::default(),
            plan: None,
            destination: None,
            review_operation: 0,
        }
    }
}

impl qobject::ImportController {
    pub fn reset(self: Pin<&mut Self>) {
        if *self.busy() {
            return;
        }
        self.clear_sensitive_state();
    }

    pub fn clear_sensitive_state(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.review_operation = state.review_operation.wrapping_add(1);
            state.plan = None;
            state.destination = None;
        }
        self.as_mut().set_reviewed(false);
        self.as_mut().set_summary_json(QString::default());
        self.as_mut().set_warnings_json(QString::default());
        self.as_mut().set_status(QString::default());
        self.set_error_message(QString::default());
    }

    pub fn review(mut self: Pin<&mut Self>, input: &QUrl, importer_id: &QString) {
        if *self.busy() {
            return;
        }
        self.as_mut().reset();
        let Some(input) = local_path(input) else {
            self.fail("Selecciona un archivo local del gestor elegido.");
            return;
        };
        let importer_id = importer_id.to_string();
        if importer_id.is_empty() {
            self.fail("Selecciona una integración de importación.");
            return;
        }
        let destination = match app_paths::database_path(crate::app_info::APP.desktop_file_id) {
            Ok(path) => path,
            Err(_) => {
                self.fail("No se pudo acceder a la bóveda actual.");
                return;
            }
        };
        self.as_mut().rust_mut().destination = Some(destination);
        let operation = self.as_ref().rust().review_operation;
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Leyendo y revisando la exportación…"));
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || review_export(&input, &importer_id),
            move |mut controller, result| {
                controller.as_mut().set_busy(false);
                if controller.as_ref().rust().review_operation != operation {
                    return;
                }
                match result {
                    Ok(plan) => {
                        controller
                            .as_mut()
                            .set_summary_json(QString::from(presentation::summary_json(&plan)));
                        controller
                            .as_mut()
                            .set_warnings_json(QString::from(presentation::warnings_json(&plan)));
                        let has_items = !plan.batch.items.is_empty();
                        controller.as_mut().rust_mut().plan = Some(plan);
                        controller.as_mut().set_reviewed(has_items);
                        controller.set_status(QString::from(if has_items {
                            "Revisa el resultado y elige las claves para los elementos que se añadirán."
                        } else {
                            "No hay elementos compatibles para importar."
                        }));
                    }
                    Err(error) => controller.fail(presentation::review_error(&error)),
                }
            },
        );
    }

    pub fn import_reviewed(mut self: Pin<&mut Self>, recipients_json: &QString) {
        if *self.busy() || !*self.reviewed() {
            return;
        }
        let recipients = match parse_selected_recipients(&recipients_json.to_string()) {
            Ok(value) => value,
            Err(_) => {
                self.fail("Selecciona al menos una clave válida para proteger tus datos.");
                return;
            }
        };
        let Some(destination) = self.as_ref().rust().destination.clone() else {
            return;
        };
        if app_paths::database_path(crate::app_info::APP.desktop_file_id)
            .ok()
            .as_ref()
            != Some(&destination)
        {
            self.as_mut().reset();
            self.fail("La bóveda activa cambió. Vuelve a revisar el archivo antes de importar.");
            return;
        }
        let Some(plan) = self.as_mut().rust_mut().plan.take() else {
            return;
        };
        self.as_mut().set_reviewed(false);
        self.as_mut().set_busy(true);
        self.as_mut().set_error_message(QString::default());
        self.as_mut().set_status(QString::from(
            "Cifrando y añadiendo los elementos a la bóveda actual…",
        ));
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || {
                receive_import(
                    plan.batch,
                    ImportOptions {
                        recipients,
                        folders: ImportFolderPolicy::PreserveHierarchy { parent: None },
                    },
                    &destination,
                )
            },
            |mut controller, result| {
                controller.as_mut().set_busy(false);
                match result {
                    Ok(receipt) => {
                        controller.as_mut().set_status(crate::i18n::plural("Importación completada: %n elemento añadido a la bóveda actual.", receipt.items_added));
                        controller.import_completed();
                    }
                    Err(_) => controller.fail("No se pudo añadir el lote. Los datos anteriores se conservaron. Comprueba las claves, los permisos y el espacio disponible; vuelve a revisar el archivo para reintentar."),
                }
            },
        );
    }

    fn set_busy(mut self: Pin<&mut Self>, value: bool) {
        if *self.busy() != value {
            self.as_mut().rust_mut().busy = value;
            self.busy_changed();
        }
    }

    fn set_reviewed(mut self: Pin<&mut Self>, value: bool) {
        if *self.reviewed() != value {
            self.as_mut().rust_mut().reviewed = value;
            self.reviewed_changed();
        }
    }

    fn set_summary_json(mut self: Pin<&mut Self>, value: QString) {
        if self.summary_json() != &value {
            self.as_mut().rust_mut().summary_json = value;
            self.summary_json_changed();
        }
    }

    fn set_warnings_json(mut self: Pin<&mut Self>, value: QString) {
        if self.warnings_json() != &value {
            self.as_mut().rust_mut().warnings_json = value;
            self.warnings_json_changed();
        }
    }

    fn set_status(mut self: Pin<&mut Self>, value: QString) {
        let value = crate::i18n::translate(&value);
        if self.status() != &value {
            self.as_mut().rust_mut().status = value;
            self.status_changed();
        }
    }

    fn set_error_message(mut self: Pin<&mut Self>, value: QString) {
        let value = crate::i18n::translate(&value);
        if self.error_message() != &value {
            self.as_mut().rust_mut().error_message = value;
            self.error_message_changed();
        }
    }

    fn fail(mut self: Pin<&mut Self>, message: &str) {
        self.as_mut().set_error_message(QString::from(message));
        self.set_status(QString::default());
    }
}

fn local_path(url: &QUrl) -> Option<PathBuf> {
    if !url.is_valid() || !url.is_local_file() || url.has_query() || url.has_fragment() {
        return None;
    }
    let path = url.to_local_file()?.to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}
