use crate::infrastructure::{
    app_paths,
    backup::parse_selected_recipients,
    import::{receive_import, review_proton_export},
};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QUrl};
use kryptopass_core::{ImportFolderPolicy, ImportOptions};
use kryptopass_import::{ImportError, ImportPlan, IssueKind};
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
        #[qproperty(QString, summary, READ, NOTIFY)]
        #[qproperty(QString, warnings, READ, NOTIFY)]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        type ImportController = super::ImportControllerRust;

        #[qinvokable]
        fn reset(self: Pin<&mut Self>);
        #[qinvokable]
        fn review(self: Pin<&mut Self>, input: &QUrl);
        #[qinvokable]
        #[cxx_name = "importReviewed"]
        fn import_reviewed(self: Pin<&mut Self>, recipients_json: &QString);
        #[qsignal]
        #[cxx_name = "importCompleted"]
        fn import_completed(self: Pin<&mut Self>);
    }
    impl cxx_qt::Threading for ImportController {}
}

#[derive(Default)]
pub struct ImportControllerRust {
    busy: bool,
    reviewed: bool,
    summary: QString,
    warnings: QString,
    status: QString,
    error_message: QString,
    plan: Option<ImportPlan>,
    destination: Option<PathBuf>,
}

impl qobject::ImportController {
    pub fn reset(mut self: Pin<&mut Self>) {
        if *self.busy() {
            return;
        }
        self.as_mut().rust_mut().plan = None;
        self.as_mut().rust_mut().destination = None;
        self.as_mut().set_reviewed(false);
        self.as_mut().set_summary(QString::default());
        self.as_mut().set_warnings(QString::default());
        self.as_mut().set_status(QString::default());
        self.set_error_message(QString::default());
    }

    pub fn review(mut self: Pin<&mut Self>, input: &QUrl) {
        if *self.busy() {
            return;
        }
        self.as_mut().reset();
        let Some(input) = local_path(input) else {
            self.fail("Selecciona un archivo local de Proton Pass.");
            return;
        };
        let destination = match app_paths::database_path(crate::app_info::APP.desktop_file_id) {
            Ok(path) => path,
            Err(_) => {
                self.fail("No se pudo acceder a la bóveda actual.");
                return;
            }
        };
        self.as_mut().rust_mut().destination = Some(destination);
        self.as_mut().set_busy(true);
        self.as_mut().set_status(QString::from("Leyendo la exportación… Si está cifrada, introduce su contraseña en la ventana de GnuPG."));
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || review_proton_export(&input),
            |mut controller, result| {
                controller.as_mut().set_busy(false);
                match result {
                    Ok(plan) => {
                        let s = plan.summary;
                        controller.as_mut().set_summary(QString::from(format!("{} bóvedas · {} carpetas · {} logins · {} notas · {} omitidos · {} en la papelera", s.vaults, s.folders, s.logins, s.notes, s.skipped, s.trashed)));
                        controller
                            .as_mut()
                            .set_warnings(QString::from(warning_text(&plan)));
                        let has_items = !plan.batch.items.is_empty();
                        controller.as_mut().rust_mut().plan = Some(plan);
                        controller.as_mut().set_reviewed(has_items);
                        controller.set_status(QString::from(if has_items {
                            "Revisa el resultado y elige las claves para los elementos que se añadirán."
                        } else {
                            "No hay elementos compatibles para importar."
                        }));
                    }
                    Err(error) => controller.fail(review_error(&error)),
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

    fn set_summary(mut self: Pin<&mut Self>, value: QString) {
        if self.summary() != &value {
            self.as_mut().rust_mut().summary = value;
            self.summary_changed();
        }
    }

    fn set_warnings(mut self: Pin<&mut Self>, value: QString) {
        if self.warnings() != &value {
            self.as_mut().rust_mut().warnings = value;
            self.warnings_changed();
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

fn review_error(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<ImportError>() {
        Some(ImportError::Decryption) => {
            "No se pudo descifrar la exportación. Revisa la contraseña en GnuPG y vuelve a intentarlo."
        }
        Some(ImportError::Limit) => {
            "La exportación supera el límite de 64 MiB o 10 000 registros. Exporta menos elementos."
        }
        Some(ImportError::Content) => {
            "Un elemento supera los límites actuales de KryptoPass o contiene un TOTP incompatible. Conserva el original y revisa la documentación de importación."
        }
        Some(_) => {
            "El archivo no es una exportación compatible de Proton Pass. Exporta de nuevo en formato ZIP o PGP y vuelve a intentarlo."
        }
        None => {
            "No se pudo leer el archivo. Comprueba su ubicación y los permisos y vuelve a intentarlo."
        }
    }
}

fn warning_text(plan: &ImportPlan) -> String {
    let categories = [
        (
            IssueKind::UnsupportedItem,
            "Elementos de tipos no compatibles omitidos",
        ),
        (
            IssueKind::AliasAsNote,
            "Alias conservados como notas; no administran correo",
        ),
        (
            IssueKind::ExtraFieldsInNotes,
            "Campos personalizados conservados como texto en las notas",
        ),
        (
            IssueKind::PasskeysInNotes,
            "Passkeys conservadas como referencia en las notas; no permiten autenticar",
        ),
        (
            IssueKind::AutofillRulesInNotes,
            "Reglas de autocompletado conservadas como referencia; solo se importan las URL predeterminadas",
        ),
        (
            IssueKind::AttachmentsNotImported,
            "Elementos con adjuntos que no se importan",
        ),
        (
            IssueKind::TimestampsNotImported,
            "Elementos cuyas fechas originales no se importan",
        ),
        (
            IssueKind::VaultDescriptionNotImported,
            "Descripciones de bóvedas que no se importan",
        ),
    ];
    categories
        .iter()
        .filter_map(|(kind, label)| {
            let count = plan
                .issues
                .iter()
                .filter(|issue| issue.kind == *kind)
                .count();
            (count > 0).then(|| format!("{label}: {count}."))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
