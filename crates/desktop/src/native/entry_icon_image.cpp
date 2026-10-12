#include "entry_icon_image.h"

#include <QBuffer>
#include <QColorSpace>
#include <QImageReader>
#include <cstring>

namespace kryptopass::desktop::native {
namespace {
bool boundedSize(const QSize &size, int dimension, qint64 pixels) {
    return size.isValid() && size.width() > 0 && size.height() > 0
        && size.width() <= dimension && size.height() <= dimension
        && qint64(size.width()) * size.height() <= pixels;
}
}

QImage decodeEntryIcon(const QByteArray &bytes, bool normalized) {
    if (bytes.isEmpty() || bytes.size() > (normalized ? 512 * 1024 : 4 * 1024 * 1024)) return {};
    QBuffer buffer;
    buffer.setData(bytes);
    buffer.open(QIODevice::ReadOnly);
    QImageReader reader(&buffer);
    reader.setAutoTransform(true);
    const auto format = reader.format().toLower();
    if (normalized ? format != "png" : (format != "png" && format != "jpeg" && format != "webp")) return {};
    if (!boundedSize(reader.size(), normalized ? 256 : 4096, 4194304) || reader.imageCount() > 1) return {};
    const auto image = reader.read();
    return reader.jumpToNextImage() ? QImage() : image;
}

QImage decodeFavicon(const QByteArray &bytes) {
    if (bytes.isEmpty() || bytes.size() > 1024 * 1024) return {};
    QBuffer buffer;
    buffer.setData(bytes);
    buffer.open(QIODevice::ReadOnly);
    QImageReader reader(&buffer);
    reader.setAutoTransform(true);
    const auto format = reader.format().toLower();
    if (format != "png" && format != "jpeg" && format != "webp" && format != "ico") return {};
    if (format != "ico") {
        if (!boundedSize(reader.size(), 512, 512 * 512) || reader.imageCount() > 1) return {};
        const auto image = reader.read();
        if (!boundedSize(image.size(), 512, 512 * 512) || reader.jumpToNextImage()) return {};
        return image;
    }
    QImage best;
    const int entries = qMin(reader.imageCount(), 16);
    for (int index = 0; index < entries; ++index) {
        if (index > 0 && !reader.jumpToImage(index)) break;
        // Some ICO handlers expose dimensions only after reading an entry.
        const auto size = reader.size();
        if (size.isValid() && !boundedSize(size, 512, 512 * 512)) continue;
        const auto candidate = reader.read();
        if (!boundedSize(candidate.size(), 512, 512 * 512)) continue;
        if (best.isNull() || candidate.width() * candidate.height() > best.width() * best.height()) best = candidate;
    }
    return best;
}

QImage normalizeEntryIcon(QImage source) {
    if (source.isNull()) return {};
    if (source.colorSpace().isValid()) source = source.convertedToColorSpace(QColorSpace::SRgb);
    if (source.width() > 256 || source.height() > 256)
        source = source.scaled(256, 256, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    // Copy pixels into a fresh buffer to discard metadata while preserving alpha.
    QImage image(source.size(), QImage::Format_RGBA8888);
    const auto pixels = source.convertToFormat(QImage::Format_RGBA8888);
    for (int y = 0; y < image.height(); ++y)
        std::memcpy(image.scanLine(y), pixels.constScanLine(y), static_cast<size_t>(image.width()) * 4);
    return image;
}

QByteArray encodeEntryIcon(const QImage &image) {
    if (image.isNull()) return {};
    QByteArray bytes;
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::WriteOnly);
    if (!image.save(&buffer, "PNG") || bytes.size() > 512 * 1024) return {};
    return bytes;
}
}
