#pragma once

#include <QByteArray>
#include <QGuiApplication>
#include <QVector>
#include <memory>

class QQmlApplicationEngine;

namespace kryptopass
{
std::unique_ptr<QGuiApplication> newApplication(const QVector<QByteArray> &args);
void finishDesktopRegistration();
void installMainWindowStateSaver(const QQmlApplicationEngine &engine);
}
