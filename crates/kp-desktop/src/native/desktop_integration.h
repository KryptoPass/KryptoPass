#pragma once
#include <QString>
namespace kryptopass::desktop::native
{
void initialize_desktop();
QString translate_ui(const QString &source);
QString translate_plural(const QString &source, int count);
QString desktop_actions();
QString about_data();
void set_action_enabled(const QString &id, bool enabled);
void show_about_kde_dialog();
bool open_bug_report();
QString configure_shortcuts();
void capture_screenshot(const QString &path);
}
