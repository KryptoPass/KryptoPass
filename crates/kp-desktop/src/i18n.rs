//! Translation at the boundary between backend messages and Qt presentation.
use cxx_kde_frameworks::ki18n::KLocalizedString;
use cxx_qt_lib::{QByteArray, QString};

/// Selects this application's gettext catalog. Call it once, before KAboutData is
/// registered, so its texts are translated too.
pub fn set_up_catalogs() {
    let domain = QByteArray::from(crate::app_info::APP.desktop_file_id);
    KLocalizedString::set_application_domain(&domain);
    // Development builds read the catalogs build.rs compiled; installed builds use the
    // standard locale directories that `ki18n_install(po)` fills.
    if cfg!(debug_assertions) {
        KLocalizedString::add_domain_locale_dir(&domain, &QString::from(env!("KP_LOCALE_DIR")));
    }
    // The backend still produces texts for people; they go through this catalog.
    kryptopass_backend::i18n::set_translator(text);
}

pub fn translate(source: &QString) -> QString {
    crate::ui_adapters::native_ffi::ffi::translate_ui(source)
}

pub fn text(source: &str) -> String {
    translate(&QString::from(source)).to_string()
}

pub use kryptopass_backend::i18n::format;

pub fn plural(source: &str, count: usize) -> QString {
    crate::ui_adapters::native_ffi::ffi::translate_plural(
        &QString::from(source),
        i32::try_from(count).unwrap_or(i32::MAX),
    )
}

#[cfg(test)]
#[path = "../tests/unit/i18n.rs"]
mod tests;
