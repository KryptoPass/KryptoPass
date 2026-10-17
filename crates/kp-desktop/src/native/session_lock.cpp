#include "native/session_lock.h"

#include <KIdleTime>
#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDBusServiceWatcher>
#include <QPointer>
#include <QSettings>

#include <algorithm>

namespace {
constexpr auto kIdleSettingsKey = "security/idleLockSeconds";
constexpr int kDefaultIdleSeconds = 5 * 60;
constexpr int kMaximumIdleSeconds = 24 * 60 * 60;
QPointer<kryptopass::desktop::native::SessionLockMonitor> monitor;
}

namespace kryptopass::desktop::native {

SessionLockMonitor::SessionLockMonitor(QObject *parent, bool watchSession)
    : QObject(parent)
    , watchSession_(watchSession)
{
    auto *idle = KIdleTime::instance();
    connect(idle, &KIdleTime::timeoutReached, this, &SessionLockMonitor::idleTimeoutReached);
    setIdleSeconds(idle_lock_seconds());
    if (!watchSession) {
        return;
    }

    auto session = QDBusConnection::sessionBus();
    session.connect(QStringLiteral("org.freedesktop.ScreenSaver"), QStringLiteral("/ScreenSaver"),
                    QStringLiteral("org.freedesktop.ScreenSaver"), QStringLiteral("ActiveChanged"),
                    this, SLOT(screenSaverChanged(bool)));
    auto system = QDBusConnection::systemBus();
    system.connect(QStringLiteral("org.freedesktop.login1"), QStringLiteral("/org/freedesktop/login1"),
                   QStringLiteral("org.freedesktop.login1.Manager"), QStringLiteral("PrepareForSleep"),
                   this, SLOT(prepareForSleep(bool)));

    auto *watcher = new QDBusServiceWatcher(QStringLiteral("org.freedesktop.ScreenSaver"), session,
                                           QDBusServiceWatcher::WatchForOwnerChange, this);
    connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
            [this](const QString &, const QString &, const QString &owner) {
                ++screenQuery_;
                if (owner.isEmpty()) {
                    screenSaverChanged(false);
                } else {
                    queryScreenSaver();
                }
            });
    queryScreenSaver();
    acquireSleepInhibitor();
}

SessionLockMonitor::~SessionLockMonitor()
{
    if (idleTimeoutId_ != -1) {
        KIdleTime::instance()->removeIdleTimeout(idleTimeoutId_);
    }
}

int SessionLockMonitor::idleSeconds() const
{
    return idleSeconds_;
}

void SessionLockMonitor::setIdleSeconds(int seconds)
{
    if (idleTimeoutId_ != -1) {
        KIdleTime::instance()->removeIdleTimeout(idleTimeoutId_);
        idleTimeoutId_ = -1;
    }
    idleSeconds_ = std::clamp(seconds, 0, kMaximumIdleSeconds);
    if (idleSeconds_ > 0) {
        idleTimeoutId_ = KIdleTime::instance()->addIdleTimeout(idleSeconds_ * 1000);
    }
}

void SessionLockMonitor::screenSaverChanged(bool active)
{
    ++screenQuery_;
    screenLocked_ = active;
    updateBlocked();
}

void SessionLockMonitor::prepareForSleep(bool sleeping)
{
    sleeping_ = sleeping;
    updateBlocked();
    if (sleeping) {
        ++sleepQuery_;
        // Release the delay only after synchronous vault invalidation completed.
        sleepInhibitor_ = QDBusUnixFileDescriptor{};
    } else if (watchSession_) {
        queryScreenSaver();
        acquireSleepInhibitor();
    }
}

void SessionLockMonitor::updateBlocked()
{
    const bool blocked = screenLocked_ || sleeping_;
    if (blocked_ == blocked) {
        return;
    }
    blocked_ = blocked;
    Q_EMIT blockedChanged(blocked);
    if (blocked) {
        Q_EMIT lockRequested();
    }
}

void SessionLockMonitor::idleTimeoutReached(int identifier, int)
{
    if (idleTimeoutId_ != -1 && identifier == idleTimeoutId_) {
        Q_EMIT lockRequested();
    }
}

void SessionLockMonitor::queryScreenSaver()
{
    const auto request = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.ScreenSaver"),
        QStringLiteral("/ScreenSaver"), QStringLiteral("org.freedesktop.ScreenSaver"), QStringLiteral("GetActive"));
    const auto query = ++screenQuery_;
    auto *watcher = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(request), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, query](QDBusPendingCallWatcher *call) {
        const QDBusPendingReply<bool> reply = *call;
        call->deleteLater();
        // A newer signal takes precedence over an earlier asynchronous snapshot.
        if (query == screenQuery_ && !reply.isError()) {
            screenSaverChanged(reply.value());
        }
    });
}

void SessionLockMonitor::acquireSleepInhibitor()
{
    auto request = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.login1"),
        QStringLiteral("/org/freedesktop/login1"), QStringLiteral("org.freedesktop.login1.Manager"), QStringLiteral("Inhibit"));
    request.setArguments({QStringLiteral("sleep"), QCoreApplication::applicationName(),
        QCoreApplication::translate("SessionLock", "Bloquear la bóveda antes de suspender"), QStringLiteral("delay")});
    const auto query = ++sleepQuery_;
    auto *watcher = new QDBusPendingCallWatcher(QDBusConnection::systemBus().asyncCall(request), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, query](QDBusPendingCallWatcher *call) {
        const QDBusPendingReply<QDBusUnixFileDescriptor> reply = *call;
        call->deleteLater();
        if (query == sleepQuery_ && !sleeping_ && !reply.isError()) {
            sleepInhibitor_ = reply.value();
        }
    });
}

void install_session_lock_monitor(QObject *receiver)
{
    auto *sessionMonitor = new SessionLockMonitor(receiver);
    monitor = sessionMonitor;
    QObject::connect(sessionMonitor, &SessionLockMonitor::blockedChanged, receiver, [receiver](bool blocked) {
        QMetaObject::invokeMethod(receiver, "setSessionBlocked", Qt::DirectConnection, Q_ARG(bool, blocked));
    });
    QObject::connect(sessionMonitor, &SessionLockMonitor::lockRequested, receiver, [receiver] {
        QMetaObject::invokeMethod(receiver, "lockForSession", Qt::DirectConnection);
    });
}

int idle_lock_seconds()
{
    bool ok = false;
    const int seconds = QSettings().value(QLatin1String(kIdleSettingsKey), kDefaultIdleSeconds).toInt(&ok);
    return ok ? std::clamp(seconds, 0, kMaximumIdleSeconds) : kDefaultIdleSeconds;
}

void set_idle_lock_seconds(int seconds)
{
    seconds = std::clamp(seconds, 0, kMaximumIdleSeconds);
    QSettings().setValue(QLatin1String(kIdleSettingsKey), seconds);
    if (monitor) {
        monitor->setIdleSeconds(seconds);
    }
}

}
