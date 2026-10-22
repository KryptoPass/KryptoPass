#pragma once

#include <QUrl>

class QObject;

namespace kryptopass::desktop::native
{
void show_in_folder(QObject *receiver, const QUrl &url);
}
