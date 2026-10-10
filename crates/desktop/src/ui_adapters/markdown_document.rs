use crate::markdown::{Document, Theme, allowed_link};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::pin::Pin;
use zeroize::Zeroizing;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qstringlist.h");
        type QString = cxx_qt_lib::QString;
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        /// Disposable presentation document, never a persistence or crypto service.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, html, READ, NOTIFY)]
        #[qproperty(QString, error, READ, NOTIFY)]
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
        fn load(self: Pin<&mut Self>, source: &QString);
        #[qinvokable]
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

        /// A request only: the consumer must explicitly choose whether to open it.
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
    document: Document,
}

impl qobject::MarkdownDocument {
    fn set_html(mut self: Pin<&mut Self>, value: QString) {
        if self.html() != &value {
            self.as_mut().rust_mut().html = value;
            self.html_changed();
        }
    }

    fn set_error(mut self: Pin<&mut Self>, value: QString) {
        if self.error() != &value {
            self.as_mut().rust_mut().error = value;
            self.error_changed();
        }
    }

    fn set_spoiler_actions(mut self: Pin<&mut Self>, value: QStringList) {
        if self.spoiler_actions() != &value {
            self.as_mut().rust_mut().spoiler_actions = value;
            self.spoiler_actions_changed();
        }
    }

    fn set_revealed_actions(mut self: Pin<&mut Self>, value: QStringList) {
        if self.revealed_actions() != &value {
            self.as_mut().rust_mut().revealed_actions = value;
            self.revealed_actions_changed();
        }
    }

    pub fn load(mut self: Pin<&mut Self>, source: &QString) {
        self.as_mut().reset_output();
        let source = Zeroizing::new(source.to_string());
        match self.as_mut().rust_mut().document.load(&source) {
            Ok(()) => self.publish(),
            Err(error) => self.set_error(QString::from(error)),
        }
    }

    pub fn clear(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().document.clear();
        self.reset_output();
    }

    pub fn hide_all(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().document.hide_all();
        self.publish();
    }

    pub fn activate_link(mut self: Pin<&mut Self>, link: &QString) {
        let value = link.to_string();
        if self.as_mut().rust_mut().document.toggle(&value) {
            self.publish();
        } else if allowed_link(&value) {
            self.link_requested(link);
        }
    }

    pub fn set_theme(
        mut self: Pin<&mut Self>,
        highlight_background: &QString,
        highlight_foreground: &QString,
        link: &QString,
        code_background: &QString,
        border: &QString,
    ) {
        let colors = [
            highlight_background,
            highlight_foreground,
            link,
            code_background,
            border,
        ]
        .map(ToString::to_string);
        if let Some(theme) = Theme::from_colors(colors.each_ref().map(String::as_str)) {
            self.as_mut().rust_mut().document.set_theme(theme);
            self.publish();
        }
    }

    fn reset_output(mut self: Pin<&mut Self>) {
        self.as_mut().set_html(QString::default());
        self.as_mut().set_spoiler_actions(QStringList::default());
        self.as_mut().set_revealed_actions(QStringList::default());
        self.set_error(QString::default());
    }

    fn publish(mut self: Pin<&mut Self>) {
        match self.as_mut().rust_mut().document.render() {
            Ok(rendered) => {
                self.as_mut()
                    .set_html(QString::from(rendered.html.as_str()));
                self.as_mut().set_spoiler_actions(
                    rendered
                        .actions
                        .iter()
                        .map(|action| QString::from(action.as_str()))
                        .collect(),
                );
                self.set_revealed_actions(
                    rendered
                        .revealed_actions
                        .iter()
                        .map(|action| QString::from(action.as_str()))
                        .collect(),
                );
            }
            Err(_) => {
                self.as_mut().clear();
                self.set_error(QString::from("No se pudo preparar el documento Markdown."));
            }
        }
    }
}
