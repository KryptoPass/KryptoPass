use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QUrl};

use crate::infrastructure::backup::{
    BackupPublishOutcome, RestoreFailure, export_backup, parse_selected_recipients, restore_backup,
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

    impl cxx_qt::Threading for BackupController {}
}

#[derive(Default)]
pub struct BackupControllerRust {
    busy: bool,
    error_message: QString,
    status: QString,
}

impl qobject::BackupController {
    pub fn clear_error(mut self: Pin<&mut Self>) {
        self.as_mut().set_error_message(QString::default());
        if !*self.busy() {
            self.as_mut().set_status(QString::default());
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
                    .fail("Selecciona al menos una clave válida para proteger la copia.");
                return;
            }
        };

        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Cifrando y guardando la copia…"));
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
                        .fail("No se pudo guardar la copia. Comprueba que puedes escribir en la carpeta elegida y que las claves seleccionadas siguen disponibles."),
                }
            },
        );
    }

    pub fn restore_managed_backup(mut self: Pin<&mut Self>, input: &QUrl) {
        if *self.busy() {
            return;
        }
        match app_paths::restored_vault_path(crate::app_info::APP.desktop_file_id) {
            Ok(path) => self.restore_backup(input, &path_to_url(&path)),
            Err(_) => self.as_mut().fail(
                "No se pudo preparar la carpeta privada para restaurar. Comprueba los permisos y el espacio disponible en la carpeta de datos de KryptoPass.",
            ),
        }
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
            .set_status(QString::from("Descifrando y comprobando la copia…"));
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
                    Err(error) => controller.as_mut().fail(restore_error_message(&error)),
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

fn path_to_url(path: &Path) -> QUrl {
    QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()))
}

fn publish_status(outcome: BackupPublishOutcome) -> QString {
    match outcome {
        BackupPublishOutcome::Durable => QString::from("Archivo guardado correctamente."),
        BackupPublishOutcome::PublishedButDurabilityUnconfirmed => QString::from(
            "El archivo se guardó, pero no se pudo confirmar que todos los cambios llegaron al disco. Conserva la copia original.",
        ),
    }
}

fn restore_error_message(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<RestoreFailure>() {
        Some(RestoreFailure::Input) => {
            "No se pudo leer la copia. Selecciona un archivo de copia accesible; el archivo puede haberse movido o superar el tamaño permitido."
        }
        Some(RestoreFailure::Destination) => {
            "No se pudo crear la bóveda nueva. Comprueba el espacio disponible y vuelve a intentar la restauración."
        }
        Some(RestoreFailure::Decryption) => {
            "No se pudo descifrar la copia. Necesitas la clave privada de alguien con acceso a ella. Comprueba que esté importada y vuelve a intentar; si GnuPG pide una contraseña, introdúcela en su ventana. El archivo también podría estar dañado."
        }
        Some(RestoreFailure::InvalidBackup) => {
            "El archivo descifrado no es una copia compatible de KryptoPass o está dañado. Selecciona otra copia o prueba con una versión más reciente de KryptoPass."
        }
        None => {
            "No se pudo completar la restauración. Comprueba el espacio disponible y los permisos de la carpeta de datos de KryptoPass."
        }
    }
}
