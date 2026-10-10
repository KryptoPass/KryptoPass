use cxx_qt::{CxxQtType, Threading};
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
        #[qproperty(QStringList, key_records, cxx_name = "keyRecords", READ, NOTIFY)]
        #[qproperty(QString, load_error, cxx_name = "loadError", READ, NOTIFY)]
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
    key_records: QStringList,
    load_error: QString,
}

impl Default for KeyringControllerRust {
    fn default() -> Self {
        Self {
            status: QString::from("Identidades GPG no cargadas"),
            busy: false,
            key_summaries: QStringList::default(),
            key_count: 0,
            key_records: QStringList::default(),
            load_error: QString::default(),
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

// Serialize only public display metadata across the QML boundary.
fn key_records(keys: &[kryptopass_core::KeyInfo]) -> QStringList {
    use serde_json::json;
    keys.iter().map(|key| {
        let identity = key.user_ids.iter().find(|uid| !uid.revoked && !uid.invalid).or_else(|| key.user_ids.first());
        let title = identity.and_then(|uid| uid.name.as_deref()).filter(|v| !v.is_empty()).unwrap_or("Sin nombre");
        let validity = |uid: Option<&kryptopass_core::UserIdInfo>| {
            use kryptopass_core::KeyValidity;
            match uid {
                Some(uid) if uid.revoked => ("revoked", "Identidad revocada"),
                Some(uid) if uid.invalid => ("invalid", "Identidad inválida"),
                Some(uid) => match uid.validity {
                    KeyValidity::Unknown => ("unknown", "Sin verificar"),
                    KeyValidity::Undefined => ("undefined", "Sin verificar"),
                    KeyValidity::Never => ("never", "No válida según GnuPG"),
                    KeyValidity::Marginal => ("marginal", "Validez marginal según GnuPG"),
                    KeyValidity::Full => ("full", "Validez plena según GnuPG"),
                    KeyValidity::Ultimate => ("ultimate", "Validez absoluta según GnuPG"),
                },
                None => ("unknown", "Sin verificar"),
            }
        };
        let (validity_code, validity_label) = validity(identity);
        let capabilities = |encrypt: bool, sign: bool, certify: bool, authenticate: bool| {
            [(encrypt, "Cifrar"), (sign, "Firmar"), (certify, "Certificar"), (authenticate, "Autenticar")]
                .into_iter().filter_map(|(enabled, name)| enabled.then_some(name)).collect::<Vec<_>>()
        };
        let status = |revoked: bool, expired: bool, disabled: bool, invalid: bool| {
            if revoked { "Revocada" } else if expired { "Caducada" } else if disabled { "Deshabilitada" } else if invalid { "Inválida" } else { "Vigente" }
        };
        let status_code = |revoked: bool, expired: bool, disabled: bool, invalid: bool| {
            if revoked { "revoked" } else if expired { "expired" } else if disabled { "disabled" } else if invalid { "invalid" } else { "valid" }
        };
        let subkeys = key.subkeys.iter().map(|subkey| json!({
            "fingerprint": subkey.fingerprint,
            "algorithm": subkey.algorithm,
            "expiresAt": subkey.expires_at,
            "capabilities": capabilities(subkey.can_encrypt, subkey.can_sign, subkey.can_certify, subkey.can_authenticate),
            "status": status(subkey.revoked, subkey.expired, subkey.disabled, subkey.invalid),
            "statusCode": status_code(subkey.revoked, subkey.expired, subkey.disabled, subkey.invalid),
        })).collect::<Vec<_>>();
        let identities = key.user_ids.iter().map(|uid| {
            let (code, label) = validity(Some(uid));
            json!({
            "id": uid.id,
            "revoked": uid.revoked,
            "invalid": uid.invalid,
            "validity": label,
            "validityCode": code,
        })}).collect::<Vec<_>>();
        let signatures = key.user_ids.iter().flat_map(|uid| uid.signatures.iter().filter_map(|sig| {
            // Self-certifications belong to the key structure, not third-party endorsements.
            if !sig.signer_key_id.is_empty() && key.fingerprint.ends_with(&sig.signer_key_id) { return None; }
            Some(json!({
                "identity": uid.id,
                "signer": sig.signer,
                "keyId": sig.signer_key_id,
                "status": if sig.revoked { "Revocación registrada" } else if sig.expired { "Caducada" } else if sig.invalid { "Inválida" } else { "Certificación registrada" },
            }))
        })).collect::<Vec<_>>();
        QString::from(json!({
            "id": key.fingerprint,
            "title": title,
            "email": identity.and_then(|uid| uid.email.as_deref()).unwrap_or(""),
            "hasSecret": key.has_secret,
            "validity": validity_label,
            "validityCode": validity_code,
            "status": status(key.revoked, key.expired, key.disabled, key.invalid),
            "statusCode": status_code(key.revoked, key.expired, key.disabled, key.invalid),
            "identities": identities,
            "subkeys": subkeys,
            "signatures": signatures,
        }).to_string())
    }).collect()
}

impl qobject::KeyringController {
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
}

fn controller_done(mut controller: Pin<&mut qobject::KeyringController>, status: String) {
    controller.as_mut().set_status(QString::from(status));
    controller.set_busy(false);
}
