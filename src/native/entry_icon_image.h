#pragma once

#include <QByteArray>
#include <QImage>

namespace kryptopass::desktop::native
{
QImage decodeEntryIcon(const QByteArray &bytes, bool normalized);
QImage decodeFavicon(const QByteArray &bytes);
QImage normalizeEntryIcon(QImage source);
QByteArray encodeEntryIcon(const QImage &image);
}
