#pragma once
#include "rust/cxx.h"
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QString>
#include <memory>

namespace kryptopass {
std::unique_ptr<QGuiApplication> new_application(const rust::Vec<rust::String>& args);
void configure_engine(QQmlApplicationEngine& engine, bool smoke);
void log_message(const QString& message);
}
