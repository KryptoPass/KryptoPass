#pragma once
#include <QString>
namespace kryptopass::desktop::native
{
QString online_settings();
bool save_online_settings(const QString &json);
bool open_online_url(const QString &url);
}
