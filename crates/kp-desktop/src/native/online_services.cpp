#include "native/online_services.h"
#include <QDesktopServices>
#include <QSettings>
#include <QUrl>

namespace kryptopass::desktop::native
{
QString online_settings()
{
    return QSettings().value(QStringLiteral("openpgp/onlineServices")).toString();
}

bool save_online_settings(const QString &json)
{
    QSettings settings;
    settings.setValue(QStringLiteral("openpgp/onlineServices"), json);
    settings.sync();
    return settings.status() == QSettings::NoError;
}

bool open_online_url(const QString &url)
{
    const QUrl target(url);
    if (!target.isValid() || target.host().isEmpty() || (target.scheme() != QStringLiteral("https") && target.scheme() != QStringLiteral("http")))
        return false;
    return QDesktopServices::openUrl(target);
}
}
