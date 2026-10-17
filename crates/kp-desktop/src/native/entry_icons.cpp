#include "entry_icons.h"
#include "entry_icon_image.h"
#include "favicon_service.h"

#include <QCache>
#include <QMutex>
#include <QMutexLocker>
#include <QQuickAsyncImageProvider>
#include <QQuickImageResponse>
#include <QQuickTextureFactory>
#include <QStandardPaths>
#include <QThread>

namespace kryptopass::desktop::native
{
namespace
{
QByteArray byteArray(rust::Slice<const std::uint8_t> bytes)
{
    return QByteArray::fromRawData(reinterpret_cast<const char *>(bytes.data()), static_cast<qsizetype>(bytes.size()));
}

rust::Vec<std::uint8_t> rustBytes(const QByteArray &bytes)
{
    rust::Vec<std::uint8_t> result;
    result.reserve(bytes.size());
    for (const auto byte : bytes)
        result.push_back(static_cast<std::uint8_t>(byte));
    return result;
}

class IconResponse final : public QQuickImageResponse
{
public:
    explicit IconResponse(QSize requestedSize)
        : requestedSize_(requestedSize)
    {
    }

    QQuickTextureFactory *textureFactory() const override
    {
        return QQuickTextureFactory::textureFactoryForImage(image_);
    }

    QString errorString() const override
    {
        return image_.isNull() ? QStringLiteral("Entry icon unavailable") : QString();
    }

    void complete(QImage image)
    {
        if (!image.isNull() && requestedSize_.isValid())
            image = image.scaled(requestedSize_.boundedTo(image.size()), Qt::KeepAspectRatio, Qt::SmoothTransformation);
        image_ = std::move(image);
        // Qt may destroy the response after this signal; it must be the last action.
        emit finished();
    }

private:
    const QSize requestedSize_;
    QImage image_;
};

class IconWorker final : public QObject
{
public:
    explicit IconWorker(const QString &appId)
    {
        const auto base = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
        favicons_ = new FaviconService(base.isEmpty() ? QString() : base + QStringLiteral("/favicons"), nullptr, 60000, 5000, appId);
        favicons_->setParent(this);
    }

    void load(const QString &id, IconResponse *response)
    {
        const auto prefix = QStringLiteral("favicon/");
        if (id.startsWith(prefix)) {
            favicons_->request(id.mid(prefix.size()), [response](QImage image) {
                response->complete(std::move(image));
            });
            return;
        }
        if (const auto cached = images_.object(id)) {
            response->complete(*cached);
            return;
        }
        const auto encoded = id.toUtf8();
        const auto bytes = entry_icon_bytes(rust::Str(encoded.constData(), encoded.size()));
        const auto image = decodeEntryIcon(byteArray({bytes.data(), bytes.size()}), true);
        if (!image.isNull())
            images_.insert(id, new QImage(image), static_cast<int>(image.sizeInBytes()));
        response->complete(image);
    }

    void prefetch(const QStringList &hosts)
    {
        for (const auto &host : hosts)
            favicons_->request(host);
    }

    void shutdown()
    {
        favicons_->shutdown();
    }

private:
    FaviconService *favicons_;
    QCache<QString, QImage> images_{8 * 1024 * 1024};
};

QMutex providerMutex;
IconWorker *activeWorker = nullptr;

class EntryIconProvider final : public QQuickAsyncImageProvider
{
public:
    explicit EntryIconProvider(const QString &appId)
        : worker_(new IconWorker(appId))
    {
        worker_->moveToThread(&thread_);
        QObject::connect(&thread_, &QThread::finished, worker_, &QObject::deleteLater);
        thread_.setObjectName(QStringLiteral("Entry icons"));
        thread_.start();
        QMutexLocker lock(&providerMutex);
        activeWorker = worker_;
    }

    ~EntryIconProvider() override
    {
        {
            QMutexLocker lock(&providerMutex);
            if (activeWorker == worker_)
                activeWorker = nullptr;
        }
        // Complete subscribers before stopping the event loop that owns their responses.
        QMetaObject::invokeMethod(
            worker_,
            [this] {
                worker_->shutdown();
            },
            Qt::BlockingQueuedConnection);
        thread_.quit();
        thread_.wait();
    }

    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &requestedSize) override
    {
        auto *response = new IconResponse(requestedSize);
        response->moveToThread(&thread_);
        QMetaObject::invokeMethod(
            worker_,
            [worker = worker_, id, response] {
                worker->load(id, response);
            },
            Qt::QueuedConnection);
        return response;
    }

private:
    QThread thread_;
    IconWorker *worker_;
};
}

rust::Vec<std::uint8_t> normalize_entry_icon(rust::Slice<const std::uint8_t> bytes)
{
    return rustBytes(encodeEntryIcon(normalizeEntryIcon(decodeEntryIcon(byteArray(bytes), false))));
}

bool valid_entry_icon(rust::Slice<const std::uint8_t> bytes)
{
    return !decodeEntryIcon(byteArray(bytes), true).isNull();
}

void install_entry_icon_provider(QQmlEngine &engine, const QString &appId)
{
    engine.addImageProvider(QStringLiteral("org.kryptopass.entryicons"), new EntryIconProvider(appId));
}

void prefetch_favicons(const QStringList &hosts)
{
    QMutexLocker lock(&providerMutex);
    if (activeWorker) {
        QMetaObject::invokeMethod(
            activeWorker,
            [worker = activeWorker, hosts] {
                worker->prefetch(hosts);
            },
            Qt::QueuedConnection);
    }
}
}
