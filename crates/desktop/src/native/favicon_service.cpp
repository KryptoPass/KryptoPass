#include "favicon_service.h"
#include "entry_icon_image.h"

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QMap>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QRegularExpression>
#include <QSaveFile>
#include <QScopedValueRollback>
#include <QTimer>
#include <memory>

namespace kryptopass::desktop::native {
namespace {
bool validHost(const QString &host) {
    static const QRegularExpression pattern(QStringLiteral(
        "^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\\.)+[a-z]{2,63}$"));
    if (host.size() > 253 || !pattern.match(host).hasMatch()) return false;
    const auto suffix = host.section(QChar('.'), -1);
    const QStringList reserved{QStringLiteral("localhost"), QStringLiteral("local"),
        QStringLiteral("internal"), QStringLiteral("test"), QStringLiteral("invalid"),
        QStringLiteral("example"), QStringLiteral("onion")};
    return !reserved.contains(suffix) && host != QStringLiteral("example.com")
        && host != QStringLiteral("example.net") && host != QStringLiteral("example.org");
}

bool validUrl(const QUrl &url) {
    return url.isValid() && url.scheme() == QStringLiteral("https")
        && validHost(QString::fromLatin1(QUrl::toAce(url.host())))
        && url.userInfo().isEmpty() && url.port() == -1;
}

QMap<QString, QString> tagAttributes(const QString &element) {
    static const QRegularExpression attr(
        QStringLiteral("(\\w[\\w:.-]*)\\s*=\\s*(?:\"([^\"]*)\"|'([^']*)'|([^\\s>]+))"));
    QMap<QString, QString> attributes;
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

QUrl siteIcon(const QByteArray &html, const QUrl &page) {
    static const QRegularExpression tag(QStringLiteral("<link\\b[^>]*>|<base\\b[^>]*>"),
        QRegularExpression::CaseInsensitiveOption);
    static const QRegularExpression whitespace(QStringLiteral("\\s+"));
    QUrl base = page;
    QUrl fallback;
    auto it = tag.globalMatch(QString::fromUtf8(html));
    while (it.hasNext()) {
        const auto element = it.next().captured();
        const auto attributes = tagAttributes(element);
        const auto href = attributes.value(QStringLiteral("href"));
        if (href.isEmpty()) continue;
        if (element.startsWith(QStringLiteral("<base"), Qt::CaseInsensitive)) {
            const auto candidate = page.resolved(QUrl(href));
            if (validUrl(candidate)) base = candidate;
            continue;
        }
        const auto rel = attributes.value(QStringLiteral("rel")).toLower().split(whitespace, Qt::SkipEmptyParts);
        const bool apple = rel.contains(QStringLiteral("apple-touch-icon"))
            || rel.contains(QStringLiteral("apple-touch-icon-precomposed"));
        if (!apple && !rel.contains(QStringLiteral("icon"))) continue;
        const auto resolved = base.resolved(QUrl(href));
        if (!validUrl(resolved) || resolved.path().endsWith(QStringLiteral(".svg"), Qt::CaseInsensitive)) continue;
        if (apple) return resolved;
        if (fallback.isEmpty()) fallback = resolved;
    }
    return fallback;
}
}

FaviconService::FaviconService(QString cacheDirectory, QNetworkAccessManager *manager,
                             int retryDelayMs, int timeoutMs, QString cacheNamespace)
    : cacheDirectory_(std::move(cacheDirectory)), cacheNamespace_(std::move(cacheNamespace)),
      manager_(manager ? manager : new QNetworkAccessManager),
      retryDelayMs_(qMax(0, retryDelayMs)), timeoutMs_(qMax(1, timeoutMs)) {
    manager_->setParent(this);
    clock_.start();
}

void FaviconService::request(const QString &host, Completion completion) {
    if (stopping_ || !validHost(host)) {
        if (completion) completion({});
        return;
    }
    // Pixel eviction must not restart speculative disk reads on every list update.
    if (!completion && warmedHosts_.contains(host)) return;
    if (const auto image = images_.object(host)) {
        if (completion) completion(*image);
        return;
    }
    if (const auto retry = retryAfter_.object(host); retry && *retry > clock_.elapsed()) {
        if (completion) completion({});
        return;
    }
    auto existing = pending_.find(host);
    if (existing != pending_.end()) {
        if (completion) {
            existing->completions.append(std::move(completion));
            // Visible icons take precedence over queued speculative prefetch.
            if (!existing->active) {
                queue_.removeOne(host);
                queue_.prepend(host);
            }
        }
        return;
    }
    Pending pending;
    if (completion) {
        pending.completions.append(std::move(completion));
        queue_.prepend(host);
    } else {
        queue_.enqueue(host);
    }
    pending_.insert(host, std::move(pending));
    pump();
}

void FaviconService::pump() {
    if (stopping_ || pumping_) return;
    QScopedValueRollback guard(pumping_, true);
    while (!stopping_ && active_ < MaxConcurrent && !queue_.isEmpty()) {
        const auto host = queue_.dequeue();
        auto &pending = pending_[host];
        pending.active = true;
        pending.deadline = clock_.elapsed() + timeoutMs_;
        ++active_;
        const auto cached = readCache(host);
        if (!cached.isNull()) {
            complete(host, cached, false);
            continue;
        }
        download(host, QUrl(QStringLiteral("https://") + host + QStringLiteral("/favicon.ico")),
            1024 * 1024, [this, host](QByteArray bytes, QUrl) {
                const auto image = normalizeEntryIcon(decodeFavicon(bytes));
                if (image.isNull()) discover(host);
                else complete(host, image);
            });
    }
}

void FaviconService::download(const QString &host, const QUrl &url, int maxBytes, DownloadCompletion completion) {
    if (stopping_) return;
    const auto pending = pending_.constFind(host);
    if (pending == pending_.cend()) return;
    const auto remaining = pending->deadline - clock_.elapsed();
    if (!validUrl(url) || remaining <= 0) {
        completion({}, url);
        return;
    }
    QNetworkRequest request(url);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
    request.setMaximumRedirectsAllowed(5);
    request.setAttribute(QNetworkRequest::CookieLoadControlAttribute, QNetworkRequest::Manual);
    request.setAttribute(QNetworkRequest::CookieSaveControlAttribute, QNetworkRequest::Manual);
    request.setHeader(QNetworkRequest::UserAgentHeader, QStringLiteral(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/131.0.0.0 Safari/537.36"));
    request.setTransferTimeout(static_cast<int>(remaining));
    auto *reply = manager_->get(request);
    reply->setReadBufferSize(maxBytes + 1);
    struct Download {
        QByteArray bytes;
        bool tooLarge = false;
    };
    const auto state = std::make_shared<Download>();
    const auto drain = [reply, state, maxBytes] {
        if (state->tooLarge || !reply->isOpen() || !reply->isReadable()) return;
        state->bytes.append(reply->read(maxBytes - state->bytes.size() + 1));
        if (state->bytes.size() > maxBytes) {
            state->tooLarge = true;
            state->bytes.clear();
            reply->abort();
        }
    };
    auto *timer = new QTimer(reply);
    timer->setSingleShot(true);
    timer->setTimerType(Qt::PreciseTimer);
    connect(timer, &QTimer::timeout, reply, &QNetworkReply::abort);
    connect(reply, &QIODevice::readyRead, this, drain);
    connect(reply, &QNetworkReply::finished, this,
        [reply, state, timer, drain, completion = std::move(completion)] {
            timer->stop();
            drain();
            const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            if (state->tooLarge || reply->error() != QNetworkReply::NoError || status < 200 || status >= 300)
                state->bytes.clear();
            const auto finalUrl = reply->url();
            reply->deleteLater();
            completion(std::move(state->bytes), finalUrl);
        });
    timer->start(static_cast<int>(remaining));
}

void FaviconService::discover(const QString &host) {
    download(host, QUrl(QStringLiteral("https://") + host + QStringLiteral("/")), 256 * 1024,
        [this, host](QByteArray html, QUrl page) {
            const auto icon = siteIcon(html, page);
            if (icon.isEmpty()) {
                complete(host, {});
                return;
            }
            download(host, icon, 1024 * 1024, [this, host](QByteArray bytes, QUrl) {
                complete(host, normalizeEntryIcon(decodeFavicon(bytes)));
            });
        });
}

void FaviconService::complete(const QString &host, const QImage &image, bool persist) {
    auto found = pending_.find(host);
    if (found == pending_.end()) return;
    const auto callbacks = std::move(found->completions);
    if (found->active) --active_;
    pending_.erase(found);
    if (image.isNull()) {
        warmedHosts_.remove(host);
        retryAfter_.insert(host, new qint64(clock_.elapsed() + retryDelayMs_));
    } else {
        warmedHosts_.insert(host, new bool(true));
        retryAfter_.remove(host);
        images_.insert(host, new QImage(image), static_cast<int>(image.sizeInBytes()));
        if (persist) writeCache(host, image);
    }
    for (const auto &callback : callbacks) callback(image);
    pump();
}

QString FaviconService::cacheFile(const QString &host) const {
    if (cacheDirectory_.isEmpty()) return {};
    QCryptographicHash hash(QCryptographicHash::Sha256);
    hash.addData(cacheNamespace_.toUtf8());
    hash.addData(QByteArray(1, '\0'));
    hash.addData(host.toUtf8());
    return cacheDirectory_ + QChar('/') + QString::fromLatin1(hash.result().toHex()) + QStringLiteral(".png");
}

QImage FaviconService::readCache(const QString &host) {
    const auto path = cacheFile(host);
    if (path.isEmpty()) return {};
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) return {};
    const auto image = decodeEntryIcon(file.read(512 * 1024 + 1), true);
    if (image.isNull()) file.remove();
    return image;
}

void FaviconService::writeCache(const QString &host, const QImage &image) {
    const auto path = cacheFile(host);
    if (path.isEmpty()) return;
    const auto bytes = encodeEntryIcon(image);
    if (bytes.isEmpty() || !QDir().mkpath(cacheDirectory_)) return;
    QSaveFile file(path);
    if (file.open(QIODevice::WriteOnly) && file.write(bytes) == bytes.size()) file.commit();
}

void FaviconService::shutdown() {
    stopping_ = true;
    const auto pending = std::move(pending_);
    pending_.clear();
    queue_.clear();
    active_ = 0;
    const auto replies = manager_->findChildren<QNetworkReply *>();
    for (auto *reply : replies) {
        reply->disconnect(this);
        reply->abort();
        reply->deleteLater();
    }
    for (const auto &request : pending)
        for (const auto &callback : request.completions) callback({});
}
}
