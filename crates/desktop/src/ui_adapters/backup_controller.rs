use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QUrl};

use crate::infrastructure::backup::{
    BackupPublishOutcome, export_backup, parse_selected_recipients, restore_backup,
};
use crate::infrastructure::{GnuPgOpenPgp, app_paths};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qurl.h");

        type QString = cxx_qt_lib::QString;
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        /// Exports and restores encrypted vault backup files.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(
            QUrl,
            vault_directory_url,
            cxx_name = "vaultDirectoryUrl",
            READ,
            CONSTANT
        )]
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

    impl cxx_qt::Threading for BackupController {}
}

pub struct BackupControllerRust {
    busy: bool,
    error_message: QString,
    status: QString,
    vault_directory_url: QUrl,
}

impl Default for BackupControllerRust {
    fn default() -> Self {
        Self {
            busy: false,
            error_message: QString::default(),
            status: QString::from("Copias de seguridad listas"),
            vault_directory_url: default_vault_directory_url(),
        }
    }
}

impl qobject::BackupController {
    pub fn clear_error(mut self: Pin<&mut Self>) {
        self.as_mut().set_error_message(QString::default());
        if !*self.busy() {
            self.as_mut()
                .set_status(QString::from("Copias de seguridad listas"));
        }
    }

    pub fn export_backup(mut self: Pin<&mut Self>, path: &QUrl, recipients_json: &QString) {
        if *self.busy() {
            return;
        }
        let Some(path) = local_path(path) else {
            self.as_mut()
                .fail("El destino de la copia no es una ruta local válida.");
            return;
        };
        let recipients = match parse_selected_recipients(&recipients_json.to_string()) {
            Ok(recipients) => recipients,
            Err(_) => {
                self.as_mut()
                    .fail("Selecciona destinatarios válidos para cifrar el backup.");
                return;
            }
        };

        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Cifrando y guardando backup…"));
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || export_backup(&path, recipients, &GnuPgOpenPgp).map(|outcome| (outcome, path)),
            move |mut controller, result| {
                controller.as_mut().set_busy(false);
                match result {
                    Ok((outcome, path)) => {
                        controller.as_mut().set_status(publish_status(outcome));
                        let url = path_to_url(&path);
                        controller.as_mut().backup_exported(&url);
                    }
                    Err(_) => controller
                        .as_mut()
                        .fail("No se pudo guardar el backup cifrado."),
                }
            },
        );
    }

    pub fn restore_backup(mut self: Pin<&mut Self>, input: &QUrl, output: &QUrl) {
        if *self.busy() {
            return;
        }
        let (Some(input), Some(output)) = (local_path(input), local_path(output)) else {
            self.as_mut()
                .fail("Selecciona archivos locales válidos para restaurar.");
            return;
        };

        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Descifrando y validando backup…"));
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || restore_backup(&input, &output, &GnuPgOpenPgp).map(|outcome| (outcome, output)),
            move |mut controller, result| {
                controller.as_mut().set_busy(false);
                match result {
                    Ok((outcome, path)) => {
                        controller.as_mut().set_status(publish_status(outcome));
                        let url = path_to_url(&path);
                        controller.as_mut().backup_restored(&url);
                    }
                    Err(_) => controller
                        .as_mut()
                        .fail("No se pudo restaurar el backup; la bóveda actual no cambió."),
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

    fn fail(mut self: Pin<&mut Self>, message: &'static str) {
        self.as_mut().set_error_message(QString::from(message));
        self.as_mut().set_status(QString::from(message));
        self.operation_failed();
    }
}

fn local_path(url: &QUrl) -> Option<PathBuf> {
    if !url.is_valid() || !url.is_local_file() || url.has_query() || url.has_fragment() {
        return None;
    }
    let path = url.to_local_file()?.to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn default_vault_directory_url() -> QUrl {
    let Ok(database) = app_paths::database_path(crate::app_info::APP.desktop_file_id) else {
        return QUrl::default();
    };
    database.parent().map(path_to_url).unwrap_or_default()
}

fn path_to_url(path: &Path) -> QUrl {
    QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()))
}

fn publish_status(outcome: BackupPublishOutcome) -> QString {
    match outcome {
        BackupPublishOutcome::Durable => QString::from("Backup guardado correctamente."),
        BackupPublishOutcome::PublishedButDurabilityUnconfirmed => QString::from(
            "El archivo se publicó, pero no se pudo confirmar su durabilidad en disco.",
        ),
    }
}
