#pragma once

#include <QByteArray>
#include <QGuiApplication>
#include <QVector>
#include <memory>

namespace kryptopass {
std::unique_ptr<QGuiApplication> newApplication(const QVector<QByteArray>& args);
}
