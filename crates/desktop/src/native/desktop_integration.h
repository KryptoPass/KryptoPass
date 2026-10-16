#pragma once
#include <QString>
namespace kryptopass::desktop::native {
void initialize_desktop();
void show_about_dialog();
QString translate_ui(const QString &source);
QString translate_plural(const QString &source, int count);
QString configured_shortcuts();
QString configure_shortcuts();
void capture_screenshot(const QString &path);
}
