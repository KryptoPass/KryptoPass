#include "native/command_line.h"

#include <KAboutData>
#include <KLocalizedString>
#include <QCommandLineOption>
#include <QCommandLineParser>
#include <QCoreApplication>
#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

namespace
{
// The whole command-line surface. `.desktop` actions, `just` recipes and
// `packaging/capture_screenshot.py` use these names.
struct Options {
    QCommandLineOption newEntry{QStringLiteral("new-entry"), i18nc("@info:shell", "Creates an item as soon as the vault is open.")};
    QCommandLineOption search{QStringLiteral("search"), i18nc("@info:shell", "Opens the search as soon as the vault is open.")};
    QCommandLineOption devices{QStringLiteral("devices"), i18nc("@info:shell", "Opens the devices window.")};
    QCommandLineOption newInstance{QStringLiteral("new-instance"), i18nc("@info:shell", "Starts separately from an instance that is already open.")};
    QCommandLineOption browserIntegration{QStringLiteral("browser-integration"), i18nc("@info:shell", "Serves the browser integration.")};
    QCommandLineOption screenshot{QStringLiteral("screenshot"),
                                  i18nc("@info:shell", "Saves a screenshot of the window to the file and quits."),
                                  i18nc("@info:shell value name", "file")};
};

void setup(QCommandLineParser &parser, const Options &options)
{
    auto about = KAboutData::applicationData();
    parser.setApplicationDescription(about.shortDescription());
    parser.addOptions({options.newEntry, options.search, options.devices, options.newInstance, options.browserIntegration, options.screenshot});
    parser.addPositionalArgument(QStringLiteral("file"),
                                 i18nc("@info:shell", "A .kpb backup to preselect for restoring."),
                                 i18nc("@info:shell argument syntax", "[file]"));
    // Adds --help, --version, --author, --license and --desktopfile.
    about.setupCommandLine(&parser);
}

QString toJson(const QCommandLineParser &parser, const Options &options, const QString &workingDirectory)
{
    QJsonArray files;
    const QDir base(workingDirectory);
    for (const auto &file : parser.positionalArguments())
        files.append(QDir::cleanPath(base.absoluteFilePath(file)));
    const QJsonObject object{
        {QStringLiteral("newEntry"), parser.isSet(options.newEntry)},
        {QStringLiteral("search"), parser.isSet(options.search)},
        {QStringLiteral("devices"), parser.isSet(options.devices)},
        {QStringLiteral("newInstance"), parser.isSet(options.newInstance)},
        {QStringLiteral("browserIntegration"), parser.isSet(options.browserIntegration)},
        {QStringLiteral("screenshot"), parser.isSet(options.screenshot) ? QJsonValue(parser.value(options.screenshot)) : QJsonValue()},
        {QStringLiteral("files"), files},
    };
    return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}
}

namespace kryptopass::desktop::native
{
QString process_command_line()
{
    QCommandLineParser parser;
    const Options options;
    setup(parser, options);
    parser.process(*QCoreApplication::instance());
    auto about = KAboutData::applicationData();
    about.processCommandLine(&parser);
    // Keeps a desktop file name given with --desktopfile.
    KAboutData::setApplicationData(about);
    return toJson(parser, options, QDir::currentPath());
}

QString parse_forwarded_command_line(const QStringList &arguments, const QString &workingDirectory)
{
    QCommandLineParser parser;
    const Options options;
    setup(parser, options);
    // A launch that reached the running instance already passed its own parser;
    // keep whatever is recognised and ignore the rest.
    parser.parse(arguments);
    return toJson(parser, options, workingDirectory);
}
}
