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
        include!("cxx-qt-lib/qurl.h");
        include!("cxx-qt-lib/core/qobjectmutptr.h");
        include!("native/sensitive_clipboard.h");
        include!("native/kde_contacts.h");
        include!("native/online_services.h");
        include!("native/entry_icons.h");
        include!("native/desktop_integration.h");
        include!("native/session_lock.h");
        include!("native/file_manager.h");
        include!("cxx-qt-lib/qqmlengine.h");
        type QQmlEngine = cxx_qt_lib::QQmlEngine;

        #[namespace = "kryptopass::desktop::native"]
        fn normalize_entry_icon(bytes: &[u8]) -> Vec<u8>;
        #[namespace = "kryptopass::desktop::native"]
        fn valid_entry_icon(bytes: &[u8]) -> bool;
        #[namespace = "kryptopass::desktop::native"]
        fn install_entry_icon_provider(engine: Pin<&mut QQmlEngine>, app_id: &QString);
        #[namespace = "kryptopass::desktop::native"]
        fn prefetch_favicons(hosts: &QStringList);

        type QString = cxx_qt_lib::QString;
        #[namespace = "kryptopass::desktop::native"]
        fn initialize_desktop();
        #[namespace = "kryptopass::desktop::native"]
        fn capture_screenshot(path: &QString);
        #[namespace = "kryptopass::desktop::native"]
        fn translate_ui(source: &QString) -> QString;
        #[namespace = "kryptopass::desktop::native"]
        fn translate_plural(source: &QString, count: i32) -> QString;
        #[namespace = "kryptopass::desktop::native"]
        fn desktop_actions() -> QString;
        #[namespace = "kryptopass::desktop::native"]
        fn set_action_enabled(id: &QString, enabled: bool);
        #[namespace = "kryptopass::desktop::native"]
        fn show_about_application_dialog();
        #[namespace = "kryptopass::desktop::native"]
        fn show_about_kde_dialog();
        #[namespace = "kryptopass::desktop::native"]
        fn show_handbook();
        #[namespace = "kryptopass::desktop::native"]
        fn open_bug_report() -> bool;
        #[namespace = "kryptopass::desktop::native"]
        fn configure_shortcuts() -> QString;
        type QStringList = cxx_qt_lib::QStringList;
        type QObjectMutPtr = cxx_qt_lib::QObjectMutPtr;
        type QUrl = cxx_qt_lib::QUrl;

        #[namespace = "kryptopass::desktop::native"]
        unsafe fn show_in_folder(receiver: QObjectMutPtr, url: &QUrl);

        #[namespace = "kryptopass::desktop::native"]
        unsafe fn install_session_lock_monitor(receiver: QObjectMutPtr);
        #[namespace = "kryptopass::desktop::native"]
        fn idle_lock_seconds() -> i32;
        #[namespace = "kryptopass::desktop::native"]
        fn set_idle_lock_seconds(seconds: i32);

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
