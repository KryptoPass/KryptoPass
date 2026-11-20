#pragma once

#include <QCache>
#include <QElapsedTimer>
#include <QHash>
#include <QImage>
#include <QNetworkAccessManager>
#include <QObject>
#include <QQueue>
#include <QUrl>
#include <functional>

namespace kryptopass::desktop::native
{
// Owned by the icon worker; all methods and completions run on that thread.
class FaviconService final : public QObject
{
public:
    using Completion = std::function<void(QImage)>;
    static constexpr int MaxConcurrent = 6;

    explicit FaviconService(QString cacheDirectory,
                            QNetworkAccessManager *manager = nullptr,
                            int retryDelayMs = 60000,
                            int timeoutMs = 5000,
                            QString cacheNamespace = {});
    void request(const QString &host, Completion completion = {});
    void shutdown();

private:
    struct Pending {
        QList<Completion> completions;
        bool active = false;
        qint64 deadline = 0;
    };
    using DownloadCompletion = std::function<void(QByteArray, QUrl)>;

    void pump();
    void download(const QString &host, const QUrl &url, int maxBytes, DownloadCompletion completion);
    void discover(const QString &host);
    void complete(const QString &host, const QImage &image, bool persist = true);
    QImage readCache(const QString &host);
    void writeCache(const QString &host, const QImage &image);
    QString cacheFile(const QString &host) const;

    const QString cacheDirectory_;
    const QString cacheNamespace_;
    QNetworkAccessManager *manager_;
    const int retryDelayMs_;
    const int timeoutMs_;
    QElapsedTimer clock_;
    QCache<QString, QImage> images_{8 * 1024 * 1024};
    QCache<QString, qint64> retryAfter_{1024};
    QCache<QString, bool> warmedHosts_{16384};
    QHash<QString, Pending> pending_;
    QQueue<QString> queue_;
    int active_ = 0;
    bool stopping_ = false;
    bool pumping_ = false;
};
}
