#include "native/desktop_integration.h"
#include "app_identity.h"

#include <KAboutData>
#include <KActionCollection>
#include <KShortcutsDialog>
#include <KStandardShortcut>
#include <QAction>
#include <QApplication>
#include <QIcon>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLibraryInfo>
#include <QLocale>
#include <QTranslator>
#include <QQuickWindow>
#include <QTimer>

namespace {
QString translated(const char *source)
{
    return QCoreApplication::translate("Shortcuts", source);
}

KActionCollection *actions()
{
    static auto collection = [] {
        auto *result = new KActionCollection(qApp);
        result->setComponentName(QString::fromUtf8(kryptopass::identity::desktop_file_id));
        const auto add = [result](const char *id, const char *label, const QList<QKeySequence> &keys) {
            auto *action = result->addAction(QString::fromLatin1(id));
            action->setText(translated(label));
            KActionCollection::setDefaultShortcuts(action, keys);
        };
        add("newEntry", QT_TRANSLATE_NOOP("Shortcuts", "Nuevo elemento…"), KStandardShortcut::shortcut(KStandardShortcut::New));
        add("editEntry", QT_TRANSLATE_NOOP("Shortcuts", "Editar elemento…"), {QKeySequence(Qt::Key_F2)});
        add("trashEntry", QT_TRANSLATE_NOOP("Shortcuts", "Mover elemento a la papelera"), KStandardShortcut::shortcut(KStandardShortcut::DeleteFile));
        add("newFolder", QT_TRANSLATE_NOOP("Shortcuts", "Nueva carpeta…"), {QKeySequence(QStringLiteral("Ctrl+Shift+N"))});
        add("refreshKeys", QT_TRANSLATE_NOOP("Shortcuts", "Recargar llavero"), KStandardShortcut::shortcut(KStandardShortcut::Reload));
        add("createKey", QT_TRANSLATE_NOOP("Shortcuts", "Crear clave OpenPGP…"), {QKeySequence(QStringLiteral("Ctrl+Shift+K"))});
        add("importKey", QT_TRANSLATE_NOOP("Shortcuts", "Importar clave OpenPGP…"), {QKeySequence(QStringLiteral("Ctrl+Shift+I"))});
        add("find", QT_TRANSLATE_NOOP("Shortcuts", "Buscar"), KStandardShortcut::shortcut(KStandardShortcut::Find));
        add("quit", QT_TRANSLATE_NOOP("Shortcuts", "Salir"), KStandardShortcut::shortcut(KStandardShortcut::Quit));
        add("lockVault", QT_TRANSLATE_NOOP("Shortcuts", "Bloquear o desbloquear bóveda"), {});
        add("copyFingerprint", QT_TRANSLATE_NOOP("Shortcuts", "Copiar huella"), {});
        add("folderUp", QT_TRANSLATE_NOOP("Shortcuts", "Subir carpeta"), {QKeySequence(QStringLiteral("Ctrl+Alt+Up"))});
        add("folderDown", QT_TRANSLATE_NOOP("Shortcuts", "Bajar carpeta"), {QKeySequence(QStringLiteral("Ctrl+Alt+Down"))});
        add("moveFolder", QT_TRANSLATE_NOOP("Shortcuts", "Mover carpeta…"), {});
        result->readSettings();
        return result;
    }();
    return collection;
}
}

namespace kryptopass::desktop::native {
void initialize_desktop()
{
    using namespace kryptopass::identity;
    const auto install = [](const QString &name, const QString &directory) {
        auto *translator = new QTranslator(qApp);
        if (translator->load(QLocale(), name, QStringLiteral("_"), directory)
            || (directory == QStringLiteral(":/i18n") && translator->load(QLocale(QStringLiteral("es")), name, QStringLiteral("_"), directory))) {
            QCoreApplication::installTranslator(translator);
        } else {
            delete translator;
        }
    };
    install(QStringLiteral("qt"), QLibraryInfo::path(QLibraryInfo::TranslationsPath));
    install(QString::fromUtf8(desktop_file_id), QStringLiteral(":/i18n"));

    KAboutData about(QString::fromUtf8(desktop_file_id), QString::fromUtf8(display_name), QString::fromUtf8(version));
    about.setShortDescription(QCoreApplication::translate("Metadata", description));
    about.setLicense(KAboutLicense::byKeyword(QString::fromUtf8(license)));
    about.setCopyrightStatement(QString::fromUtf8(copyright));
    about.setHomepage(QString::fromUtf8(homepage));
    about.setBugAddress(QByteArray(repository) + "/issues");
    about.setOrganizationDomain(QByteArray(organization_domain));
    about.setDesktopFileName(QString::fromUtf8(desktop_file_id));
    about.addAuthor(QString::fromUtf8(authors));
    KAboutData::setApplicationData(about);
    QApplication::setWindowIcon(QIcon::fromTheme(QString::fromUtf8(desktop_file_id), QIcon(QStringLiteral(":/") + QString::fromUtf8(desktop_file_id).replace(QLatin1Char('.'), QLatin1Char('/')) + QStringLiteral("/icon.svg"))));
}

QString translate_ui(const QString &source)
{
    const auto bytes = source.toUtf8();
    return QCoreApplication::translate("Backend", bytes.constData());
}

QString translate_plural(const QString &source, int count)
{
    const auto bytes = source.toUtf8();
    return QCoreApplication::translate("Backend", bytes.constData(), nullptr, count);
}

QString configured_shortcuts()
{
    QJsonObject object;
    for (auto *action : actions()->actions()) {
        QJsonArray sequences;
        for (const auto &key : action->shortcuts()) {
            sequences.append(key.toString(QKeySequence::PortableText));
        }
        object.insert(action->objectName(), sequences);
    }
    return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

QString configure_shortcuts()
{
    KShortcutsDialog dialog(KShortcutsEditor::WidgetAction, KShortcutsEditor::LetterShortcutsDisallowed, QApplication::activeWindow());
    dialog.addCollection(actions(), QString::fromUtf8(kryptopass::identity::display_name));
    dialog.configure(true);
    return configured_shortcuts();
}
}

namespace kryptopass::desktop::native {
void capture_screenshot(const QString &path)
{
    QTimer::singleShot(2000, qApp, [path] {
        for (auto *window : QGuiApplication::topLevelWindows()) {
            if (auto *quick = qobject_cast<QQuickWindow *>(window); quick && quick->isVisible()) {
                QCoreApplication::exit(quick->grabWindow().save(path) ? 0 : 1);
                return;
            }
        }
        QCoreApplication::exit(1);
    });
}
}
