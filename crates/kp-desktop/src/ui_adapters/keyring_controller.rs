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

    extern "RustQt" {
        /// GPG identity feature controller. It exposes only view-safe metadata.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(bool, busy)]
        #[qproperty(bool, operation_busy, cxx_name = "operationBusy", READ, NOTIFY)]
        #[qproperty(QStringList, key_summaries, cxx_name = "keySummaries")]
        #[qproperty(i32, key_count, cxx_name = "keyCount")]
        #[qproperty(QStringList, key_records, cxx_name = "keyRecords", READ, NOTIFY)]
        #[qproperty(QString, load_error, cxx_name = "loadError", READ, NOTIFY)]
        #[qproperty(QString, operation_error, cxx_name = "operationError", READ, NOTIFY)]
        #[qproperty(QString, online_settings, cxx_name = "onlineSettings", READ, NOTIFY)]
        #[qproperty(QString, key_availability, cxx_name = "keyAvailability", READ, NOTIFY)]
        #[qproperty(QString, identity_catalog, cxx_name = "identityCatalog", READ, NOTIFY)]
        #[qproperty(QString, catalog_error, cxx_name = "catalogError", READ, NOTIFY)]
        #[qproperty(QString, identity_health, cxx_name = "identityHealth", READ, NOTIFY)]
        type KeyringController = super::KeyringControllerRust;

        #[qinvokable]
        #[cxx_name = "editCatalog"]
        fn edit_catalog(self: Pin<&mut Self>, json: &QString);

        #[qinvokable]
        #[cxx_name = "copySshPublicKey"]
        fn copy_ssh_public_key(self: Pin<&mut Self>, fingerprint: &QString, target: &QString);

        #[qsignal]
        #[cxx_name = "sshPublicKeyReady"]
        fn ssh_public_key_ready(self: Pin<&mut Self>, public_key: &QString);

        #[qinvokable]
        #[cxx_name = "publishIdentity"]
        fn publish_identity(
            self: Pin<&mut Self>,
            fingerprint: &QString,
            server: &QString,
            upload: bool,
        );

        #[qsignal]
        #[cxx_name = "catalogSaved"]
        fn catalog_saved(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "matchesSearch"]
        fn matches_search(self: Pin<&mut Self>, query: &QString, metadata: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "saveOnlineSettings"]
        fn save_online_settings(self: Pin<&mut Self>, json: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "openOnlineService"]
        fn open_online_service(self: Pin<&mut Self>, index: i32, fingerprint: &QString);

        /// Reloads public OpenPGP identity metadata without blocking Qt.
        #[qinvokable]
        #[cxx_name = "refreshKeys"]
        fn refresh_keys(self: Pin<&mut Self>);

        /// Runs the system-agent signing probe without blocking Qt.
        #[qinvokable]
        #[cxx_name = "testAgentPrompt"]
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
    /// Last listed keys, kept so identity health follows catalog edits without asking GnuPG.
    keys: Vec<kryptopass_keyring::KeyInfo>,
    identity_health: QString,
    key_availability: QString,
    identity_catalog: QString,
    catalog_error: QString,
    metadata_search: kryptopass_core::search::MetadataSearch,
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
        let (catalog, catalog_error) = match crate::keyring::catalog::read()
            .and_then(|catalog| Ok(serde_json::to_string(&catalog)?))
        {
            Ok(catalog) => (catalog, String::new()),
            Err(_) => (
                "{}".into(),
                crate::i18n::text(
                    "No se pudo leer la organización de tus identidades. No se sobrescribirá el archivo.",
                ),
            ),
        };
        Self {
            keys: Vec::new(),
            identity_health: QString::from("{}"),
            key_availability: QString::from("{}"),
            identity_catalog: QString::from(catalog),
            catalog_error: QString::from(catalog_error),
            metadata_search: kryptopass_core::search::MetadataSearch::default(),
            online_settings: QString::from(serde_json::to_string(&settings).unwrap_or_default()),
            status: QString::from(crate::i18n::text("Identidades GPG no cargadas")),
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
    pub fn copy_ssh_public_key(mut self: Pin<&mut Self>, fingerprint: &QString, target: &QString) {
        if *self.busy() {
            return;
        }
        let fingerprint = fingerprint.to_string();
        let target = target.to_string();
        self.as_mut().set_busy(true);
        self.as_mut().set_operation_error(QString::default());
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || {
                let fingerprint = kryptopass_keyring::Fingerprint::parse(&fingerprint)?;
                let target = kryptopass_keyring::Fingerprint::parse(&target)?;
                kryptopass_keyring::Keyring::system().public_ssh_key(&fingerprint, &target)
            },
            |mut controller, result| {
                controller.as_mut().set_busy(false);
                match result {
                    Ok(public_key) => controller.ssh_public_key_ready(&QString::from(public_key)),
                    Err(_) => controller.fail_operation(
                        "No se pudo exportar la clave pública SSH seleccionada.".into(),
                    ),
                }
            },
        );
    }

    pub fn publish_identity(
        mut self: Pin<&mut Self>,
        fingerprint: &QString,
        server: &QString,
        upload: bool,
    ) {
        if *self.busy() {
            return;
        }
        let fingerprint = fingerprint.to_string();
        let server = server.to_string();
        let allowed = serde_json::from_str::<OnlineSettings>(&self.online_settings().to_string())
            .is_ok_and(|settings| settings.enabled && settings.keyservers.contains(&server));
        if !allowed {
            self.fail_operation(
                "Activa los servicios online y selecciona un servidor configurado.".into(),
            );
            return;
        }
        self.as_mut().set_busy(true);
        self.as_mut().set_operation_error(QString::default());
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || crate::keyring::publication::run(&fingerprint, &server, upload),
            |mut controller, result| {
                controller.as_mut().set_busy(false);
                controller.as_mut().reload_catalog();
                if let Err(error) = result {
                    controller.fail_operation(error.to_string());
                }
            },
        );
    }

    pub fn edit_catalog(mut self: Pin<&mut Self>, json: &QString) {
        if *self.busy() {
            return;
        }
        let request = json.to_string();
        self.as_mut().set_busy(true);
        self.as_mut().set_operation_error(QString::default());
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || {
                let keys = crate::keyring::commands::list_keys().map_err(anyhow::Error::msg)?;
                crate::keyring::catalog::edit(&request, &keys).map(drop)
            },
            |mut controller, result| {
                controller.as_mut().set_busy(false);
                match result {
                    Ok(()) => {
                        controller.as_mut().reload_catalog();
                        controller.catalog_saved();
                    }
                    Err(error) => controller.fail_operation(error.to_string()),
                }
            },
        );
    }

    fn reload_catalog(mut self: Pin<&mut Self>) {
        match crate::keyring::catalog::read().and_then(|catalog| {
            let health = crate::keyring::health::assess(
                &self.keys,
                &catalog,
                crate::keyring::catalog::now(),
            );
            Ok((serde_json::to_string(&catalog)?, health.to_string()))
        }) {
            Ok((json, health)) => {
                self.as_mut().rust_mut().identity_catalog = QString::from(json);
                self.as_mut().identity_catalog_changed();
                self.as_mut().rust_mut().identity_health = QString::from(health);
                self.as_mut().identity_health_changed();
                self.as_mut().rust_mut().catalog_error = QString::default();
            }
            Err(_) => {
                self.as_mut().rust_mut().catalog_error = QString::from(crate::i18n::text(
                    "No se pudo leer la organización de tus identidades. No se sobrescribirá el archivo.",
                ))
            }
        }
        self.catalog_error_changed();
    }

    pub fn matches_search(self: Pin<&mut Self>, query: &QString, metadata: &QString) -> bool {
        self.rust_mut()
            .metadata_search
            .matches(&query.to_string(), &metadata.to_string())
    }

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

        self.as_mut().reload_catalog();
        if !self.load_error().is_empty() {
            self.as_mut().rust_mut().load_error = QString::default();
            self.as_mut().load_error_changed();
        }
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Cargando claves GPG…"));

        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            || {
                let keys = crate::keyring::commands::list_keys();
                let observation = keys
                    .as_ref()
                    .ok()
                    .map(|keys| crate::keyring::catalog::observe(keys));
                (keys, crate::keyring::commands::availability(), observation)
            },
            |mut controller, (result, availability, observation)| {
                controller.as_mut().rust_mut().key_availability =
                    QString::from(availability.unwrap_or_else(|_| "{}".into()));
                controller.as_mut().key_availability_changed();
                controller.as_mut().rust_mut().keys = result.clone().unwrap_or_default();
                controller.as_mut().reload_catalog();
                if observation.is_some_and(|result| result.is_err()) {
                    controller.as_mut().rust_mut().catalog_error = QString::from(
                        crate::i18n::text("No se pudo guardar la organización de tus identidades."),
                    );
                    controller.as_mut().catalog_error_changed();
                }
                match result {
                    Ok(keys) => {
                        controller.as_mut().rust_mut().key_records = key_records(&keys);
                        controller.as_mut().key_records_changed();
                        let (summaries, count) = key_summaries(keys);
                        controller.as_mut().set_key_summaries(summaries);
                        controller.as_mut().set_key_count(count);
                        controller.as_mut().set_status(crate::i18n::plural(
                            "%n clave GPG cargada",
                            count as usize,
                        ));
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
                let observation = keys
                    .as_ref()
                    .ok()
                    .map(|keys| crate::keyring::catalog::observe(keys));
                (
                    outcome,
                    keys,
                    crate::keyring::commands::availability(),
                    observation,
                )
            },
            move |mut controller, (outcome, keys, availability, observation)| {
                controller.as_mut().rust_mut().key_availability =
                    QString::from(availability.unwrap_or_else(|_| "{}".into()));
                controller.as_mut().key_availability_changed();
                if observation.is_some_and(|result| result.is_err()) {
                    controller
                        .as_mut()
                        .set_operation_error(QString::from(crate::i18n::text(
                            "No se pudo guardar la organización de tus identidades.",
                        )));
                }
                let reload_error = match keys {
                    Ok(keys) => {
                        controller.as_mut().rust_mut().keys = keys.clone();
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
                            controller.as_mut().set_operation_error(QString::from(crate::i18n::format("La operación se completó, pero no se pudo actualizar la lista de claves: %1", &[error.to_string()])));
                        }
                        if action == "copyPublic"
                            && let Some(armored) = outcome.public_key
                        {
                            let armored = QString::from(armored);
                            controller.as_mut().public_key_ready(&armored);
                        }
                        if crate::keyring::catalog::record_export(&fingerprint.to_string(), &action)
                            .is_err()
                        {
                            controller.as_mut().set_operation_error(QString::from(crate::i18n::text("El archivo se exportó, pero no se pudo guardar la fecha en KryptoPass.")));
                        }
                        controller.as_mut().reload_catalog();
                        let message = QString::from(outcome.message);
                        controller.as_mut().set_status(message.clone());
                        controller
                            .as_mut()
                            .operation_finished(&fingerprint, &message);
                    }
                    Err(message) => {
                        let message = match reload_error {
                            Some(error) => crate::i18n::format(
                                "%1 Además, no se pudo actualizar la lista de claves: %2",
                                &[message.to_string(), error.to_string()],
                            ),
                            None => message,
                        };
                        controller.as_mut().reload_catalog();
                        controller.as_mut().fail_operation(message);
                    }
                }
            },
        );
    }

    pub fn clear_operation_error(mut self: Pin<&mut Self>) {
        self.as_mut().set_operation_error(QString::default());
    }

    fn set_status(mut self: Pin<&mut Self>, value: QString) {
        let value = crate::i18n::translate(&value);
        if self.status() != &value {
            self.as_mut().rust_mut().status = value;
            self.status_changed();
        }
    }

    fn set_operation_busy(mut self: Pin<&mut Self>, value: bool) {
        if *self.operation_busy() != value {
            self.as_mut().rust_mut().operation_busy = value;
            self.operation_busy_changed();
        }
    }

    fn set_operation_error(mut self: Pin<&mut Self>, value: QString) {
        let value = crate::i18n::translate(&value);
        if self.operation_error() != &value {
            self.as_mut().rust_mut().operation_error = value;
            self.operation_error_changed();
        }
    }

    fn fail_operation(mut self: Pin<&mut Self>, message: String) {
        let message = crate::i18n::translate(&QString::from(message));
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
