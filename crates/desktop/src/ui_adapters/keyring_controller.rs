use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList, QUrl};
use serde_json::Value;
use std::pin::Pin;

use super::native_ffi::ffi as native_ffi;
use crate::keyring::online::{OnlineSettings, service_url};

use super::keyring_projection::{key_records, key_summaries};

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

    #[auto_cxx_name]
    extern "RustQt" {
        /// GPG identity feature controller. It exposes only view-safe metadata.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status)]
        #[qproperty(bool, busy)]
        #[qproperty(bool, operation_busy, cxx_name = "operationBusy", READ, NOTIFY)]
        #[qproperty(QStringList, key_summaries)]
        #[qproperty(i32, key_count)]
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

        /// Reloads public OpenPGP identity metadata without blocking Qt.
        #[qinvokable]
        fn refresh_keys(self: Pin<&mut Self>);

        /// Runs the system-agent signing probe without blocking Qt.
        #[qinvokable]
        fn test_agent_prompt(self: Pin<&mut Self>);

        /// Executes a typed keyring operation without blocking Qt.
        #[qinvokable]
        #[cxx_name = "runOperation"]
        fn run_operation(self: Pin<&mut Self>, request_json: &QString);

        /// Clears the last keyring-operation error.
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

    impl cxx_qt::Threading for KeyringController {}
}

pub struct KeyringControllerRust {
    online_settings: QString,
    status: QString,
    busy: bool,
    operation_busy: bool,
    key_summaries: QStringList,
    key_count: i32,
    key_records: QStringList,
    load_error: QString,
    operation_error: QString,
}

impl Default for KeyringControllerRust {
    fn default() -> Self {
        let stored = unsafe { native_ffi::online_settings() };
        let settings = serde_json::from_str::<OnlineSettings>(&String::from(&stored))
            .ok()
            .filter(|settings| settings.validate().is_ok())
            .unwrap_or_default();
        Self {
            online_settings: QString::from(serde_json::to_string(&settings).unwrap_or_default()),
            status: QString::from("Identidades GPG no cargadas"),
            busy: false,
            operation_busy: false,
            key_summaries: QStringList::default(),
            key_count: 0,
            key_records: QStringList::default(),
            load_error: QString::default(),
            operation_error: QString::default(),
        }
    }
}

impl qobject::KeyringController {
    pub fn save_online_settings(mut self: Pin<&mut Self>, json: &QString) -> bool {
        let result = serde_json::from_str::<OnlineSettings>(&String::from(json))
            .map_err(|_| "No se pudo leer la configuración de servicios online.".to_owned())
            .and_then(|settings| settings.validate());
        if let Err(error) = result {
            self.as_mut().set_operation_error(QString::from(error));
            return false;
        }
        if !unsafe { native_ffi::save_online_settings(json) } {
            self.as_mut()
                .set_operation_error(QString::from("No se pudo guardar la configuración."));
            return false;
        }
        self.as_mut().rust_mut().online_settings = json.clone();
        self.as_mut().online_settings_changed();
        self.as_mut().set_operation_error(QString::default());
        true
    }

    pub fn open_online_service(mut self: Pin<&mut Self>, index: i32, fingerprint: &QString) {
        let result = (|| {
            let settings: OnlineSettings =
                serde_json::from_str(&String::from(self.online_settings()))
                    .map_err(|_| "No se pudo leer la configuración.".to_owned())?;
            if !settings.enabled {
                return Err("Los servicios online están desactivados.".to_owned());
            }
            let template = usize::try_from(index)
                .ok()
                .and_then(|index| settings.services.get(index))
                .ok_or_else(|| "Selecciona un servicio online.".to_owned())?;
            let url = service_url(template, &String::from(fingerprint))?;
            if !unsafe { native_ffi::open_online_url(&QString::from(url)) } {
                return Err("No se pudo abrir el navegador predeterminado de KDE.".to_owned());
            }
            Ok(())
        })();
        self.as_mut()
            .set_operation_error(QString::from(result.err().unwrap_or_default()));
    }

    pub fn refresh_keys(mut self: Pin<&mut Self>) {
        if *self.busy() {
            return;
        }

        if !self.load_error().is_empty() {
            self.as_mut().rust_mut().load_error = QString::default();
            self.as_mut().load_error_changed();
        }
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Cargando claves GPG…"));

        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            crate::keyring::commands::list_keys,
            |mut controller, result| {
                match result {
                    Ok(keys) => {
                        controller.as_mut().rust_mut().key_records = key_records(&keys);
                        controller.as_mut().key_records_changed();
                        let (summaries, count) = key_summaries(keys);
                        controller.as_mut().set_key_summaries(summaries);
                        controller.as_mut().set_key_count(count);
                        controller
                            .as_mut()
                            .set_status(QString::from(format!("Claves GPG cargadas: {count}")));
                    }
                    Err(error) => {
                        controller.as_mut().rust_mut().load_error =
                            QString::from(error.to_string());
                        controller.as_mut().load_error_changed();
                        controller.as_mut().rust_mut().key_records = QStringList::default();
                        controller.as_mut().key_records_changed();
                        controller
                            .as_mut()
                            .set_key_summaries(QStringList::default());
                        controller.as_mut().set_key_count(0);
                        controller
                            .as_mut()
                            .set_status(QString::from("Error al cargar claves GPG"));
                    }
                }

                controller.set_busy(false);
            },
        );
    }

    pub fn test_agent_prompt(mut self: Pin<&mut Self>) {
        if *self.busy() {
            return;
        }

        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Esperando al agente GPG…"));

        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            crate::keyring::commands::probe_agent,
            |controller, result| {
                let status = match result {
                    Ok(()) => {
                        "Firma de prueba completada; si el agente ya tenía la passphrase en caché, no apareció diálogo"
                    }
                    Err(error) => {
                        return controller_done(
                            controller,
                            format!("Prueba del agente GPG falló: {error}"),
                        );
                    }
                };

                controller_done(controller, status.to_owned());
            },
        );
    }

    pub fn run_operation(mut self: Pin<&mut Self>, request_json: &QString) {
        if *self.busy() {
            return;
        }

        self.as_mut().set_operation_error(QString::default());

        let (request_json, action) = match normalize_operation_request(&String::from(request_json))
        {
            Ok(request) => request,
            Err(message) => {
                self.as_mut().fail_operation(message);
                return;
            }
        };

        if action == "refreshExternal" {
            let enabled =
                serde_json::from_str::<OnlineSettings>(&String::from(self.online_settings()))
                    .is_ok_and(|settings| settings.enabled);
            if !enabled {
                self.as_mut()
                    .fail_operation("Las consultas online están desactivadas.".into());
                return;
            }
        }

        self.as_mut().set_operation_busy(true);
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Ejecutando operación OpenPGP…"));

        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || {
                let outcome = crate::keyring::commands::execute_operation(&request_json);
                // Reload on both success and failure because an operation can commit a partial
                // result before reporting an error.
                let keys = crate::keyring::commands::list_keys();
                (outcome, keys)
            },
            move |mut controller, (outcome, keys)| {
                let reload_error = match keys {
                    Ok(keys) => {
                        controller.as_mut().rust_mut().key_records = key_records(&keys);
                        controller.as_mut().key_records_changed();
                        let (summaries, count) = key_summaries(keys);
                        controller.as_mut().set_key_summaries(summaries);
                        controller.as_mut().set_key_count(count);
                        None
                    }
                    Err(error) => Some(error.to_string()),
                };

                controller.as_mut().set_busy(false);
                controller.as_mut().set_operation_busy(false);

                match outcome {
                    Ok(outcome) => {
                        let fingerprint =
                            QString::from(outcome.selected_fingerprint.unwrap_or_default());
                        if let Some(error) = reload_error {
                            controller.as_mut().set_operation_error(QString::from(format!(
                                "La operación se completó, pero no se pudo actualizar la lista de claves: {error}"
                            )));
                        }
                        if action == "copyPublic"
                            && let Some(armored) = outcome.public_key
                        {
                            let armored = QString::from(armored);
                            controller.as_mut().public_key_ready(&armored);
                        }
                        let message = QString::from(outcome.message);
                        controller.as_mut().set_status(message.clone());
                        controller
                            .as_mut()
                            .operation_finished(&fingerprint, &message);
                    }
                    Err(message) => {
                        let message = match reload_error {
                            Some(error) => format!(
                                "{message} Además, no se pudo actualizar la lista de claves: {error}"
                            ),
                            None => message,
                        };
                        controller.as_mut().fail_operation(message);
                    }
                }
            },
        );
    }

    pub fn clear_operation_error(mut self: Pin<&mut Self>) {
        self.as_mut().set_operation_error(QString::default());
    }

    fn set_operation_busy(mut self: Pin<&mut Self>, value: bool) {
        if *self.operation_busy() != value {
            self.as_mut().rust_mut().operation_busy = value;
            self.operation_busy_changed();
        }
    }

    fn set_operation_error(mut self: Pin<&mut Self>, value: QString) {
        if self.operation_error() != &value {
            self.as_mut().rust_mut().operation_error = value;
            self.operation_error_changed();
        }
    }

    fn fail_operation(mut self: Pin<&mut Self>, message: String) {
        let message = QString::from(message);
        self.as_mut().set_operation_error(message.clone());
        self.as_mut().set_status(message.clone());
        self.as_mut().operation_failed(&message);
    }
}

fn normalize_operation_request(request_json: &str) -> Result<(String, String), String> {
    if request_json.len() > 64 * 1024 {
        return Err("La solicitud de operación OpenPGP supera el tamaño permitido.".to_owned());
    }

    let mut request: Value = serde_json::from_str(request_json)
        .map_err(|_| "La solicitud de operación OpenPGP no es JSON válido.".to_owned())?;
    let Some(fields) = request.as_object_mut() else {
        return Err("La solicitud de operación OpenPGP no tiene un formato válido.".to_owned());
    };
    let action = fields
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    if let Some(path) = fields.get_mut("path") {
        let Value::String(path_value) = path else {
            return Err("Selecciona una ruta local válida para la operación.".to_owned());
        };
        if path_value.trim().is_empty() {
            return Err("Selecciona una ruta local válida para la operación.".to_owned());
        }
        let path_url = if path_value.starts_with('/') {
            QUrl::from_local_file(&QString::from(path_value.as_str()))
        } else {
            QUrl::from(path_value.as_str())
        };
        if !path_url.is_valid()
            || !path_url.is_local_file()
            || path_url.has_query()
            || path_url.has_fragment()
            || !path_url.host_or_default().is_empty()
        {
            return Err("Selecciona un archivo local; no se aceptan URL remotas.".to_owned());
        }
        let local_path = path_url
            .to_local_file()
            .map(|local_path| String::from(&local_path))
            .filter(|local_path| !local_path.is_empty())
            .ok_or_else(|| "Selecciona una ruta local válida para la operación.".to_owned())?;
        *path = Value::String(local_path);
    }

    let request_json = serde_json::to_string(&request)
        .map_err(|_| "No se pudo preparar la solicitud de operación OpenPGP.".to_owned())?;
    Ok((request_json, action))
}

fn controller_done(mut controller: Pin<&mut qobject::KeyringController>, status: String) {
    controller.as_mut().set_status(QString::from(status));
    controller.set_busy(false);
}
