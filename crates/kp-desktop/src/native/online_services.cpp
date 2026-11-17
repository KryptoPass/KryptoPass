#include "native/online_services.h"
#include "native/settings.h"
#include <QDesktopServices>
#include <QUrl>

namespace kryptopass::desktop::native
{
QString online_settings()
{
    return settings::group(settings::kOpenPgpGroup).readEntry(settings::kOnlineServices, QString());
}

bool save_online_settings(const QString &json)
{
    auto group = settings::group(settings::kOpenPgpGroup);
    group.writeEntry(settings::kOnlineServices, json);
    return group.sync();
}

bool open_online_url(const QString &url)
{
    const QUrl target(url);
    if (!target.isValid() || target.host().isEmpty() || (target.scheme() != QStringLiteral("https") && target.scheme() != QStringLiteral("http")))
        return false;
    return QDesktopServices::openUrl(target);
}
}
