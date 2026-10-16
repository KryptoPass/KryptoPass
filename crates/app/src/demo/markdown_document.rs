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
        #[qproperty(QString, html, cxx_name = "html", READ, NOTIFY)]
        #[qproperty(QString, error, cxx_name = "error", READ, NOTIFY)]
        #[qproperty(
            QStringList,
            spoiler_actions,
            cxx_name = "spoilerActions",
            READ,
            NOTIFY
        )]
        #[qproperty(
            QStringList,
            revealed_actions,
            cxx_name = "revealedActions",
            READ,
            NOTIFY
        )]
        type MarkdownDocument = super::MarkdownDocumentRust;
        #[qinvokable]
        #[cxx_name = "load"]
        fn load(self: Pin<&mut Self>, source: &QString);
        #[qinvokable]
        #[cxx_name = "clear"]
        fn clear(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "hideAll"]
        fn hide_all(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "activateLink"]
        fn activate_link(self: Pin<&mut Self>, link: &QString);
        #[qinvokable]
        #[cxx_name = "setTheme"]
        fn set_theme(
            self: Pin<&mut Self>,
            highlight_background: &QString,
            highlight_foreground: &QString,
            link: &QString,
            code_background: &QString,
            border: &QString,
        );
        #[qsignal]
        #[cxx_name = "linkRequested"]
        fn link_requested(self: Pin<&mut Self>, link: &QString);
    }
}

#[derive(Default)]
pub struct MarkdownDocumentRust {
    html: QString,
    error: QString,
    spoiler_actions: QStringList,
    revealed_actions: QStringList,
}

impl qobject::MarkdownDocument {
    fn publish_html(mut self: Pin<&mut Self>, value: QString) {
        if self.html() != &value {
            self.as_mut().rust_mut().html = value;
            self.html_changed();
        }
    }
    fn publish_error(mut self: Pin<&mut Self>, value: QString) {
        if self.error() != &value {
            self.as_mut().rust_mut().error = value;
            self.error_changed();
        }
    }
    pub fn load(mut self: Pin<&mut Self>, source: &QString) {
        let source = source.to_string();
        if source.len() > 256 * 1024 {
            self.publish_error(QString::from(
                "La nota supera el límite de la demostración.",
            ));
            return;
        }
        let escaped = source
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\n', "<br>");
        self.as_mut().publish_error(QString::default());
        self.publish_html(QString::from(format!("<p>{escaped}</p>")));
    }
    pub fn clear(mut self: Pin<&mut Self>) {
        self.as_mut().publish_html(QString::default());
        self.publish_error(QString::default());
    }
    pub fn hide_all(self: Pin<&mut Self>) {
        // Plain-text demo rendering has no spoiler actions.
        let _ = self;
    }
    pub fn activate_link(self: Pin<&mut Self>, link: &QString) {
        self.link_requested(link);
    }
    pub fn set_theme(
        self: Pin<&mut Self>,
        _highlight_background: &QString,
        _highlight_foreground: &QString,
        _link: &QString,
        _code_background: &QString,
        _border: &QString,
    ) {
        // Plain-text demo rendering inherits the QML label palette.
        let _ = self;
    }
}
