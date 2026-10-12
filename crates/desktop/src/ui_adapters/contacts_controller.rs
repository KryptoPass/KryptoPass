use std::pin::Pin;

use cxx_qt::{CxxQtType, casting::Upcast};
use cxx_qt_lib::{QObjectMutPtr, QString, QStringList};

use super::native_ffi::ffi as native_ffi;

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
        /// Projects KDE Contacts data and native address-book actions into QML.
        #[qobject]
        #[qml_element]
        #[qproperty(
            QStringList,
            contact_records,
            cxx_name = "contactRecords",
            READ,
            NOTIFY
        )]
        #[qproperty(
            QStringList,
            recipient_candidates,
            cxx_name = "recipientCandidates",
            READ,
            NOTIFY
        )]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(QString, error_message, cxx_name = "errorMessage", READ, NOTIFY)]
        type ContactsController = super::ContactsControllerRust;

        #[qinvokable]
        #[cxx_name = "refreshContacts"]
        fn refresh_contacts(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "openContacts"]
        fn open_contacts(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "newContact"]
        fn new_contact(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "editContact"]
        fn edit_contact(self: Pin<&mut Self>, uid: &QString);
        #[qinvokable]
        #[cxx_name = "chooseContactForKey"]
        fn choose_contact_for_key(self: Pin<&mut Self>, fingerprint: &QString);
        #[qinvokable]
        #[cxx_name = "chooseRecipient"]
        fn choose_recipient(self: Pin<&mut Self>, existing_fingerprints_json: &QString);
        #[qinvokable]
        #[cxx_name = "selectRecipient"]
        fn select_recipient(self: Pin<&mut Self>, fingerprint: &QString);
        #[qinvokable]
        #[cxx_name = "clearRecipientSelection"]
        fn clear_recipient_selection(self: Pin<&mut Self>);

        /// Native callback entry points. They run on the Qt thread through invokeMethod.
        #[qinvokable]
        #[cxx_name = "receiveContacts"]
        fn receive_contacts(self: Pin<&mut Self>, records: &QStringList, error_message: &QString);
        #[qinvokable]
        #[cxx_name = "receiveOperation"]
        fn receive_operation(self: Pin<&mut Self>, success: bool, error_message: &QString);
        #[qsignal]
        #[cxx_name = "recipientSelected"]
        fn recipient_selected(self: Pin<&mut Self>, fingerprint: &QString);
    }
}

#[derive(Default)]
pub struct ContactsControllerRust {
    contact_records: QStringList,
    recipient_candidates: QStringList,
    busy: bool,
    error_message: QString,
}

impl qobject::ContactsController {
    pub fn refresh_contacts(mut self: Pin<&mut Self>) {
        if *self.busy() {
            return;
        }
        self.as_mut().set_busy(true);
        self.as_mut().set_error_message(QString::default());
        unsafe { native_ffi::refresh_contacts(receiver_ptr(self.as_mut())) };
    }

    pub fn open_contacts(mut self: Pin<&mut Self>) {
        let opened = unsafe { native_ffi::open_contacts() };
        if !opened {
            self.as_mut().set_error_message(QString::from(
                "No se pudo abrir KDE Contacts. Comprueba que KAddressBook está instalado.",
            ));
        }
    }

    pub fn new_contact(mut self: Pin<&mut Self>) {
        self.as_mut().begin_operation();
        unsafe {
            native_ffi::open_contact_editor(receiver_ptr(self.as_mut()), &QString::default())
        };
    }

    pub fn edit_contact(mut self: Pin<&mut Self>, uid: &QString) {
        if uid.is_empty() {
            self.as_mut()
                .set_error_message(QString::from("El contacto seleccionado no es válido."));
            return;
        }
        self.as_mut().begin_operation();
        unsafe { native_ffi::open_contact_editor(receiver_ptr(self.as_mut()), uid) };
    }

    pub fn choose_contact_for_key(mut self: Pin<&mut Self>, fingerprint: &QString) {
        if !is_full_fingerprint(&String::from(fingerprint)) {
            self.as_mut().set_error_message(QString::from(
                "Solo se puede vincular una huella OpenPGP completa.",
            ));
            return;
        }
        self.as_mut().begin_operation();
        let normalized = QString::from(String::from(fingerprint).to_ascii_uppercase());
        unsafe { native_ffi::choose_contact_for_key(receiver_ptr(self.as_mut()), &normalized) };
    }

    pub fn choose_recipient(mut self: Pin<&mut Self>, existing_fingerprints_json: &QString) {
        let fingerprints_json = String::from(existing_fingerprints_json);
        let fingerprints: Vec<String> = match serde_json::from_str(&fingerprints_json) {
            Ok(fingerprints) => fingerprints,
            Err(_) => {
                self.as_mut().set_error_message(QString::from(
                    "No se pudo leer la lista de huellas disponibles.",
                ));
                return;
            }
        };
        let allowed: std::collections::HashSet<String> = fingerprints
            .into_iter()
            .filter(|fingerprint| is_full_fingerprint(fingerprint))
            .map(|fingerprint| fingerprint.to_ascii_uppercase())
            .collect();
        let mut candidates = QStringList::default();
        let mut seen = std::collections::HashSet::new();
        for record in self.contact_records().iter() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&String::from(record)) else {
                continue;
            };

            let mut contact_fingerprints = Vec::new();
            if let Some(fingerprints) = value
                .get("openPgpFingerprints")
                .and_then(serde_json::Value::as_array)
            {
                contact_fingerprints.extend(
                    fingerprints
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned),
                );
            }
            if let Some(fingerprint) = value
                .get("openPgpFingerprint")
                .and_then(serde_json::Value::as_str)
            {
                contact_fingerprints.push(fingerprint.to_owned());
            }

            for fingerprint in contact_fingerprints {
                if !is_full_fingerprint(&fingerprint) {
                    continue;
                }
                let normalized = fingerprint.to_ascii_uppercase();
                if allowed.contains(&normalized) && seen.insert(normalized.clone()) {
                    candidates.append(QString::from(normalized));
                }
            }
        }
        if candidates.is_empty() {
            self.as_mut().set_error_message(QString::from(
                "Actualiza KDE Contacts. No hay contactos vinculados a las claves válidas disponibles.",
            ));
            return;
        }
        self.as_mut().set_error_message(QString::default());
        self.as_mut()
            .set_recipient_candidates(QStringList::default());
        self.as_mut().set_recipient_candidates(candidates);
    }

    pub fn select_recipient(mut self: Pin<&mut Self>, fingerprint: &QString) {
        let normalized = String::from(fingerprint).to_ascii_uppercase();
        if !self
            .recipient_candidates()
            .iter()
            .any(|candidate| String::from(candidate).eq_ignore_ascii_case(&normalized))
        {
            return;
        }
        let selected = QString::from(normalized);
        self.as_mut().recipient_selected(&selected);
        self.as_mut()
            .set_recipient_candidates(QStringList::default());
    }

    pub fn clear_recipient_selection(mut self: Pin<&mut Self>) {
        self.as_mut()
            .set_recipient_candidates(QStringList::default());
    }

    pub fn receive_contacts(
        mut self: Pin<&mut Self>,
        records: &QStringList,
        error_message: &QString,
    ) {
        self.as_mut().set_contact_records(records.clone());
        self.as_mut().set_busy(false);
        self.as_mut().set_error_message(error_message.clone());
    }

    pub fn receive_operation(mut self: Pin<&mut Self>, success: bool, error_message: &QString) {
        if success {
            self.as_mut().set_error_message(QString::default());
            self.as_mut().set_busy(false);
            self.as_mut().refresh_contacts();
        } else {
            self.as_mut().set_busy(false);
            self.as_mut().set_error_message(error_message.clone());
        }
    }

    fn begin_operation(mut self: Pin<&mut Self>) {
        self.as_mut().set_busy(true);
        self.as_mut().set_error_message(QString::default());
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

    fn set_contact_records(mut self: Pin<&mut Self>, value: QStringList) {
        if self.contact_records() != &value {
            self.as_mut().rust_mut().contact_records = value;
            self.contact_records_changed();
        }
    }

    fn set_recipient_candidates(mut self: Pin<&mut Self>, value: QStringList) {
        if self.recipient_candidates() != &value {
            self.as_mut().rust_mut().recipient_candidates = value;
            self.recipient_candidates_changed();
        }
    }
}

fn receiver_ptr(controller: Pin<&mut qobject::ContactsController>) -> QObjectMutPtr {
    let base: Pin<&mut cxx_qt::QObject> = controller.upcast_pin();
    let receiver = unsafe { Pin::get_unchecked_mut(base) };
    // The QObject remains alive while the corresponding Qt job or dialog is active.
    unsafe { QObjectMutPtr::from_raw(std::ptr::from_mut(receiver)) }
}

fn is_full_fingerprint(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
