use crate::infrastructure::entry_icons::entry_icon_bytes;

// Shared native C++ functions used by the small Qt adapters.

#[cxx::bridge]
pub(crate) mod ffi {
    extern "Rust" {
        #[namespace = "kryptopass::desktop::native"]
        fn entry_icon_bytes(source: &str) -> Vec<u8>;
    }
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qstringlist.h");
        include!("cxx-qt-lib/core/qobjectmutptr.h");
        include!("native/sensitive_clipboard.h");
        include!("native/kde_contacts.h");
        include!("native/online_services.h");
        include!("native/entry_icons.h");
        include!("cxx-qt-lib/qqmlengine.h");
        type QQmlEngine = cxx_qt_lib::QQmlEngine;

        #[namespace = "kryptopass::desktop::native"]
        fn normalize_entry_icon(bytes: &[u8]) -> Vec<u8>;
        #[namespace = "kryptopass::desktop::native"]
        fn valid_entry_icon(bytes: &[u8]) -> bool;
        #[namespace = "kryptopass::desktop::native"]
        fn install_entry_icon_provider(engine: Pin<&mut QQmlEngine>, app_id: &QString);
        #[namespace = "kryptopass::desktop::native"]
        fn fetch_favicon(host: &QString) -> Vec<u8>;
        #[namespace = "kryptopass::desktop::native"]
        fn prefetch_favicons(hosts: &QString);

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
        unsafe fn online_settings() -> QString;
        #[namespace = "kryptopass::desktop::native"]
        unsafe fn save_online_settings(json: &QString) -> bool;
        #[namespace = "kryptopass::desktop::native"]
        unsafe fn open_online_url(url: &QString) -> bool;
        #[namespace = "kryptopass::desktop::native"]
        unsafe fn open_contacts() -> bool;
    }
}
