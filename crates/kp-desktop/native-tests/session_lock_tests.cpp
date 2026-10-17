#include "native/session_lock.h"

#include <KIdleTime>
#include <QSettings>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>

using kryptopass::desktop::native::SessionLockMonitor;
using kryptopass::desktop::native::set_idle_lock_seconds;

namespace
{
int addedTimeout(const QHash<int, int> &before)
{
    const auto current = KIdleTime::instance()->idleTimeouts();
    for (auto iterator = current.cbegin(); iterator != current.cend(); ++iterator) {
        if (!before.contains(iterator.key())) {
            return iterator.key();
        }
    }
    return -1;
}
}

class SessionLockTests : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QVERIFY(settings_.isValid());
        QCoreApplication::setOrganizationName(QStringLiteral("org.kryptopass.tests"));
        QCoreApplication::setApplicationName(QStringLiteral("org.kryptopass.tests.sessionlock"));
        QSettings::setDefaultFormat(QSettings::IniFormat);
        QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, settings_.path());
    }

    void init()
    {
        QSettings().clear();
        set_idle_lock_seconds(0);
    }

    void overlappingSessionEventsKeepAccessBlocked_data()
    {
        QTest::addColumn<bool>("screenFirst");
        QTest::newRow("screen-lock-then-suspend") << true;
        QTest::newRow("suspend-then-screen-lock") << false;
    }

    void overlappingSessionEventsKeepAccessBlocked()
    {
        QFETCH(bool, screenFirst);
        SessionLockMonitor monitor(nullptr, false);
        QStringList events;
        connect(&monitor, &SessionLockMonitor::blockedChanged, this, [&events](bool blocked) {
            events.append(blocked ? QStringLiteral("blocked") : QStringLiteral("unblocked"));
        });
        connect(&monitor, &SessionLockMonitor::lockRequested, this, [&events] {
            events.append(QStringLiteral("lock"));
        });
        const auto first = [&](bool active) {
            if (screenFirst) {
                monitor.screenSaverChanged(active);
            } else {
                monitor.prepareForSleep(active);
            }
        };
        const auto second = [&](bool active) {
            if (screenFirst) {
                monitor.prepareForSleep(active);
            } else {
                monitor.screenSaverChanged(active);
            }
        };

        first(false);
        second(false);
        QVERIFY(events.isEmpty());

        first(true);
        QCOMPARE(events, QStringList({QStringLiteral("blocked"), QStringLiteral("lock")}));
        first(true);
        second(true);
        first(false);
        QCOMPARE(events.size(), 2);

        second(false);
        QCOMPARE(events.last(), QStringLiteral("unblocked"));
        QCOMPARE(events.size(), 3);
        second(false);
        QCOMPARE(events.size(), 3);

        first(true);
        QCOMPARE(events.mid(3), QStringList({QStringLiteral("blocked"), QStringLiteral("lock")}));
    }

    void idleRequestsLockWithoutBlockingManualUnlock()
    {
        const auto before = KIdleTime::instance()->idleTimeouts();
        SessionLockMonitor monitor(nullptr, false);
        QSignalSpy locks(&monitor, &SessionLockMonitor::lockRequested);
        QSignalSpy blocked(&monitor, &SessionLockMonitor::blockedChanged);
        monitor.setIdleSeconds(30);
        const int identifier = addedTimeout(before);
        QVERIFY(identifier != -1);
        QCOMPARE(KIdleTime::instance()->idleTimeouts().value(identifier), 30000);

        KIdleTime::instance()->timeoutReached(-1, 30000);
        QCOMPARE(locks.size(), 0);
        KIdleTime::instance()->timeoutReached(identifier, 30000);
        QCOMPARE(locks.size(), 1);
        QVERIFY(blocked.isEmpty());

        KIdleTime::instance()->timeoutReached(identifier, 30000);
        QCOMPARE(locks.size(), 2);
        QVERIFY(blocked.isEmpty());
    }

    void replacedAndDisabledIdleTimeoutsCannotLock()
    {
        const auto before = KIdleTime::instance()->idleTimeouts();
        SessionLockMonitor monitor(nullptr, false);
        QSignalSpy locks(&monitor, &SessionLockMonitor::lockRequested);
        monitor.setIdleSeconds(30);
        const int original = addedTimeout(before);
        QVERIFY(original != -1);

        monitor.setIdleSeconds(60);
        const int replacement = addedTimeout(before);
        QVERIFY(replacement != -1);
        QVERIFY(replacement != original);
        QVERIFY(!KIdleTime::instance()->idleTimeouts().contains(original));
        QCOMPARE(KIdleTime::instance()->idleTimeouts().value(replacement), 60000);
        KIdleTime::instance()->timeoutReached(original, 30000);
        QCOMPARE(locks.size(), 0);
        KIdleTime::instance()->timeoutReached(replacement, 60000);
        QCOMPARE(locks.size(), 1);

        monitor.setIdleSeconds(0);
        QCOMPARE(monitor.idleSeconds(), 0);
        QCOMPARE(KIdleTime::instance()->idleTimeouts(), before);
        KIdleTime::instance()->timeoutReached(original, 30000);
        KIdleTime::instance()->timeoutReached(replacement, 60000);
        KIdleTime::instance()->timeoutReached(-1, 0);
        QCOMPARE(locks.size(), 1);
    }

    void destructionRemovesOnlyTheMonitorsIdleTimeout()
    {
        auto *idle = KIdleTime::instance();
        const int other = idle->addIdleTimeout(15000);
        const auto before = idle->idleTimeouts();
        {
            SessionLockMonitor monitor(nullptr, false);
            monitor.setIdleSeconds(30);
            QCOMPARE(idle->idleTimeouts().size(), before.size() + 1);
        }
        QCOMPARE(idle->idleTimeouts(), before);
        idle->removeIdleTimeout(other);
    }

private:
    QTemporaryDir settings_;
};

QTEST_GUILESS_MAIN(SessionLockTests)

#include "session_lock_tests.moc"
