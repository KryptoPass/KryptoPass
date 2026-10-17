#include "application.h"

#include "cxx-qt-lib/qcoreapplication.h"
#include <QApplication>
#include <QDBusPendingCallWatcher>
#include <QDeadlineTimer>
#include <QEvent>
#include <QEventLoop>
#include <QPointer>
#include <QTimer>

namespace kryptopass
{
std::unique_ptr<QGuiApplication> newApplication(const QVector<QByteArray> &args)
{
    // KDE's contact editor uses widgets; keep argv alive for the application lifetime.
    auto argsData = std::make_unique<rust::cxxqtlib1::ApplicationArgsData>(args);
    auto app = std::make_unique<QApplication>(argsData->size(), argsData->data());
    argsData->setParent(app.get());
    argsData.release();
    return app;
}

void finishDesktopRegistration()
{
#ifdef Q_OS_LINUX
    // Qt queues host-portal registration until the desktop ID is available.
    // Finish that public bootstrap before PR_SET_DUMPABLE blocks /proc access.
    QCoreApplication::sendPostedEvents(nullptr, QEvent::MetaCall);
    QList<QPointer<QDBusPendingCallWatcher>> pending;
    for (auto *watcher : qApp->findChildren<QDBusPendingCallWatcher *>())
        pending.append(watcher);

    const QDeadlineTimer deadline(1500);
    for (const auto &watcher : pending) {
        if (!watcher || watcher->isFinished())
            continue;
        if (deadline.hasExpired())
            break;
        QEventLoop loop;
        QTimer timeout;
        timeout.setSingleShot(true);
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &loop, &QEventLoop::quit);
        QObject::connect(&timeout, &QTimer::timeout, &loop, &QEventLoop::quit);
        timeout.start(static_cast<int>(deadline.remainingTime()));
        loop.exec(QEventLoop::ExcludeUserInputEvents);
    }
#endif
}
}
