#pragma once

#include <QObject>
#include <QDBusUnixFileDescriptor>
#include <QtGlobal>

namespace kryptopass::desktop::native {

class SessionLockMonitor final : public QObject
{
    Q_OBJECT

public:
    explicit SessionLockMonitor(QObject *parent = nullptr, bool watchSession = true);
    ~SessionLockMonitor() override;
    int idleSeconds() const;
    void setIdleSeconds(int seconds);

Q_SIGNALS:
    void lockRequested();
    void blockedChanged(bool blocked);

public Q_SLOTS:
    void screenSaverChanged(bool active);
    void prepareForSleep(bool sleeping);
    void idleTimeoutReached(int identifier, int milliseconds);

private:
    void queryScreenSaver();
    void acquireSleepInhibitor();
    void updateBlocked();
    bool watchSession_;
    bool screenLocked_ = false;
    bool sleeping_ = false;
    bool blocked_ = false;
    int idleSeconds_ = 0;
    int idleTimeoutId_ = -1;
    quint64 screenQuery_ = 0;
    quint64 sleepQuery_ = 0;
    QDBusUnixFileDescriptor sleepInhibitor_;
};

void install_session_lock_monitor(QObject *receiver);
int idle_lock_seconds();
void set_idle_lock_seconds(int seconds);

}
