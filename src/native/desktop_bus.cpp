#include "native/desktop_bus.h"

#include <KAboutData>
#include <KDBusService>
#include <KWindowSystem>
#include <QDBusConnection>
#include <QGuiApplication>
#include <QPointer>
#include <QWindow>

#include <algorithm>

namespace
{
constexpr auto kVaultObjectPath = "/Vault";
// Set by installMainWindowStateSaver; marks the main window among the top-level ones.
constexpr auto kMainWindowProperty = "kryptopassMainWindowStateSaverInstalled";
QPointer<KDBusService> service;

void raiseMainWindow()
{
    for (QWindow *window : QGuiApplication::topLevelWindows()) {
        if (!window->property(kMainWindowProperty).toBool())
            continue;
        if (window->windowState() & Qt::WindowMinimized)
            window->showNormal();
        else
            window->show();
        // The activation token is only available while activateRequested runs.
        KWindowSystem::updateStartupId(window);
        window->raise();
        KWindowSystem::activateWindow(window);
        return;
    }
}
}

namespace kryptopass::desktop::native
{

void VaultBusAdaptor::Lock()
{
    Q_EMIT lockRequested();
}

bool register_unique_application()
{
    // KDBusService names the bus <reversed organization domain>.<component>, and our
    // component already is the full app ID. Lend it the last label only while it
    // registers, so the bus name equals the desktop file ID; settings, shortcuts and
    // the handbook keep using the real component name.
    const KAboutData about = KAboutData::applicationData();
    const QString appId = about.componentName();
    QStringList domainLabels = about.organizationDomain().split(QLatin1Char('.'));
    std::reverse(domainLabels.begin(), domainLabels.end());
    const QString domainPrefix = domainLabels.join(QLatin1Char('.')) + QLatin1Char('.');
    if (appId.startsWith(domainPrefix)) {
        KAboutData busAbout(appId.mid(domainPrefix.size()), about.displayName(), about.version());
        busAbout.setOrganizationDomain(about.organizationDomain().toUtf8());
        busAbout.setDesktopFileName(about.desktopFileName());
        KAboutData::setApplicationData(busAbout);
    }
    // A duplicate instance forwards its arguments to the running one and exits here,
    // before it opens the vault or starts device listeners.
    service = new KDBusService(KDBusService::Unique | KDBusService::NoExitOnFailure, qApp);
    KAboutData::setApplicationData(about);
    return service->isRegistered();
}

void install_activation_receiver(QObject *receiver)
{
    if (!service)
        return;
    QObject::connect(service, &KDBusService::activateRequested, receiver, [receiver](const QStringList &arguments, const QString &workingDirectory) {
        raiseMainWindow();
        QMetaObject::invokeMethod(receiver, "receiveActivation", Qt::DirectConnection, Q_ARG(QStringList, arguments), Q_ARG(QString, workingDirectory));
    });
}

void install_vault_bus(QObject *receiver)
{
    auto *adaptor = new VaultBusAdaptor(receiver);
    QObject::connect(adaptor, &VaultBusAdaptor::lockRequested, receiver, [receiver] {
        QMetaObject::invokeMethod(receiver, "lockForSession", Qt::DirectConnection);
    });
    if (!QDBusConnection::sessionBus().registerObject(QLatin1String(kVaultObjectPath), adaptor, QDBusConnection::ExportScriptableSlots)) {
        qWarning("Could not export the vault D-Bus interface");
    }
}

}
