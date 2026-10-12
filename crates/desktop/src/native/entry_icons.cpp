#include "entry_icons.h"
#include <QBuffer>
#include <QCache>
#include <QColorSpace>
#include <QCryptographicHash>
#include <QDebug>
#include <QDir>
#include <QEventLoop>
#include <QFile>
#include <QFileInfo>
#include <QImageReader>
#include <QMap>
#include <QMutex>
#include <QMutexLocker>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QQuickImageProvider>
#include <QRegularExpression>
#include <QSaveFile>
#include <QSet>
#include <QStandardPaths>
#include <QStringList>
#include <QTimer>
#include <QUrl>
#include <chrono>
#include <cstring>
#include <future>
#include <thread>

namespace kryptopass::desktop::native {
namespace {
QImage decode(rust::Slice<const std::uint8_t> bytes, bool normalized) {
    if (bytes.empty() || bytes.size() > (normalized ? 512u * 1024u : 4u * 1024u * 1024u)) return {};
    QByteArray data(reinterpret_cast<const char *>(bytes.data()), static_cast<qsizetype>(bytes.size()));
    QBuffer buffer(&data);
    buffer.open(QIODevice::ReadOnly);
    QImageReader reader(&buffer);
    reader.setAutoTransform(true);
    const auto format = reader.format().toLower();
    if (normalized ? format != "png" : (format != "png" && format != "jpeg" && format != "webp")) return {};
    const QSize size = reader.size();
    const int limit = normalized ? 256 : 4096;
    if (!size.isValid() || size.width() > limit || size.height() > limit
        || qint64(size.width()) * size.height() > 4194304 || reader.imageCount() > 1) return {};
    auto image = reader.read();
    if (image.isNull() || reader.jumpToNextImage()) return {};
    return image;
}

// Rendered pixels only: normalized sRGB output without file metadata.
QImage renderNormalized(QImage source) {
    if (source.isNull()) return {};
    if (source.colorSpace().isValid()) source = source.convertedToColorSpace(QColorSpace::SRgb);
    if (source.width() > 256 || source.height() > 256)
        source = source.scaled(256, 256, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    // A fresh pixel buffer preserves alpha while dropping file metadata.
    QImage image(source.size(), QImage::Format_RGBA8888);
    const auto pixels = source.convertToFormat(QImage::Format_RGBA8888);
    for (int y = 0; y < image.height(); ++y)
        std::memcpy(image.scanLine(y), pixels.constScanLine(y), static_cast<size_t>(image.width()) * 4);
    return image;
}

rust::Vec<std::uint8_t> encodeNormalizedPng(const QImage &image) {
    rust::Vec<std::uint8_t> result;
    if (image.isNull()) return result;
    QByteArray output;
    QBuffer buffer(&output);
    buffer.open(QIODevice::WriteOnly);
    if (!image.save(&buffer, "PNG") || output.size() > 512 * 1024) return result;
    result.reserve(output.size());
    for (auto byte : output) result.push_back(static_cast<std::uint8_t>(byte));
    return result;
}

// Second validation layer; Rust already rejects anything exotic before calling.
bool validFaviconHost(const QString &host) {
    if (host.isEmpty() || host.size() > 253 || !host.contains(QChar('.'))) return false;
    if (host != host.toLower()) return false;
    for (const QChar separator : {QChar('/'), QChar(':'), QChar('@'), QChar(' '), QChar('%'), QChar('?'), QChar('#')}) {
        if (host.contains(separator)) return false;
    }
    const auto labels = host.split(QChar('.'));
    for (const auto &label : labels) {
        if (label.isEmpty() || label.size() > 63 || label.startsWith(QChar('-')) || label.endsWith(QChar('-'))) return false;
        for (const QChar ch : label) {
            const uint code = ch.unicode();
            if (!((code >= 'a' && code <= 'z') || (code >= '0' && code <= '9') || code == '-')) return false;
        }
    }
    return true;
}

QMutex faviconMutex;
QString faviconNamespace;
// Session guards so background work is never repeated: hosts already being
// fetched elsewhere, and hosts that recently failed (negative cache).
QSet<QString> faviconInFlight;
QSet<QString> faviconFailed;

QString faviconCacheFile(const QString &host) {
    const QString base = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
    if (base.isEmpty()) return {};
    QCryptographicHash hash(QCryptographicHash::Sha256);
    hash.addData(faviconNamespace.toUtf8());
    hash.addData(QByteArray(1, '\0'));
    hash.addData(host.toLower().toUtf8());
    return base + QStringLiteral("/favicons/") + QString::fromLatin1(hash.result().toHex())
        + QStringLiteral(".png");
}

// Quote-aware attribute parser: handles name="a b", name='x', name=bare,
// in any order. Naive [^"'s>]+ patterns truncate values like
// rel="shortcut icon" at the first space.
QMap<QString, QString> tagAttributes(const QString &element) {
    QMap<QString, QString> attributes;
    const thread_local QRegularExpression attr(
        QStringLiteral("(\\w[\\w:.-]*)\\s*=\\s*(?:\"([^\"]*)\"|'([^']*)'|([^\\s>]+))"),
        QRegularExpression::CaseInsensitiveOption);
    auto it = attr.globalMatch(element);
    while (it.hasNext()) {
        const auto match = it.next();
        QString value = match.captured(2);
        if (value.isNull()) value = match.captured(3);
        if (value.isNull()) value = match.captured(4);
        attributes.insert(match.captured(1).toLower(), value.trimmed());
    }
    return attributes;
}

// Blocking GET with its own local loop. Only ever run this on plain worker
// threads: nesting a loop inside QtQuick's image loader thread reenters its
// posted-event delivery and segfaults (lockThreadPostEventList).
QByteArray downloadUrl(QNetworkAccessManager &manager, const QUrl &url, int maxBytes, int timeoutMs) {
    QByteArray data;
    if (!url.isValid() || url.scheme() != QStringLiteral("https")) return data;
    QNetworkRequest request(url);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
    // Plain clients get bot-walled (403s even for favicons); identify as a browser.
    request.setHeader(QNetworkRequest::UserAgentHeader, QStringLiteral("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"));
    request.setTransferTimeout(timeoutMs);
    QEventLoop loop;
    QTimer timer;
    timer.setSingleShot(true);
    QObject::connect(&timer, &QTimer::timeout, &loop, &QEventLoop::quit);
    QNetworkReply *reply = manager.get(request);
    QObject::connect(reply, &QNetworkReply::downloadProgress, [&](qint64 received, qint64) {
        if (received > maxBytes) reply->abort();
    });
    QObject::connect(reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);
    timer.start(timeoutMs);
    loop.exec();
    if (reply->error() == QNetworkReply::NoError) {
        data = reply->readAll();
    } else {
        qInfo() << "kryptopass.entryicons: download failed for" << url.host() << url.path() << ":" << reply->errorString();
    }
    reply->deleteLater();
    if (data.size() > maxBytes) data.clear();
    return data;
}

// Finds the site-declared icon via <link rel="icon"> on the site itself.
// Same-domain derivation, no third-party services involved.
QUrl discoverSiteIcon(QNetworkAccessManager &manager, const QString &host) {
    const QUrl page(QStringLiteral("https://") + host + QStringLiteral("/"));
    const QByteArray html = downloadUrl(manager, page, 256 * 1024, 4000);
    if (html.isEmpty()) return {};
    const thread_local QRegularExpression tag(QStringLiteral("<link[^>]*>|<base[^>]*>"),
        QRegularExpression::CaseInsensitiveOption);
    QUrl base = page;
    QUrl fallback;
    auto it = tag.globalMatch(QString::fromUtf8(html.constData(), static_cast<int>(qMin(html.size(), 256 * 1024))));
    while (it.hasNext()) {
        const QString element = it.next().captured(0);
        const auto attributes = tagAttributes(element);
        if (element.startsWith(QStringLiteral("<base"), Qt::CaseInsensitive)) {
            const QUrl candidate(attributes.value(QStringLiteral("href")));
            if (candidate.scheme() == QStringLiteral("https") && validFaviconHost(candidate.host())) base = candidate;
            continue;
        }
        if (!attributes.value(QStringLiteral("rel")).toLower().contains(QStringLiteral("icon"))) continue;
        const QString hrefValue = attributes.value(QStringLiteral("href"));
        if (hrefValue.isEmpty() || hrefValue.startsWith(QStringLiteral("data:"), Qt::CaseInsensitive)) continue;
        // Relative URLs resolve against <base href> when present, else the page.
        const QUrl resolved = base.resolved(QUrl(hrefValue));
        if (resolved.scheme() != QStringLiteral("https") || !validFaviconHost(resolved.host())) continue;
        if (attributes.value(QStringLiteral("rel")).toLower().contains(QStringLiteral("apple-touch-icon"))) return resolved;
        if (resolved.path().endsWith(QStringLiteral(".svg"), Qt::CaseInsensitive)) continue;
        if (!fallback.isValid()) fallback = resolved;
    }
    return fallback;
}

QByteArray downloadFavicon(QNetworkAccessManager &manager, const QString &host) {
    return downloadUrl(manager, QUrl(QStringLiteral("https://") + host + QStringLiteral("/favicon.ico")),
        1024 * 1024, 5000);
}

QImage decodeFavicon(const QByteArray &data) {
    if (data.isEmpty() || data.size() > 1024 * 1024) return {};
    QBuffer buffer(const_cast<QByteArray *>(&data));
    buffer.open(QIODevice::ReadOnly);
    QImageReader reader(&buffer);
    reader.setAutoTransform(true);
    const auto format = reader.format().toLower();
    if (format != "png" && format != "jpeg" && format != "webp" && format != "ico") return {};
    // Real-world .ico files bundle several resolutions and report no usable
    // upfront size, so read first and pick the largest valid entry instead
    // of trusting size()/imageCount() gates that reject every .ico.
    if (format == "ico") {
        QImage best;
        const int entries = reader.imageCount();
        for (int i = 0; i < entries && i < 16; ++i) {
            if (i > 0 && !reader.jumpToImage(i)) break;
            const QImage candidate = reader.read();
            if (candidate.isNull() || candidate.width() > 512 || candidate.height() > 512
                || candidate.width() <= 0 || candidate.height() <= 0) continue;
            if (best.isNull() || candidate.width() > best.width()) best = candidate;
        }
        return best;
    }
    QImage image = reader.read();
    if (image.isNull() || reader.jumpToNextImage()) return {};
    if (image.width() <= 0 || image.height() <= 0 || image.width() > 512 || image.height() > 512
        || qint64(image.width()) * image.height() > 4194304) return {};
    return image;
}

// Disk-only read, safe from any thread: never touches the network.
rust::Vec<std::uint8_t> readDiskFavicon(const QString &host) {
    rust::Vec<std::uint8_t> result;
    const QString path = faviconCacheFile(host);
    if (path.isEmpty()) return result;
    QFile cached(path);
    if (!cached.open(QIODevice::ReadOnly)) return result;
    const QByteArray data = cached.read(512 * 1024 + 1);
    if (data.isEmpty() || data.size() > 512 * 1024
        || !data.startsWith(QByteArray::fromRawData("\x89PNG\r\n\x1a\n", 8))) return result;
    qInfo() << "kryptopass.entryicons: favicon cache hit for" << host;
    result.reserve(data.size());
    for (auto byte : data) result.push_back(static_cast<std::uint8_t>(byte));
    return result;
}

// Disk-first fetch; the downloads run wherever the caller runs, so only call
// this from plain worker threads, never from Qt-managed threads.
rust::Vec<std::uint8_t> fetchAndCacheHost(const QString &host, bool discover) {
    auto cached = readDiskFavicon(host);
    if (!cached.empty()) return cached;
    QNetworkAccessManager manager;
    QByteArray downloaded = downloadFavicon(manager, host);
    if (downloaded.isEmpty() && discover) {
        const QUrl siteIcon = discoverSiteIcon(manager, host);
        if (siteIcon.isValid()) {
            qInfo() << "kryptopass.entryicons: discovered site icon for" << host << "at" << siteIcon.toString();
            downloaded = downloadUrl(manager, siteIcon, 1024 * 1024, 4000);
        }
    }
    auto png = encodeNormalizedPng(renderNormalized(decodeFavicon(downloaded)));
    if (png.empty()) return png;
    {
        QMutexLocker lock(&faviconMutex);
        const QString path = faviconCacheFile(host);
        if (!path.isEmpty()) {
            QDir().mkpath(QFileInfo(path).absolutePath());
            QSaveFile file(path);
            if (file.open(QIODevice::WriteOnly)) {
                file.write(reinterpret_cast<const char *>(png.data()), static_cast<qint64>(png.size()));
                file.commit();
            }
        }
    }
    return png;
}

void releaseInflight(const QString &host, bool failed) {
    QMutexLocker lock(&faviconMutex);
    faviconInFlight.remove(host);
    if (failed) faviconFailed.insert(host);
}

class EntryIconProvider final : public QQuickImageProvider {public:
    EntryIconProvider() : QQuickImageProvider(QQuickImageProvider::Image, ForceAsynchronousImageLoading), cache(16 * 1024 * 1024) {}
    QImage requestImage(const QString &id, QSize *size, const QSize &) override {
        {
            QMutexLocker lock(&mutex);
            if (const auto cached = cache.object(id)) {
                if (size) *size = cached->size();
                return *cached;
            }
        }
        const auto encoded = id.toUtf8();
        const auto bytes = entry_icon_bytes(rust::Str(encoded.constData(), encoded.size()));
        QImage image = decode(rust::Slice<const std::uint8_t>(bytes.data(), bytes.size()), true);
        if (!image.isNull() || !id.startsWith(QStringLiteral("favicon/"))) {
            if (!image.isNull()) {
                QMutexLocker lock(&mutex);
                cache.insert(id, new QImage(image), static_cast<int>(image.sizeInBytes()));
            }
            if (size) *size = image.size();
            return image;
        }
        // Cold favicon: download on a plain worker thread and block-wait here.
        // The wait processes no events, so the loader thread never reenters
        // QtQuick's event delivery (the previous nested QEventLoop segfaulted).
        const QString host = id.mid(QStringLiteral("favicon/").size());
        {
            QMutexLocker lock(&faviconMutex);
            if (faviconFailed.contains(host) || faviconInFlight.contains(host)) {
                if (size) *size = QSize();
                return {};
            }
            faviconInFlight.insert(host);
        }
        qInfo() << "kryptopass.entryicons: cold favicon for" << host << "(blocking loader briefly)";
        // Loader path stays on the cheap direct hit; full discovery runs in prefetch.
        auto pending = std::async(std::launch::async, fetchAndCacheHost, host, false);
        rust::Vec<std::uint8_t> fetched;
        if (pending.wait_for(std::chrono::seconds(6)) == std::future_status::ready) fetched = pending.get();
        image = decode(rust::Slice<const std::uint8_t>(fetched.data(), fetched.size()), true);
        {
            QMutexLocker lock(&faviconMutex);
            faviconInFlight.remove(host);
            if (image.isNull()) faviconFailed.insert(host);
        }
        if (!image.isNull()) {
            QMutexLocker lock(&mutex);
            cache.insert(id, new QImage(image), static_cast<int>(image.sizeInBytes()));
        }
        if (size) *size = image.size();
        return image;
    }
private:
    QMutex mutex;
    QCache<QString, QImage> cache;
};
}

rust::Vec<std::uint8_t> normalize_entry_icon(rust::Slice<const std::uint8_t> bytes) {
    return encodeNormalizedPng(renderNormalized(decode(bytes, false)));
}

bool valid_entry_icon(rust::Slice<const std::uint8_t> bytes) { return !decode(bytes, true).isNull(); }

void install_entry_icon_provider(QQmlEngine &engine, const QString &appId) {
    QMutexLocker lock(&faviconMutex);
    faviconNamespace = appId;
    engine.addImageProvider(QStringLiteral("org.kryptopass.entryicons"), new EntryIconProvider);
}

// Loader-thread entry point (via Rust): disk cache only, never blocks on
// the network. A miss returns empty and the provider downloads on a worker.
rust::Vec<std::uint8_t> fetch_favicon(const QString &host) {
    if (!validFaviconHost(host)) {
        qWarning() << "kryptopass.entryicons: ignoring invalid favicon host";
        return {};
    }
    {
        QMutexLocker lock(&faviconMutex);
        if (faviconFailed.contains(host)) {
            qDebug() << "kryptopass.entryicons: skipping recently failed host" << host;
            return {};
        }
    }
    return readDiskFavicon(host);
}

// Proactive warm-up: every host resolves against the same shared disk cache,
// so the list never fetches one by one while scrolling.
void prefetch_favicons(const QString &hosts) {
    QStringList pending;
    {
        QMutexLocker lock(&faviconMutex);
        for (const auto &host : hosts.split(QChar(','), Qt::SkipEmptyParts)) {
            if (!validFaviconHost(host) || faviconFailed.contains(host) || faviconInFlight.contains(host)) continue;
            faviconInFlight.insert(host);
            pending.append(host);
        }
    }
    if (pending.isEmpty()) return;
    qInfo() << "kryptopass.entryicons: prefetching" << pending.size() << "favicons";
    const int workers = qMin(6, pending.size());
    for (int i = 0; i < workers; ++i) {
        std::thread([pending, i, workers] {
            for (int j = i; j < pending.size(); j += workers) {
                const QString host = pending.at(j);
                const bool failed = fetchAndCacheHost(host, true).empty();
                releaseInflight(host, failed);
            }
        }).detach();
    }
}
}
