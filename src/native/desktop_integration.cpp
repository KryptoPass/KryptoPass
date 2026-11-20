#include "native/desktop_integration.h"
#include "app_identity.h"

#include <KAboutData>
#include <KActionCollection>
#include <KHelpClient>
#include <KHelpMenu>
#include <KLazyLocalizedString>
#include <KLocalizedString>
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
#include <QRegularExpression>
#include <QTimer>
#include <QTranslator>
#include <QUrl>

namespace
{
KActionCollection *actions()
{
    static auto collection = [] {
        auto *result = new KActionCollection(qApp);
        result->setComponentName(QString::fromUtf8(kryptopass::identity::desktop_file_id));
        const auto add = [result](const char *id, const KLazyLocalizedString &label, const char *icon, const QList<QKeySequence> &keys) {
            auto *action = result->addAction(QString::fromLatin1(id));
            action->setText(label.toString());
            action->setIcon(QIcon::fromTheme(QString::fromLatin1(icon)));
            KActionCollection::setDefaultShortcuts(action, keys);
        };
        add("newEntry", kli18nc("@action", "New Item…"), "document-new", KStandardShortcut::shortcut(KStandardShortcut::New));
        add("editEntry", kli18nc("@action", "Edit Item…"), "document-edit", {QKeySequence(Qt::Key_F2)});
        add("trashEntry", kli18nc("@action", "Move Item to Trash"), "user-trash", KStandardShortcut::shortcut(KStandardShortcut::DeleteFile));
        add("newFolder", kli18nc("@action", "New Folder…"), "folder-new", {QKeySequence(QStringLiteral("Ctrl+Shift+N"))});
        add("refreshKeys", kli18nc("@action", "Reload Keyring"), "view-refresh", KStandardShortcut::shortcut(KStandardShortcut::Reload));
        add("createKey", kli18nc("@action", "Add Identity…"), "document-new", {QKeySequence(QStringLiteral("Ctrl+Shift+K"))});
        add("importKey", kli18nc("@action", "Import OpenPGP Key…"), "document-import", {QKeySequence(QStringLiteral("Ctrl+Shift+I"))});
        add("importVault", kli18nc("@action", "Import from Another Password Manager…"), "document-import", {});
        add("exportBackup", kli18nc("@action", "Export Backup…"), "document-save", {});
        add("restoreBackup", kli18nc("@action", "Restore Backup…"), "document-revert", {});
        add("moveEntry", kli18nc("@action", "Move to Another Folder…"), "folder-move", {});
        add("showDevices", kli18nc("@action", "Devices"), "network-connect", {});
        add("showVault", kli18nc("@action", "Folders"), "folder", {});
        add("showKeys", kli18nc("@action", "Keys"), "view-certificate", {});
        add("showAll", kli18nc("@action", "All Items"), "view-list-details", {});
        add("showFavorites", kli18nc("@action", "Favorites"), "rating", {});
        add("showTrash", kli18nc("@action", "Trash"), "user-trash", {});
        add("toggleFavorite", kli18nc("@action", "Favorite"), "rating", {});
        add("restoreEntry", kli18nc("@action", "Restore Item"), "edit-undo", {});
        add("deleteEntry", kli18nc("@action", "Delete Item Permanently…"), "edit-delete", {});
        add("copyUsername", kli18nc("@action", "Copy Username"), "edit-copy", {QKeySequence(QStringLiteral("Ctrl+Shift+U"))});
        add("copyPassword", kli18nc("@action", "Copy Password"), "edit-copy", {QKeySequence(QStringLiteral("Ctrl+Shift+P"))});
        add("copyTotp", kli18nc("@action", "Copy Verification Code"), "edit-copy", {QKeySequence(QStringLiteral("Ctrl+Shift+T"))});
        add("showBackupInFolder", kli18nc("@action", "Show in Folder"), "system-file-manager", {});
        add("lockVault", kli18nc("@action", "Lock or Unlock Vault"), "object-locked", {});
        add("copyFingerprint", kli18nc("@action", "Copy Fingerprint"), "edit-copy", {});
        add("folderUp", kli18nc("@action", "Move Folder Up"), "go-up", {QKeySequence(QStringLiteral("Ctrl+Alt+Up"))});
        add("folderDown", kli18nc("@action", "Move Folder Down"), "go-down", {QKeySequence(QStringLiteral("Ctrl+Alt+Down"))});
        add("moveFolder", kli18nc("@action", "Move Folder…"), "folder-move", {});
        auto *configureApp = result->addAction(QStringLiteral("configureApp"));
        configureApp->setText(i18nc("@action", "Configure %1…", QString::fromUtf8(kryptopass::identity::display_name)));
        configureApp->setIcon(QIcon::fromTheme(QStringLiteral("preferences-other")));
        KActionCollection::setDefaultShortcuts(configureApp, KStandardShortcut::shortcut(KStandardShortcut::Preferences));
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
    // Qt's own texts (standard buttons, file dialogs). The application's come from KI18n.
    auto *qtTranslator = new QTranslator(qApp);
    if (qtTranslator->load(QLocale(), QStringLiteral("qt"), QStringLiteral("_"), QLibraryInfo::path(QLibraryInfo::TranslationsPath))) {
        QCoreApplication::installTranslator(qtTranslator);
    } else {
        delete qtTranslator;
    }

    KAboutData about(QString::fromUtf8(desktop_file_id), QString::fromUtf8(display_name), QString::fromUtf8(version));
    // Messages.sh extracts the description from Cargo.toml with this context.
    about.setShortDescription(i18nc("@info application description", description));
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
    // Sets the application name, version, display name, organization domain and desktop
    // file name. KAboutData has no organization name, which QSettings needs.
    KAboutData::setApplicationData(about);
    QCoreApplication::setOrganizationName(QString::fromUtf8(organization));
    QApplication::setWindowIcon(icon);
}

KHelpMenu *helpMenu()
{
    static auto *help = new KHelpMenu;
    return help;
}

void show_about_application_dialog()
{
    // KDE's standard About window, fed by the KAboutData registered at startup.
    helpMenu()->aboutApplication();
}

void show_handbook(const QString &anchor)
{
    // Opens help:/<component name>, optionally at a section id of the handbook.
    KHelpClient::invokeHelp(anchor);
}

void show_about_kde_dialog()
{
    helpMenu()->aboutKDE();
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
    // The msgid is only known at run time, so it goes straight to KLocalizedString. Each
    // %N is substituted by itself: the Rust caller fills the placeholders afterwards.
    const auto bytes = source.toUtf8();
    auto message = ki18n(bytes.constData());
    static const QRegularExpression placeholder(QStringLiteral("%(\\d+)"));
    int highest = 0;
    for (auto it = placeholder.globalMatch(source); it.hasNext();) {
        highest = qMax(highest, it.next().captured(1).toInt());
    }
    for (int index = 1; index <= highest; ++index) {
        message = message.subs(QStringLiteral("%%1").arg(index));
    }
    return message.toString();
}

QString translate_plural(const QString &singular, const QString &plural, int count)
{
    const auto singularBytes = singular.toUtf8();
    const auto pluralBytes = plural.toUtf8();
    return ki18np(singularBytes.constData(), pluralBytes.constData()).subs(count).toString();
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
void capture_screenshot(const QString &path, bool devicesWindow)
{
    QTimer::singleShot(2000, qApp, [path, devicesWindow] {
        for (auto *window : QGuiApplication::topLevelWindows()) {
            if (auto *quick = qobject_cast<QQuickWindow *>(window);
                quick && quick->isVisible() && (!devicesWindow || quick->objectName() == QStringLiteral("devicesWindow"))) {
                QCoreApplication::exit(quick->grabWindow().save(path) ? 0 : 1);
                return;
            }
        }
        QCoreApplication::exit(1);
    });
}
}
