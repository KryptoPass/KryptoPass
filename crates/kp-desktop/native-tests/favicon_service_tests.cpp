#include "favicon_service.h"

#include <QBuffer>
#include <QColor>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QHash>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QPointer>
#include <QRegularExpression>
#include <QStringList>
#include <QTemporaryDir>
#include <QTest>
#include <QThread>
#include <cstring>
#include <memory>

using kryptopass::desktop::native::FaviconService;

namespace
{
struct Exchange {
    QUrl url;
    QPointer<QNetworkReply> reply;
    bool aborted = false;
    bool finished = false;
};

class FakeReply final : public QNetworkReply
{
public:
    FakeReply(const QNetworkRequest &request, QNetworkAccessManager::Operation operation, std::shared_ptr<Exchange> exchange, QObject *parent)
        : QNetworkReply(parent)
        , exchange_(std::move(exchange))
    {
        setRequest(request);
        setUrl(request.url());
        setOperation(operation);
        setAttribute(QNetworkRequest::HttpStatusCodeAttribute, 200);
        open(QIODevice::ReadOnly | QIODevice::Unbuffered);
    }

    void abort() override
    {
        if (isFinished())
            return;
        exchange_->aborted = true;
        fail(QNetworkReply::OperationCanceledError);
    }

    void append(const QByteArray &bytes, qint64 reportedProgress = -1)
    {
        if (isFinished())
            return;
        bytes_.append(bytes);
        emit readyRead();
        if (!isFinished())
            emit downloadProgress(reportedProgress < 0 ? bytes_.size() : reportedProgress, -1);
    }

    void succeed(const QByteArray &bytes, const QByteArray &contentType = "image/png")
    {
        if (isFinished())
            return;
        setHeader(QNetworkRequest::ContentTypeHeader, contentType);
        append(bytes);
        if (isFinished())
            return;
        setFinished(true);
        emit finished();
    }

    void fail(QNetworkReply::NetworkError error = QNetworkReply::ContentNotFoundError)
    {
        if (isFinished())
            return;
        setAttribute(QNetworkRequest::HttpStatusCodeAttribute, 404);
        setError(error, QStringLiteral("Simulated network failure"));
        setFinished(true);
        emit errorOccurred(error);
        emit finished();
    }

    qint64 bytesAvailable() const override
    {
        return bytes_.size() - offset_ + QIODevice::bytesAvailable();
    }

    bool isSequential() const override
    {
        return true;
    }

protected:
    qint64 readData(char *destination, qint64 maximum) override
    {
        const qint64 count = qMin(maximum, bytes_.size() - offset_);
        if (count <= 0)
            return isFinished() ? -1 : 0;
        std::memcpy(destination, bytes_.constData() + offset_, static_cast<size_t>(count));
        offset_ += count;
        return count;
    }

private:
    std::shared_ptr<Exchange> exchange_;
    QByteArray bytes_;
    qint64 offset_ = 0;
};

class FakeManager final : public QNetworkAccessManager
{
public:
    QList<std::shared_ptr<Exchange>> exchanges;
    int active = 0;
    int peakActive = 0;

    FakeReply *pending(const QString &host, const QString &path) const
    {
        for (const auto &exchange : exchanges) {
            if (exchange->url.host() == host && exchange->url.path() == path && exchange->reply && !exchange->finished)
                return static_cast<FakeReply *>(exchange->reply.data());
        }
        return nullptr;
    }

protected:
    QNetworkReply *createRequest(Operation operation, const QNetworkRequest &request, QIODevice *) override
    {
        auto exchange = std::make_shared<Exchange>();
        exchange->url = request.url();
        auto *reply = new FakeReply(request, operation, exchange, this);
        exchange->reply = reply;
        exchanges.append(exchange);
        peakActive = qMax(peakActive, ++active);
        connect(reply, &QNetworkReply::finished, this, [this, exchange] {
            if (exchange->finished)
                return;
            exchange->finished = true;
            --active;
        });
        return reply;
    }
};

QByteArray png(const QSize &size = QSize(32, 32))
{
    QImage image(size, QImage::Format_RGBA8888);
    image.fill(QColor(34, 104, 190, 180));
    image.setText(QStringLiteral("Description"), QStringLiteral("Metadata to remove"));
    QByteArray bytes;
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::WriteOnly);
    if (!image.save(&buffer, "PNG"))
        return {};
    return bytes;
}
}

class FaviconServiceTests final : public QObject
{
    Q_OBJECT

private slots:
    void consumersJoinPrefetchAndReceiveItsResult()
    {
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        auto *manager = new FakeManager;
        QList<QImage> results;
        bool callbacksOnOwnerThread = true;
        FaviconService service(cache.path(), manager);
        const auto collect = [&](QImage image) {
            callbacksOnOwnerThread &= QThread::currentThread() == service.thread();
            results.append(image);
        };

        service.request(QStringLiteral("alpha.org"));
        service.request(QStringLiteral("alpha.org"), collect);
        service.request(QStringLiteral("alpha.org"), collect);

        QTRY_COMPARE(manager->exchanges.size(), 1);
        auto *reply = manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico"));
        QVERIFY(reply);
        const QByteArray imageBytes = png();
        QVERIFY(!imageBytes.isEmpty());
        reply->succeed(imageBytes);

        QTRY_COMPARE(results.size(), 2);
        QVERIFY(!results.at(0).isNull());
        QCOMPARE(results.at(0), results.at(1));
        QCOMPARE(manager->exchanges.size(), 1);
        QVERIFY(callbacksOnOwnerThread);
    }

    void queuedHostsRespectConcurrencyLimit()
    {
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        auto *manager = new FakeManager;
        QList<QImage> results;
        FaviconService service(cache.path(), manager);
        const QStringList hosts = {QStringLiteral("alpha.org"),
                                   QStringLiteral("beta.org"),
                                   QStringLiteral("gamma.org"),
                                   QStringLiteral("delta.org"),
                                   QStringLiteral("epsilon.org"),
                                   QStringLiteral("zeta.org"),
                                   QStringLiteral("eta.org")};
        for (const auto &host : hosts)
            service.request(host, [&](QImage image) {
                results.append(image);
            });

        QTRY_COMPARE(manager->exchanges.size(), FaviconService::MaxConcurrent);
        QCOMPARE(manager->active, FaviconService::MaxConcurrent);
        QVERIFY(!manager->pending(hosts.last(), QStringLiteral("/favicon.ico")));
        const QByteArray imageBytes = png();
        auto *first = manager->pending(hosts.first(), QStringLiteral("/favicon.ico"));
        QVERIFY(first);
        first->succeed(imageBytes);

        QTRY_COMPARE(manager->exchanges.size(), hosts.size());
        QCOMPARE(manager->active, FaviconService::MaxConcurrent);
        for (const auto &host : hosts.mid(1)) {
            auto *reply = manager->pending(host, QStringLiteral("/favicon.ico"));
            QVERIFY(reply);
            reply->succeed(imageBytes);
        }

        QTRY_COMPARE(results.size(), hosts.size());
        for (const auto &image : results)
            QVERIFY(!image.isNull());
        QCOMPARE(manager->peakActive, FaviconService::MaxConcurrent);
        QCOMPARE(manager->active, 0);
    }

    void failedHostCanRetryAfterNegativeCacheExpires()
    {
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        auto *manager = new FakeManager;
        QList<QImage> results;
        constexpr int retryDelay = 100;
        FaviconService service(cache.path(), manager, retryDelay);
        const auto collect = [&](QImage image) {
            results.append(image);
        };

        service.request(QStringLiteral("alpha.org"), collect);
        QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico")));
        manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico"))->fail();
        QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/")));
        manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/"))->fail();
        QTRY_COMPARE(results.size(), 1);
        QVERIFY(results.first().isNull());
        const qsizetype failedRequests = manager->exchanges.size();

        service.request(QStringLiteral("alpha.org"), collect);
        QTRY_COMPARE(results.size(), 2);
        QVERIFY(results.last().isNull());
        QCOMPARE(manager->exchanges.size(), failedRequests);

        QTest::qWait(retryDelay + 50);
        service.request(QStringLiteral("alpha.org"), collect);
        QTRY_COMPARE(manager->exchanges.size(), failedRequests + 1);
        auto *retry = manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico"));
        QVERIFY(retry);
        retry->succeed(png());

        QTRY_COMPARE(results.size(), 3);
        QVERIFY(!results.last().isNull());
    }

    void invalidDirectImageDiscoversDeclaredIcon()
    {
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        auto *manager = new FakeManager;
        QList<QImage> results;
        FaviconService service(cache.path(), manager);
        service.request(QStringLiteral("alpha.org"), [&](QImage image) {
            results.append(image);
        });

        QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico")));
        manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico"))->succeed("<html>Not an image</html>", "text/html");
        QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/")));
        QVERIFY(results.isEmpty());
        manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/"))
            ->succeed("<html><head><link href='/assets/site.png' rel='shortcut icon'></head></html>", "text/html");
        QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/assets/site.png")));
        manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/assets/site.png"))->succeed(png());

        QTRY_COMPARE(results.size(), 1);
        QVERIFY(!results.first().isNull());
        QCOMPARE(manager->exchanges.size(), 3);
        for (const auto &exchange : manager->exchanges)
            QCOMPARE(exchange->url.scheme(), QStringLiteral("https"));
    }

    void timeoutAbortsPartialRepliesAndReleasesCapacity()
    {
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        auto *manager = new FakeManager;
        QHash<QString, QImage> results;
        FaviconService service(cache.path(), manager, 60000, 150);
        const QStringList hosts = {QStringLiteral("alpha.org"),
                                   QStringLiteral("beta.org"),
                                   QStringLiteral("gamma.org"),
                                   QStringLiteral("delta.org"),
                                   QStringLiteral("epsilon.org"),
                                   QStringLiteral("zeta.org"),
                                   QStringLiteral("eta.org")};
        for (const auto &host : hosts)
            service.request(host, [&, host](QImage image) {
                results.insert(host, image);
            });

        QTRY_COMPARE(manager->exchanges.size(), FaviconService::MaxConcurrent);
        auto *partial = manager->pending(hosts.first(), QStringLiteral("/favicon.ico"));
        QVERIFY(partial);
        const QByteArray imageBytes = png();
        partial->append(imageBytes.first(imageBytes.size() / 2));

        QTRY_VERIFY_WITH_TIMEOUT(manager->pending(hosts.last(), QStringLiteral("/favicon.ico")), 1000);
        manager->pending(hosts.last(), QStringLiteral("/favicon.ico"))->succeed(imageBytes);
        QTRY_COMPARE_WITH_TIMEOUT(results.size(), hosts.size(), 1000);
        for (const auto &host : hosts.first(FaviconService::MaxConcurrent))
            QVERIFY2(results.value(host).isNull(), qPrintable(host));
        QVERIFY(!results.value(hosts.last()).isNull());
        QCOMPARE(manager->exchanges.size(), hosts.size());
        for (int i = 0; i < FaviconService::MaxConcurrent; ++i)
            QVERIFY(manager->exchanges.at(i)->aborted);
        QVERIFY(manager->peakActive <= FaviconService::MaxConcurrent);
        QCOMPARE(manager->active, 0);
    }

    void closedAbortedRepliesCompleteFallbackAndReleaseCapacity()
    {
        QTest::failOnWarning(QRegularExpression(QStringLiteral("QIODevice::read.*device not open")));
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        auto *manager = new FakeManager;
        QHash<QString, QImage> results;
        FaviconService service(cache.path(), manager);
        const QStringList hosts = {QStringLiteral("alpha.org"),
                                   QStringLiteral("beta.org"),
                                   QStringLiteral("gamma.org"),
                                   QStringLiteral("delta.org"),
                                   QStringLiteral("epsilon.org"),
                                   QStringLiteral("zeta.org"),
                                   QStringLiteral("eta.org")};
        for (const auto &host : hosts)
            service.request(host, [&, host](QImage image) {
                results.insert(host, image);
            });

        QTRY_COMPARE(manager->exchanges.size(), FaviconService::MaxConcurrent);
        auto *direct = manager->pending(hosts.first(), QStringLiteral("/favicon.ico"));
        QVERIFY(direct);
        direct->close();
        direct->abort();

        QTRY_VERIFY(manager->pending(hosts.first(), QStringLiteral("/")));
        QVERIFY(results.isEmpty());
        QVERIFY(!manager->pending(hosts.last(), QStringLiteral("/favicon.ico")));
        auto *fallback = manager->pending(hosts.first(), QStringLiteral("/"));
        fallback->close();
        fallback->abort();

        QTRY_VERIFY(results.contains(hosts.first()));
        QVERIFY(results.value(hosts.first()).isNull());
        QTRY_VERIFY(manager->pending(hosts.last(), QStringLiteral("/favicon.ico")));
        const QByteArray imageBytes = png();
        QVERIFY(!imageBytes.isEmpty());
        for (const auto &host : hosts.mid(1)) {
            auto *reply = manager->pending(host, QStringLiteral("/favicon.ico"));
            QVERIFY(reply);
            reply->succeed(imageBytes);
        }

        QTRY_COMPARE(results.size(), hosts.size());
        for (const auto &host : hosts.mid(1))
            QVERIFY(!results.value(host).isNull());
        QCOMPARE(manager->exchanges.size(), hosts.size() + 1);
        QCOMPARE(manager->peakActive, FaviconService::MaxConcurrent);
        QCOMPARE(manager->active, 0);
    }

    void shutdownCompletesWaitersAndRejectsNewWork()
    {
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        auto *manager = new FakeManager;
        QList<QImage> results;
        FaviconService service(cache.path(), manager);
        const auto collect = [&](QImage image) {
            results.append(image);
        };
        for (int i = 0; i < FaviconService::MaxConcurrent + 1; ++i)
            service.request(QStringLiteral("host%1.org").arg(i), collect);
        service.request(QStringLiteral("host0.org"), collect);
        QTRY_COMPARE(manager->exchanges.size(), FaviconService::MaxConcurrent);

        service.shutdown();
        QTRY_COMPARE(results.size(), FaviconService::MaxConcurrent + 2);
        for (const auto &image : results)
            QVERIFY(image.isNull());
        QCOMPARE(manager->active, 0);
        for (const auto &exchange : manager->exchanges)
            QVERIFY(exchange->aborted);

        service.request(QStringLiteral("new.org"), collect);
        QTRY_COMPARE(results.size(), FaviconService::MaxConcurrent + 3);
        QVERIFY(results.last().isNull());
        service.shutdown();
        QCoreApplication::processEvents();
        QCOMPARE(results.size(), FaviconService::MaxConcurrent + 3);
        QCOMPARE(manager->exchanges.size(), FaviconService::MaxConcurrent);
    }

    void successfulIconsAreNormalizedAndCachedAcrossInstances()
    {
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        QImage normalized;
        {
            auto *manager = new FakeManager;
            QList<QImage> results;
            FaviconService service(cache.path(), manager);
            const auto collect = [&](QImage image) {
                results.append(image);
            };
            service.request(QStringLiteral("alpha.org"), collect);
            QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico")));
            manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico"))->succeed(png(QSize(300, 150)));
            QTRY_COMPARE(results.size(), 1);
            normalized = results.first();
            QCOMPARE(normalized.size(), QSize(256, 128));
            QVERIFY(normalized.textKeys().isEmpty());
            QVERIFY(normalized.hasAlphaChannel());

            service.request(QStringLiteral("alpha.org"), collect);
            QTRY_COMPARE(results.size(), 2);
            QCOMPARE(results.last(), normalized);
            QCOMPARE(manager->exchanges.size(), 1);
        }
        {
            auto *manager = new FakeManager;
            QList<QImage> results;
            FaviconService service(cache.path(), manager);
            service.request(QStringLiteral("alpha.org"), [&](QImage image) {
                results.append(image);
            });
            QTRY_COMPARE(results.size(), 1);
            QCOMPARE(results.first().convertToFormat(QImage::Format_RGBA8888), normalized.convertToFormat(QImage::Format_RGBA8888));
            QCOMPARE(manager->exchanges.size(), 0);
        }
    }

    void repeatedPrefetchAfterPixelEvictionLeavesRecoveryToConsumer()
    {
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        auto *manager = new FakeManager;
        QList<QImage> results;
        FaviconService service(cache.path(), manager);
        const QString firstHost = QStringLiteral("warm0.org");
        const QByteArray imageBytes = png(QSize(256, 256));
        QVERIFY(!imageBytes.isEmpty());
        QString firstCacheFile;

        // Forty distinct icons exceed the 8 MiB pixel budget without keeping them alive in callers.
        for (int i = 0; i < 40; ++i) {
            const QString host = QStringLiteral("warm%1.org").arg(i);
            service.request(host);
            QTRY_VERIFY(manager->pending(host, QStringLiteral("/favicon.ico")));
            manager->pending(host, QStringLiteral("/favicon.ico"))->succeed(imageBytes);
            QTRY_COMPARE(manager->active, 0);
            if (i == 0) {
                const QDir directory(cache.path());
                const auto files = directory.entryList(QDir::Files);
                QCOMPARE(files.size(), 1);
                firstCacheFile = directory.absoluteFilePath(files.first());
            }
        }

        QVERIFY(QFile::remove(firstCacheFile));
        const qsizetype warmedRequests = manager->exchanges.size();
        service.request(firstHost);
        QCoreApplication::processEvents();
        QCOMPARE(manager->exchanges.size(), warmedRequests);
        QCOMPARE(manager->active, 0);

        service.request(firstHost, [&](QImage image) {
            results.append(image);
        });
        QTRY_COMPARE(manager->exchanges.size(), warmedRequests + 1);
        auto *recovery = manager->pending(firstHost, QStringLiteral("/favicon.ico"));
        QVERIFY(recovery);
        QVERIFY(results.isEmpty());
        recovery->succeed(imageBytes);

        QTRY_COMPARE(results.size(), 1);
        QCOMPARE(results.first().size(), QSize(256, 256));
        QCOMPARE(manager->active, 0);
    }

    void oversizedImageIsRejectedAndNotCached()
    {
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        {
            auto *manager = new FakeManager;
            QList<QImage> results;
            FaviconService service(cache.path(), manager);
            service.request(QStringLiteral("alpha.org"), [&](QImage image) {
                results.append(image);
            });
            QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico")));
            manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico"))->succeed(png(QSize(513, 1)));
            QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/")));
            manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/"))->succeed("<html><head></head></html>", "text/html");
            QTRY_COMPARE(results.size(), 1);
            QVERIFY(results.first().isNull());
        }
        {
            auto *manager = new FakeManager;
            FaviconService service(cache.path(), manager);
            service.request(QStringLiteral("alpha.org"));
            QTRY_COMPARE(manager->exchanges.size(), 1);
            QVERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico")));
        }
    }

    void streamedResponseBytesAreBounded_data()
    {
        QTest::addColumn<bool>("html");
        QTest::addColumn<int>("limit");
        QTest::newRow("favicon") << false << 1024 * 1024;
        QTest::newRow("html discovery") << true << 256 * 1024;
    }

    void streamedResponseBytesAreBounded()
    {
        QFETCH(bool, html);
        QFETCH(int, limit);
        QTemporaryDir cache;
        QVERIFY(cache.isValid());
        auto *manager = new FakeManager;
        QList<QImage> results;
        FaviconService service(cache.path(), manager);
        service.request(QStringLiteral("alpha.org"), [&](QImage image) {
            results.append(image);
        });
        QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico")));
        if (html) {
            manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/favicon.ico"))->fail();
            QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/")));
        }
        const QString path = html ? QStringLiteral("/") : QStringLiteral("/favicon.ico");
        auto *reply = manager->pending(QStringLiteral("alpha.org"), path);
        QVERIFY(reply);
        const auto oversized = manager->exchanges.last();
        // A low progress count models Qt delivering more bytes after decoding transfer encodings.
        reply->append(QByteArray(limit + 1, 'x'), 1);
        QTRY_VERIFY(oversized->aborted);
        if (!html) {
            QTRY_VERIFY(manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/")));
            manager->pending(QStringLiteral("alpha.org"), QStringLiteral("/"))->fail();
        }

        QTRY_COMPARE(results.size(), 1);
        QVERIFY(results.first().isNull());
        QCOMPARE(manager->active, 0);
    }
};

QTEST_GUILESS_MAIN(FaviconServiceTests)

#include "favicon_service_tests.moc"
