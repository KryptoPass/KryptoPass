#include "file_manager.h"

#include <KIO/OpenFileManagerWindowJob>
#include <QFileInfo>
#include <QMetaObject>

namespace kryptopass::desktop::native
{
void show_in_folder(QObject *receiver, const QUrl &url)
{
    if (!receiver)
        return;

    const QFileInfo file(url.toLocalFile());
    if (!url.isValid() || !url.isLocalFile() || url.hasQuery() || url.hasFragment() || !file.isAbsolute() || !file.isFile()) {
        QMetaObject::invokeMethod(receiver, "receiveFileManagerError", Qt::QueuedConnection);
        return;
    }

    auto *job = new KIO::OpenFileManagerWindowJob(receiver);
    job->setHighlightUrls({url});
    QObject::connect(job, &KJob::result, receiver, [receiver](KJob *finished) {
        if (finished->error())
            QMetaObject::invokeMethod(receiver, "receiveFileManagerError", Qt::QueuedConnection);
    });
    job->start();
}
}
