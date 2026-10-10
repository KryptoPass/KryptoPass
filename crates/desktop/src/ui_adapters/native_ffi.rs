//! Shared native C++ functions used by the small Qt adapters.

#[cxx::bridge]
pub(crate) mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qstringlist.h");
        include!("cxx-qt-lib/core/qobjectmutptr.h");
        include!("native/sensitive_clipboard.h");
        include!("native/kde_contacts.h");

        type QString = cxx_qt_lib::QString;
        type QStringList = cxx_qt_lib::QStringList;
        type QObjectMutPtr = cxx_qt_lib::QObjectMutPtr;

        #[namespace = "kryptopass::desktop::native"]
        unsafe fn copy_sensitive(text: &QString, auto_clear_seconds: i32) -> bool;
        #[namespace = "kryptopass::desktop::native"]
        unsafe fn clear_owned();
        #[namespace = "kryptopass::desktop::native"]
        unsafe fn auto_clear_seconds() -> i32;
        #[namespace = "kryptopass::desktop::native"]
        unsafe fn set_auto_clear_seconds(seconds: i32);

        #[namespace = "kryptopass::desktop::native"]
        unsafe fn refresh_contacts(receiver: QObjectMutPtr);
        #[namespace = "kryptopass::desktop::native"]
        unsafe fn open_contact_editor(receiver: QObjectMutPtr, uid: &QString);
        #[namespace = "kryptopass::desktop::native"]
        unsafe fn choose_contact_for_key(receiver: QObjectMutPtr, fingerprint: &QString);
        #[namespace = "kryptopass::desktop::native"]
        unsafe fn open_contacts() -> bool;
    }
}
