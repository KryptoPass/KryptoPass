#include "native/sensitive_clipboard.h"

#include <QByteArray>
#include <QClipboard>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QMimeData>
#include <QPointer>
#include <QSettings>
#include <QTimer>
#include <QUuid>

#include <algorithm>

namespace {

constexpr int kDefaultAutoClearSeconds = 30;
constexpr int kMaximumAutoClearSeconds = 24 * 60 * 60;
constexpr auto kAutoClearSettingsKey = "security/clipboardAutoClearSeconds";
constexpr auto kOwnedTokenMimeType = "application/x-kryptopass-owned-clipboard-token";
constexpr auto kPasswordManagerHintMimeType = "x-kde-passwordManagerHint";

int boundedSeconds(int seconds)
{
    return std::clamp(seconds, 0, kMaximumAutoClearSeconds);
}

void erase(QString &value)
{
    if (value.isNull()) {
        return;
    }

    value.detach();
    auto *bytes = reinterpret_cast<volatile unsigned char *>(value.data());
    const qsizetype byteCount = value.size() * qsizetype(sizeof(QChar));
    for (qsizetype index = 0; index < byteCount; ++index) {
        bytes[index] = 0;
    }
    value.clear();
    value.squeeze();
}

void erase(QByteArray &value)
{
    if (value.isEmpty()) {
        return;
    }

    volatile char *bytes = value.data();
    for (qsizetype index = 0; index < value.size(); ++index) {
        bytes[index] = '\0';
    }
    value.clear();
    value.squeeze();
}

class ClipboardGuard final : public QObject
{
public:
    explicit ClipboardGuard(QClipboard *clipboard, QObject *parent)
        : QObject(parent)
        , clipboard_(clipboard)
        , timer_(this)
    {
        timer_.setSingleShot(true);
        QObject::connect(&timer_, &QTimer::timeout, this, [this] {
            clearIfStillOwned();
        });
        QObject::connect(clipboard_, &QClipboard::dataChanged, this, [this] {
            if (hasOwnedContents_ && !clipboardMatchesOwnedContents()) {
                discardOwnership();
            }
        });
        if (auto *application = QCoreApplication::instance()) {
            QObject::connect(application, &QCoreApplication::aboutToQuit, this, [this] {
                clearOwned();
            });
        }
    }

    bool copySensitive(const QString &text, int autoClearSeconds)
    {
        if (!clipboard_ || text.isEmpty()) {
            return false;
        }

        // Qt and the window system may make their own copies of clipboard data.
        // We wipe our local cached copy and replace the current clipboard contents,
        // but cannot guarantee erasure from external clients that ignore the hint.

        timer_.stop();
        discardOwnership();

        ownedText_ = text;
        ownedToken_ = QUuid::createUuid().toString(QUuid::WithoutBraces).toLatin1();
        hasOwnedContents_ = true;

        auto *mimeData = new QMimeData;
        mimeData->setText(text);
#if defined(Q_OS_UNIX)
        // This is the established KDE/Plasma hint consumed by Klipper.
        mimeData->setData(kPasswordManagerHintMimeType, QByteArrayLiteral("secret"));
#endif
        mimeData->setData(kOwnedTokenMimeType, ownedToken_);
        clipboard_->setMimeData(mimeData, QClipboard::Clipboard);

        if (!clipboardMatchesOwnedContents()) {
            discardOwnership();
            return false;
        }

        const int seconds = boundedSeconds(autoClearSeconds);
        if (seconds > 0) {
            timer_.start(seconds * 1000);
        }
        return true;
    }

    void clearOwned()
    {
        timer_.stop();
        if (clipboardMatchesOwnedContents()) {
            // Overwrite with empty plain text instead of QClipboard::clear(), which may
            // restore an earlier selection on some X11 clipboard-manager setups.
            auto *mimeData = new QMimeData;
            mimeData->setText(QString{});
            discardOwnership();
            clipboard_->setMimeData(mimeData, QClipboard::Clipboard);
        } else {
            discardOwnership();
        }
    }

    void setAutoClearSeconds(int seconds)
    {
        const int bounded = boundedSeconds(seconds);
        timer_.stop();
        if (hasOwnedContents_ && bounded > 0 && clipboardMatchesOwnedContents()) {
            timer_.start(bounded * 1000);
        }
    }

private:
    bool clipboardMatchesOwnedContents() const
    {
        if (!clipboard_ || !hasOwnedContents_ || ownedToken_.isEmpty()) {
            return false;
        }

        const QMimeData *mimeData = clipboard_->mimeData(QClipboard::Clipboard);
        return mimeData && mimeData->hasText()
            && mimeData->data(QLatin1String(kOwnedTokenMimeType)) == ownedToken_
            && mimeData->text() == ownedText_;
    }

    void clearIfStillOwned()
    {
        if (!clipboardMatchesOwnedContents()) {
            discardOwnership();
            return;
        }

        auto *mimeData = new QMimeData;
        mimeData->setText(QString{});
        discardOwnership();
        clipboard_->setMimeData(mimeData, QClipboard::Clipboard);
    }

    void discardOwnership()
    {
        hasOwnedContents_ = false;
        erase(ownedText_);
        erase(ownedToken_);
    }

    QPointer<QClipboard> clipboard_;
    QTimer timer_;
    QString ownedText_;
    QByteArray ownedToken_;
    bool hasOwnedContents_ = false;
};

ClipboardGuard *guard()
{
    static QPointer<ClipboardGuard> instance;
    if (instance) {
        return instance;
    }

    auto *application = QGuiApplication::instance();
    if (!application) {
        return nullptr;
    }

    auto *clipboard = QGuiApplication::clipboard();
    if (!clipboard) {
        return nullptr;
    }

    instance = new ClipboardGuard(clipboard, application);
    return instance;
}

} // namespace

namespace kryptopass::desktop::native {

bool copy_sensitive(const QString &text, int auto_clear_seconds)
{
    auto *clipboardGuard = guard();
    return clipboardGuard && clipboardGuard->copySensitive(text, auto_clear_seconds);
}

void clear_owned()
{
    if (auto *clipboardGuard = guard()) {
        clipboardGuard->clearOwned();
    }
}

int auto_clear_seconds()
{
    QSettings settings;
    bool ok = false;
    const int configured = settings.value(QLatin1String(kAutoClearSettingsKey), kDefaultAutoClearSeconds).toInt(&ok);
    return ok ? boundedSeconds(configured) : kDefaultAutoClearSeconds;
}

void set_auto_clear_seconds(int seconds)
{
    QSettings settings;
    settings.setValue(QLatin1String(kAutoClearSettingsKey), boundedSeconds(seconds));
    settings.sync();
    if (auto *clipboardGuard = guard()) {
        clipboardGuard->setAutoClearSeconds(seconds);
    }
}

} // namespace kryptopass::desktop::native
