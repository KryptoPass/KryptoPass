#include "application.h"

#include "cxx-qt-lib/qcoreapplication.h"
#include <QApplication>

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
}
