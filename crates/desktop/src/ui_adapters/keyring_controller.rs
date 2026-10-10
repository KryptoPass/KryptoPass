use cxx_qt::Threading;
use cxx_qt_lib::{QString, QStringList};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qstringlist.h");

        type QString = cxx_qt_lib::QString;
        type QStringList = cxx_qt_lib::QStringList;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// GPG identity feature controller. It exposes only view-safe metadata.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status)]
        #[qproperty(bool, busy)]
        #[qproperty(QStringList, key_summaries)]
        #[qproperty(i32, key_count)]
        type KeyringController = super::KeyringControllerRust;

        /// Reloads public OpenPGP identity metadata without blocking Qt.
        #[qinvokable]
        fn refresh_keys(self: Pin<&mut Self>);

        /// Runs the system-agent signing probe without blocking Qt.
        #[qinvokable]
        fn test_agent_prompt(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for KeyringController {}
}

pub struct KeyringControllerRust {
    status: QString,
    busy: bool,
    key_summaries: QStringList,
    key_count: i32,
}

impl Default for KeyringControllerRust {
    fn default() -> Self {
        Self {
            status: QString::from("Identidades GPG no cargadas"),
            busy: false,
            key_summaries: QStringList::default(),
            key_count: 0,
        }
    }
}

fn key_summaries(keys: Vec<kryptopass_core::KeyInfo>) -> (QStringList, i32) {
    let key_count = i32::try_from(keys.len()).unwrap_or(i32::MAX);
    let summaries = keys
        .into_iter()
        .map(|key| {
            let identity = key
                .user_ids
                .first()
                .map(|user_id| user_id.id.as_str())
                .unwrap_or("Sin identidad");

            QString::from(format!("{identity}\n{}", key.fingerprint))
        })
        .collect();

    (summaries, key_count)
}

impl qobject::KeyringController {
    pub fn refresh_keys(mut self: Pin<&mut Self>) {
        if *self.busy() {
            return;
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
                        let (summaries, count) = key_summaries(keys);
                        controller.as_mut().set_key_summaries(summaries);
                        controller.as_mut().set_key_count(count);
                        controller
                            .as_mut()
                            .set_status(QString::from(format!("Claves GPG cargadas: {count}")));
                    }
                    Err(error) => {
                        controller
                            .as_mut()
                            .set_key_summaries(QStringList::default());
                        controller.as_mut().set_key_count(0);
                        controller.as_mut().set_status(QString::from(format!(
                            "No se pudieron cargar las claves GPG: {error}"
                        )));
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
}

fn controller_done(mut controller: Pin<&mut qobject::KeyringController>, status: String) {
    controller.as_mut().set_status(QString::from(status));
    controller.set_busy(false);
}
