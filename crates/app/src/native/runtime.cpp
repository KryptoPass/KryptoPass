#include "runtime.h"
#include "cxx-qt-lib/qcoreapplication.h"
#include <QIcon>
#include <QTimer>
#include <QDebug>

namespace kryptopass {
std::unique_ptr<QGuiApplication> new_application(const rust::Vec<rust::String>& args)
{
    QVector<QByteArray> values;
    for (const auto& arg : args)
        values.append(QByteArray(arg.data(), arg.size()));
    // Qt retains argv; attach its owned storage to the application's lifetime.
    auto storage = std::make_unique<rust::cxxqtlib1::ApplicationArgsData>(values);
    auto app = std::make_unique<QGuiApplication>(storage->size(), storage->data());
    storage->setParent(app.get());
    storage.release();
    return app;
}

void configure_engine(QQmlApplicationEngine& engine, bool smoke)
{
    QIcon::setThemeName(QStringLiteral("breeze"));
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed,
                     &engine, [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    if (smoke)
        QTimer::singleShot(3000, &engine, [] { QCoreApplication::quit(); });
}

void log_message(const QString& message)
{
    qInfo().noquote() << message;
}
}
