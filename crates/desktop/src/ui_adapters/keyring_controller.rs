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

#[cfg(test)]
mod tests {
    use super::key_records;
    use kryptopass_core::keyring::{CertificationInfo, SubkeyInfo};
    use kryptopass_core::{KeyInfo, KeyValidity, UserIdInfo};

    fn key_with_valid_subkey() -> KeyInfo {
        KeyInfo {
            fingerprint: "0123456789ABCDEF0123456789ABCDEF01234567".to_owned(),
            user_ids: Vec::new(),
            subkeys: vec![SubkeyInfo {
                fingerprint: "89ABCDEF0123456789ABCDEF0123456789ABCDEF".to_owned(),
                algorithm: "cv25519".to_owned(),
                expires_at: None,
                can_encrypt: true,
                can_sign: false,
                can_certify: false,
                can_authenticate: false,
                revoked: false,
                expired: false,
                disabled: false,
                invalid: false,
            }],
            has_secret: false,
            can_encrypt: true,
            can_sign: false,
            can_certify: false,
            can_authenticate: false,
            revoked: false,
            expired: false,
            disabled: false,
            invalid: false,
        }
    }

    fn status_cases() -> [([bool; 4], &'static str, &'static str); 16] {
        // Flags are ordered revoked, expired, disabled, invalid.
        [
            ([false, false, false, false], "valid", "Vigente"),
            ([true, false, false, false], "revoked", "Revocada"),
            ([false, true, false, false], "expired", "Caducada"),
            ([false, false, true, false], "disabled", "Deshabilitada"),
            ([false, false, false, true], "invalid", "Inválida"),
            ([true, true, false, false], "revoked", "Revocada"),
            ([true, false, true, false], "revoked", "Revocada"),
            ([true, false, false, true], "revoked", "Revocada"),
            ([false, true, true, false], "expired", "Caducada"),
            ([false, true, false, true], "expired", "Caducada"),
            ([false, false, true, true], "disabled", "Deshabilitada"),
            ([true, true, true, false], "revoked", "Revocada"),
            ([true, true, false, true], "revoked", "Revocada"),
            ([true, false, true, true], "revoked", "Revocada"),
            ([false, true, true, true], "expired", "Caducada"),
            ([true, true, true, true], "revoked", "Revocada"),
        ]
    }

    fn serialized_key_record(key: KeyInfo) -> serde_json::Value {
        let records = key_records(&[key]);
        assert_eq!(records.len(), 1);
        let Some(serialized_record) = records.iter().next() else {
            panic!("expected one serialized key record");
        };
        let Ok(record) = serde_json::from_str(&serialized_record.to_string()) else {
            panic!("key record must contain valid JSON");
        };
        record
    }

    #[test]
    fn key_records_serializes_key_status_codes_and_flag_precedence() {
        for (flags, expected_code, expected_status) in status_cases() {
            let mut key = key_with_valid_subkey();
            [key.revoked, key.expired, key.disabled, key.invalid] = flags;

            let record = serialized_key_record(key);

            assert_eq!(record["statusCode"], expected_code, "key flags: {flags:?}");
            assert_eq!(record["status"], expected_status, "key flags: {flags:?}");
            assert_eq!(record["subkeys"][0]["statusCode"], "valid");
            assert_eq!(record["subkeys"][0]["status"], "Vigente");
        }
    }

    #[test]
    fn key_records_serializes_subkey_status_codes_and_flag_precedence() {
        for (flags, expected_code, expected_status) in status_cases() {
            let mut key = key_with_valid_subkey();
            let subkey = &mut key.subkeys[0];
            [
                subkey.revoked,
                subkey.expired,
                subkey.disabled,
                subkey.invalid,
            ] = flags;

            let record = serialized_key_record(key);
            let subkey = &record["subkeys"][0];

            assert_eq!(
                subkey["statusCode"], expected_code,
                "subkey flags: {flags:?}"
            );
            assert_eq!(subkey["status"], expected_status, "subkey flags: {flags:?}");
            assert_eq!(record["statusCode"], "valid");
            assert_eq!(record["status"], "Vigente");
        }
    }

    #[test]
    fn key_records_preserves_public_metadata_and_excludes_self_certifications() {
        let fingerprint = "0123456789ABCDEF0123456789ABCDEF01234567";
        let identity = "Ada <ada@example.org>";
        let subkey_fingerprint = "89ABCDEF0123456789ABCDEF0123456789ABCDEF";
        let third_party_key_id = "FEDCBA9876543210";
        let key = KeyInfo {
            fingerprint: fingerprint.to_owned(),
            user_ids: vec![UserIdInfo {
                id: identity.to_owned(),
                name: Some("Ada".to_owned()),
                email: Some("ada@example.org".to_owned()),
                comment: None,
                validity: KeyValidity::Full,
                revoked: false,
                invalid: false,
                signatures: vec![
                    CertificationInfo {
                        signer_key_id: "89ABCDEF01234567".to_owned(),
                        signer: identity.to_owned(),
                        revoked: false,
                        expired: false,
                        invalid: false,
                    },
                    CertificationInfo {
                        signer_key_id: third_party_key_id.to_owned(),
                        signer: "Grace <grace@example.org>".to_owned(),
                        revoked: false,
                        expired: false,
                        invalid: false,
                    },
                ],
            }],
            subkeys: vec![SubkeyInfo {
                fingerprint: subkey_fingerprint.to_owned(),
                algorithm: "cv25519".to_owned(),
                expires_at: Some(1_900_000_000),
                can_encrypt: true,
                can_sign: false,
                can_certify: false,
                can_authenticate: false,
                revoked: false,
                expired: false,
                disabled: false,
                invalid: false,
            }],
            has_secret: true,
            can_encrypt: true,
            can_sign: true,
            can_certify: true,
            can_authenticate: false,
            revoked: false,
            expired: false,
            disabled: false,
            invalid: false,
        };

        let records = key_records(&[key]);
        assert_eq!(records.len(), 1);
        let Some(serialized_record) = records.iter().next() else {
            panic!("expected one serialized key record");
        };
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&serialized_record.to_string())
        else {
            panic!("key record must contain valid JSON");
        };
        assert_eq!(record["id"], fingerprint);
        assert_eq!(record["title"], "Ada");
        assert_eq!(record["email"], "ada@example.org");
        assert_eq!(record["hasSecret"], true);
        assert_eq!(record["identities"][0]["id"], identity);
        assert_eq!(record["subkeys"][0]["fingerprint"], subkey_fingerprint);
        assert_eq!(record["subkeys"][0]["algorithm"], "cv25519");
        assert_eq!(record["subkeys"][0]["expiresAt"], 1_900_000_000_u64);
        assert_eq!(
            record["subkeys"][0]["capabilities"],
            serde_json::json!(["Cifrar"])
        );
        let Some(signatures) = record["signatures"].as_array() else {
            panic!("key record must contain a certification array");
        };
        assert_eq!(signatures.len(), 1);
        assert_eq!(signatures[0]["identity"], identity);
        assert_eq!(signatures[0]["keyId"], third_party_key_id);
        assert_eq!(signatures[0]["signer"], "Grace <grace@example.org>");
    }

    fn validity_cases() -> [(KeyValidity, &'static str, &'static str); 6] {
        [
            (KeyValidity::Unknown, "unknown", "Sin verificar"),
            (KeyValidity::Undefined, "undefined", "Sin verificar"),
            (KeyValidity::Never, "never", "No válida según GnuPG"),
            (
                KeyValidity::Marginal,
                "marginal",
                "Validez marginal según GnuPG",
            ),
            (KeyValidity::Full, "full", "Validez plena según GnuPG"),
            (
                KeyValidity::Ultimate,
                "ultimate",
                "Validez absoluta según GnuPG",
            ),
        ]
    }

    fn user_id_with_validity(
        name: &str,
        validity: KeyValidity,
        revoked: bool,
        invalid: bool,
    ) -> UserIdInfo {
        UserIdInfo {
            id: format!("{name} <{name}@example.org>"),
            name: Some(name.to_owned()),
            email: Some(format!("{name}@example.org")),
            comment: None,
            validity,
            revoked,
            invalid,
            signatures: Vec::new(),
        }
    }

    #[test]
    fn key_records_serializes_validity_codes_and_labels_for_every_uid() {
        let mut key = key_with_valid_subkey();
        key.user_ids = validity_cases()
            .into_iter()
            .map(|(validity, code, _)| user_id_with_validity(code, validity, false, false))
            .collect();

        let record = serialized_key_record(key);
        let Some(identities) = record["identities"].as_array() else {
            panic!("key record must contain an identity array");
        };
        assert_eq!(identities.len(), validity_cases().len());
        for (identity, (validity, code, label)) in identities.iter().zip(validity_cases()) {
            assert_eq!(identity["id"], format!("{code} <{code}@example.org>"));
            assert_eq!(identity["validityCode"], code, "UID validity: {validity:?}");
            assert_eq!(identity["validity"], label, "UID validity: {validity:?}");
        }
    }

    #[test]
    fn key_records_uses_first_unflagged_uid_validity_for_every_enum_value() {
        for (validity, code, label) in validity_cases() {
            let mut key = key_with_valid_subkey();
            key.user_ids = vec![
                user_id_with_validity("revoked", KeyValidity::Ultimate, true, false),
                user_id_with_validity("invalid", KeyValidity::Ultimate, false, true),
                user_id_with_validity("selected", validity, false, false),
                user_id_with_validity("later", KeyValidity::Ultimate, false, false),
            ];

            let record = serialized_key_record(key);
            let selected = &record["identities"][2];

            assert_eq!(record["title"], "selected");
            assert_eq!(record["email"], "selected@example.org");
            assert_eq!(selected["validityCode"], code, "UID validity: {validity:?}");
            assert_eq!(selected["validity"], label, "UID validity: {validity:?}");
            assert_eq!(record["validityCode"], code, "UID validity: {validity:?}");
            assert_eq!(record["validity"], label, "UID validity: {validity:?}");
            assert_eq!(record["validityCode"], selected["validityCode"]);
            assert_eq!(record["validity"], selected["validity"]);
        }
    }

    #[test]
    fn key_records_uid_flags_override_every_validity_with_revoked_precedence() {
        for (validity, _, _) in validity_cases() {
            for (flags, code, label) in [
                ([true, false], "revoked", "Identidad revocada"),
                ([false, true], "invalid", "Identidad inválida"),
                ([true, true], "revoked", "Identidad revocada"),
            ] {
                let [revoked, invalid] = flags;
                let mut key = key_with_valid_subkey();
                key.user_ids = vec![user_id_with_validity(
                    "selected", validity, revoked, invalid,
                )];

                let record = serialized_key_record(key);
                let identity = &record["identities"][0];

                assert_eq!(identity["revoked"], revoked);
                assert_eq!(identity["invalid"], invalid);
                assert_eq!(
                    identity["validityCode"], code,
                    "UID validity: {validity:?}, flags: {flags:?}"
                );
                assert_eq!(
                    identity["validity"], label,
                    "UID validity: {validity:?}, flags: {flags:?}"
                );
                assert_eq!(
                    record["validityCode"], code,
                    "UID validity: {validity:?}, flags: {flags:?}"
                );
                assert_eq!(
                    record["validity"], label,
                    "UID validity: {validity:?}, flags: {flags:?}"
                );
                assert_eq!(record["validityCode"], identity["validityCode"]);
                assert_eq!(record["validity"], identity["validity"]);
            }
        }
    }

    #[test]
    fn key_records_uses_first_uid_validity_when_all_uids_are_flagged() {
        for (flags, code, label) in [
            ([true, false], "revoked", "Identidad revocada"),
            ([false, true], "invalid", "Identidad inválida"),
            ([true, true], "revoked", "Identidad revocada"),
        ] {
            let [revoked, invalid] = flags;
            let mut key = key_with_valid_subkey();
            key.user_ids = vec![
                user_id_with_validity("first", KeyValidity::Unknown, revoked, invalid),
                user_id_with_validity("later-revoked", KeyValidity::Ultimate, true, false),
                user_id_with_validity("later-invalid", KeyValidity::Full, false, true),
            ];

            let record = serialized_key_record(key);
            let selected = &record["identities"][0];

            assert_eq!(record["title"], "first");
            assert_eq!(record["email"], "first@example.org");
            assert_eq!(selected["validityCode"], code, "first UID flags: {flags:?}");
            assert_eq!(selected["validity"], label, "first UID flags: {flags:?}");
            assert_eq!(record["validityCode"], code, "first UID flags: {flags:?}");
            assert_eq!(record["validity"], label, "first UID flags: {flags:?}");
            assert_eq!(record["validityCode"], selected["validityCode"]);
            assert_eq!(record["validity"], selected["validity"]);
        }
    }

    #[test]
    fn key_records_without_uids_serializes_unknown_validity() {
        let record = serialized_key_record(key_with_valid_subkey());

        assert_eq!(record["identities"], serde_json::json!([]));
        assert_eq!(record["validityCode"], "unknown");
        assert_eq!(record["validity"], "Sin verificar");
    }
}
