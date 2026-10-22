#include "native/desktop_integration.h"
#include "app_identity.h"

#include <KAboutData>
#include <KActionCollection>
#include <KHelpMenu>
#include <KShortcutsDialog>
#include <KStandardActions>
#include <KStandardShortcut>
#include <QAction>
#include <QApplication>
#include <QDesktopServices>
#include <QIcon>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLibraryInfo>
#include <QLocale>
#include <QQuickWindow>
#include <QTimer>
#include <QTranslator>
#include <QUrl>

namespace
{
QString translated(const char *source)
{
    return QCoreApplication::translate("Shortcuts", source);
}

KActionCollection *actions()
{
    static auto collection = [] {
        auto *result = new KActionCollection(qApp);
        result->setComponentName(QString::fromUtf8(kryptopass::identity::desktop_file_id));
        const auto add = [result](const char *id, const char *label, const char *icon, const QList<QKeySequence> &keys) {
            auto *action = result->addAction(QString::fromLatin1(id));
            action->setText(translated(label));
            action->setIcon(QIcon::fromTheme(QString::fromLatin1(icon)));
            KActionCollection::setDefaultShortcuts(action, keys);
        };
        add("newEntry", QT_TRANSLATE_NOOP("Shortcuts", "Nuevo elemento…"), "document-new", KStandardShortcut::shortcut(KStandardShortcut::New));
        add("editEntry", QT_TRANSLATE_NOOP("Shortcuts", "Editar elemento…"), "document-edit", {QKeySequence(Qt::Key_F2)});
        add("trashEntry",
            QT_TRANSLATE_NOOP("Shortcuts", "Mover elemento a la papelera"),
            "user-trash",
            KStandardShortcut::shortcut(KStandardShortcut::DeleteFile));
        add("newFolder", QT_TRANSLATE_NOOP("Shortcuts", "Nueva carpeta…"), "folder-new", {QKeySequence(QStringLiteral("Ctrl+Shift+N"))});
        add("refreshKeys", QT_TRANSLATE_NOOP("Shortcuts", "Recargar llavero"), "view-refresh", KStandardShortcut::shortcut(KStandardShortcut::Reload));
        add("createKey", QT_TRANSLATE_NOOP("Shortcuts", "Crear clave OpenPGP…"), "document-new", {QKeySequence(QStringLiteral("Ctrl+Shift+K"))});
        add("importKey", QT_TRANSLATE_NOOP("Shortcuts", "Importar clave OpenPGP…"), "document-import", {QKeySequence(QStringLiteral("Ctrl+Shift+I"))});
        add("importVault", QT_TRANSLATE_NOOP("Shortcuts", "Importar desde otro gestor…"), "document-import", {});
        add("exportBackup", QT_TRANSLATE_NOOP("Shortcuts", "Crear copia de seguridad…"), "document-save", {});
        add("restoreBackup", QT_TRANSLATE_NOOP("Shortcuts", "Restaurar copia de seguridad…"), "document-revert", {});
        add("moveEntry", QT_TRANSLATE_NOOP("Shortcuts", "Mover a otra carpeta…"), "folder-move", {});
        add("showDevices", QT_TRANSLATE_NOOP("Shortcuts", "Dispositivos"), "network-connect", {});
        add("showVault", QT_TRANSLATE_NOOP("Shortcuts", "Carpetas"), "folder", {});
        add("showKeys", QT_TRANSLATE_NOOP("Shortcuts", "Claves"), "view-certificate", {});
        add("showAll", QT_TRANSLATE_NOOP("Shortcuts", "Todos los elementos"), "view-list-details", {});
        add("showFavorites", QT_TRANSLATE_NOOP("Shortcuts", "Favoritos"), "rating", {});
        add("showTrash", QT_TRANSLATE_NOOP("Shortcuts", "Papelera"), "user-trash", {});
        add("toggleFavorite", QT_TRANSLATE_NOOP("Shortcuts", "Favorito"), "rating", {});
        add("restoreEntry", QT_TRANSLATE_NOOP("Shortcuts", "Restaurar elemento"), "edit-undo", {});
        add("deleteEntry", QT_TRANSLATE_NOOP("Shortcuts", "Eliminar elemento definitivamente…"), "edit-delete", {});
        add("copyUsername", QT_TRANSLATE_NOOP("Shortcuts", "Copiar nombre de usuario"), "edit-copy", {QKeySequence(QStringLiteral("Ctrl+Shift+U"))});
        add("copyPassword", QT_TRANSLATE_NOOP("Shortcuts", "Copiar contraseña"), "edit-copy", {QKeySequence(QStringLiteral("Ctrl+Shift+P"))});
        add("copyTotp", QT_TRANSLATE_NOOP("Shortcuts", "Copiar código de verificación"), "edit-copy", {QKeySequence(QStringLiteral("Ctrl+Shift+T"))});
        add("showBackupInFolder", QT_TRANSLATE_NOOP("Shortcuts", "Mostrar en la carpeta"), "system-file-manager", {});
        add("lockVault", QT_TRANSLATE_NOOP("Shortcuts", "Bloquear o desbloquear bóveda"), "object-locked", {});
        add("copyFingerprint", QT_TRANSLATE_NOOP("Shortcuts", "Copiar huella"), "edit-copy", {});
        add("folderUp", QT_TRANSLATE_NOOP("Shortcuts", "Subir carpeta"), "go-up", {QKeySequence(QStringLiteral("Ctrl+Alt+Up"))});
        add("folderDown", QT_TRANSLATE_NOOP("Shortcuts", "Bajar carpeta"), "go-down", {QKeySequence(QStringLiteral("Ctrl+Alt+Down"))});
        add("moveFolder", QT_TRANSLATE_NOOP("Shortcuts", "Mover carpeta…"), "folder-move", {});
        const auto standard = [result](const char *id, KStandardActions::StandardAction kind) {
            auto *action = KStandardActions::create(kind, qApp, [] { }, qApp);
            result->addAction(QString::fromLatin1(id), action);
            KActionCollection::setDefaultShortcuts(action, action->shortcuts());
        };
        standard("find", KStandardActions::Find);
        standard("quit", KStandardActions::Quit);
        standard("configureShortcuts", KStandardActions::KeyBindings);
        standard("help", KStandardActions::HelpContents);
        standard("reportBug", KStandardActions::ReportBug);
        standard("aboutApp", KStandardActions::AboutApp);
        standard("aboutKde", KStandardActions::AboutKDE);
        result->readSettings();
        return result;
    }();
    return collection;
}
}

namespace kryptopass::desktop::native
{
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
    const auto icon = QIcon::fromTheme(
        QString::fromUtf8(desktop_file_id),
        QIcon(QStringLiteral(":/") + QString::fromUtf8(desktop_file_id).replace(QLatin1Char('.'), QLatin1Char('/')) + QStringLiteral("/icon.svg")));
    about.setProgramLogo(icon);
    KAboutData::setApplicationData(about);
    QApplication::setWindowIcon(icon);
}

QString about_data()
{
    const auto about = KAboutData::applicationData();
    QJsonArray authors;
    for (const auto &author : about.authors()) {
        authors.append(QJsonObject{{QStringLiteral("name"), author.name()}});
    }
    QJsonArray licenses;
    for (const auto &license : about.licenses()) {
        licenses.append(QJsonObject{
            {QStringLiteral("name"), license.name(KAboutLicense::FullName)},
            {QStringLiteral("text"), license.text()},
        });
    }
    const QJsonObject object{
        {QStringLiteral("displayName"), about.displayName()},
        {QStringLiteral("version"), about.version()},
        {QStringLiteral("shortDescription"), about.shortDescription()},
        {QStringLiteral("copyrightStatement"), about.copyrightStatement()},
        {QStringLiteral("homepage"), about.homepage()},
        {QStringLiteral("bugAddress"), about.bugAddress()},
        {QStringLiteral("programLogo"),
         QStringLiteral("qrc:/") + QString(about.desktopFileName()).replace(QLatin1Char('.'), QLatin1Char('/')) + QStringLiteral("/icon.svg")},
        {QStringLiteral("authors"), authors},
        {QStringLiteral("licenses"), licenses},
    };
    return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

void show_about_kde_dialog()
{
    static auto *help = new KHelpMenu;
    help->aboutKDE();
}

bool open_bug_report()
{
    return QDesktopServices::openUrl(QUrl(KAboutData::applicationData().bugAddress()));
}

void set_action_enabled(const QString &id, bool enabled)
{
    if (auto *action = actions()->action(id)) {
        action->setEnabled(enabled);
    }
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

QString desktop_actions()
{
    QJsonObject object;
    for (auto *action : actions()->actions()) {
        QJsonArray sequences;
        for (const auto &key : action->shortcuts()) {
            sequences.append(key.toString(QKeySequence::PortableText));
        }
        object.insert(action->objectName(),
                      QJsonObject{
                          {QStringLiteral("text"), action->text()},
                          {QStringLiteral("icon"), action->icon().name()},
                          {QStringLiteral("shortcuts"), sequences},
                      });
    }
    return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

QString configure_shortcuts()
{
    KShortcutsDialog dialog(KShortcutsEditor::WidgetAction, KShortcutsEditor::LetterShortcutsDisallowed, QApplication::activeWindow());
    dialog.addCollection(actions(), QString::fromUtf8(kryptopass::identity::display_name));
    dialog.configure(true);
    return desktop_actions();
}
}

namespace kryptopass::desktop::native
{
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
