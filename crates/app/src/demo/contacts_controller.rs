//! Explicit UI demonstration controller; no real vault or operating-system adapter.
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }
    extern "RustQt" {
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
        #[qproperty(bool, busy, cxx_name = "busy", READ, NOTIFY)]
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
    fn publish_recipient_candidates(mut self: Pin<&mut Self>, value: QStringList) {
        if self.recipient_candidates() != &value {
            self.as_mut().rust_mut().recipient_candidates = value;
            self.recipient_candidates_changed();
        }
    }
    fn publish_error_message(mut self: Pin<&mut Self>, value: QString) {
        if self.error_message() != &value {
            self.as_mut().rust_mut().error_message = value;
            self.error_message_changed();
        }
    }
    pub fn refresh_contacts(self: Pin<&mut Self>) {
        // No system address book is connected in demo mode.
        let _ = self;
    }
    pub fn open_contacts(self: Pin<&mut Self>) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
    pub fn new_contact(self: Pin<&mut Self>) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
    pub fn edit_contact(self: Pin<&mut Self>, _uid: &QString) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
    pub fn choose_contact_for_key(self: Pin<&mut Self>, _fingerprint: &QString) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
    pub fn choose_recipient(self: Pin<&mut Self>, _existing_fingerprints_json: &QString) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
    pub fn select_recipient(self: Pin<&mut Self>, _fingerprint: &QString) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
    pub fn clear_recipient_selection(self: Pin<&mut Self>) {
        self.publish_recipient_candidates(QStringList::default());
    }
    pub fn receive_contacts(
        self: Pin<&mut Self>,
        _records: &QStringList,
        _error_message: &QString,
    ) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
    pub fn receive_operation(self: Pin<&mut Self>, _success: bool, _error_message: &QString) {
        self.publish_error_message(QString::from(super::UNAVAILABLE));
    }
}
