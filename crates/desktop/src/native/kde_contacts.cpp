#include "native/kde_contacts.h"

#include <Akonadi/ContactEditorDialog>
#include <Akonadi/ContactSearchJob>
#include <Akonadi/EmailAddressSelectionDialog>
#include <Akonadi/EmailAddressSelection>
#include <Akonadi/Item>
#include <Akonadi/ItemModifyJob>
#include <KContacts/Addressee>
#include <KJob>

#include <QApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMessageBox>
#include <QMetaObject>
#include <QProcess>
#include <QPointer>
#include <QRegularExpression>
#include <QWidget>

namespace {

constexpr auto kFingerprintApp = "KADDRESSBOOK";
constexpr auto kFingerprintField = "OPENPGPFP";

QString recordJson(const KContacts::Addressee &contact)
{
    const QJsonObject record{
        {QStringLiteral("uid"), contact.uid()},
        {QStringLiteral("name"), contact.realName()},
        {QStringLiteral("email"), contact.preferredEmail()},
        {QStringLiteral("openPgpFingerprint"), contact.custom(QLatin1String(kFingerprintApp), QLatin1String(kFingerprintField))},
    };
    return QString::fromUtf8(QJsonDocument(record).toJson(QJsonDocument::Compact));
}

void invokeContacts(QObject *receiver, const QStringList &records, const QString &error)
{
    if (!receiver) {
        return;
    }
    QMetaObject::invokeMethod(
        receiver,
        "receiveContacts",
        Qt::QueuedConnection,
        Q_ARG(QStringList, records),
        Q_ARG(QString, error));
}

void invokeOperation(QObject *receiver, bool success, const QString &error)
{
    if (!receiver) {
        return;
    }
    QMetaObject::invokeMethod(
        receiver,
        "receiveOperation",
        Qt::QueuedConnection,
        Q_ARG(bool, success),
        Q_ARG(QString, error));
}

bool isFullFingerprint(const QString &fingerprint)
{
    static const QRegularExpression expression(QStringLiteral("^(?:[0-9A-Fa-f]{40}|[0-9A-Fa-f]{64})$"));
    return expression.match(fingerprint).hasMatch();
}

void showEditor(QObject *receiver, const Akonadi::Item &item, bool creating)
{
    const QPointer<QObject> receiverGuard(receiver);
    auto *dialog = new Akonadi::ContactEditorDialog(
        creating ? Akonadi::ContactEditorDialog::CreateMode : Akonadi::ContactEditorDialog::EditMode,
        QApplication::activeWindow());
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    if (!creating) {
        dialog->setContact(item);
    }

    QObject::connect(dialog, &Akonadi::ContactEditorDialog::contactStored, receiver, [receiverGuard](const Akonadi::Item &) {
        if (receiverGuard) {
            invokeOperation(receiverGuard.data(), true, {});
        }
    });
    QObject::connect(dialog, &Akonadi::ContactEditorDialog::error, receiver, [receiverGuard](const QString &) {
        if (receiverGuard) {
            invokeOperation(receiverGuard.data(), false, QStringLiteral("No se pudo guardar el contacto en KDE Contacts."));
        }
    });
    QObject::connect(receiver, &QObject::destroyed, dialog, &QObject::deleteLater);
    QObject::connect(dialog, &QDialog::rejected, receiver, [receiverGuard] {
        if (receiverGuard) {
            invokeOperation(receiverGuard.data(), false, {});
        }
    });
    dialog->open();
}

void finishLink(QObject *receiver, const Akonadi::Item &item, const QString &fingerprint, const QString &contactName)
{
    const QPointer<QObject> receiverGuard(receiver);
    if (!item.isValid()) {
        invokeOperation(receiver, false, QStringLiteral("El contacto seleccionado ya no está disponible."));
        return;
    }
    if (!item.hasPayload<KContacts::Addressee>()) {
        invokeOperation(receiver, false, QStringLiteral("No se pudieron leer los datos del contacto seleccionado."));
        return;
    }

    KContacts::Addressee contact;
    try {
        contact = item.payload<KContacts::Addressee>();
    } catch (...) {
        invokeOperation(receiver, false, QStringLiteral("No se pudieron leer los datos del contacto seleccionado."));
        return;
    }

    if (contact.uid().isEmpty()) {
        invokeOperation(receiver, false, QStringLiteral("El contacto seleccionado no tiene una identidad válida."));
        return;
    }

    const QString previous = contact.custom(QLatin1String(kFingerprintApp), QLatin1String(kFingerprintField));
    const QString confirmation = previous.isEmpty()
        ? QStringLiteral("¿Vincular %1 con esta huella OpenPGP?\n\n%2").arg(contactName, fingerprint)
        : QStringLiteral("%1 ya tiene una huella OpenPGP vinculada. ¿Reemplazarla por esta?\n\n%2\n\nHuella actual: %3")
              .arg(contactName, fingerprint, previous);
    const auto answer = QMessageBox::question(
        QApplication::activeWindow(), QStringLiteral("Vincular clave OpenPGP"), confirmation);
    if (!receiverGuard) {
        return;
    }
    if (answer != QMessageBox::Yes) {
        invokeOperation(receiverGuard.data(), false, {});
        return;
    }

    contact.insertCustom(QLatin1String(kFingerprintApp), QLatin1String(kFingerprintField), fingerprint);
    Akonadi::Item updated = item;
    updated.setPayload(contact);
    auto *job = new Akonadi::ItemModifyJob(updated, receiverGuard.data());
    job->disableAutomaticConflictHandling();
    QObject::connect(job, &KJob::result, receiverGuard.data(), [receiverGuard, job](KJob *) {
        if (receiverGuard) {
            if (job->error()) {
                invokeOperation(receiverGuard.data(), false, QStringLiteral("No se pudo guardar el vínculo en KDE Contacts."));
            } else {
                invokeOperation(receiverGuard.data(), true, {});
            }
        }
        job->deleteLater();
    });
}

void startContactSearch(QObject *receiver)
{
    if (!receiver) {
        return;
    }
    const QPointer<QObject> receiverGuard(receiver);
    auto *job = new Akonadi::ContactSearchJob(receiver);
    QObject::connect(job, &KJob::result, receiver, [receiverGuard, job](KJob *) {
        if (!receiverGuard) {
            job->deleteLater();
            return;
        }
        QStringList records;
        QString error;
        if (job->error()) {
            error = QStringLiteral("No se pudieron consultar los contactos de KDE.");
        } else {
            const auto contacts = job->contacts();
            records.reserve(contacts.size());
            for (const auto &contact : contacts) {
                if (!contact.uid().isEmpty()) {
                    records.push_back(recordJson(contact));
                }
            }
        }
        invokeContacts(receiverGuard.data(), records, error);
        job->deleteLater();
    });
}

} // namespace

namespace kryptopass::desktop::native {

void refresh_contacts(QObject *receiver)
{
    startContactSearch(receiver);
}

void open_contact_editor(QObject *receiver, const QString &uid)
{
    if (uid.isEmpty()) {
        showEditor(receiver, {}, true);
        return;
    }
    if (!receiver) {
        return;
    }

    const QPointer<QObject> receiverGuard(receiver);
    auto *job = new Akonadi::ContactSearchJob(receiver);
    job->setQuery(Akonadi::ContactSearchJob::ContactUid, uid, Akonadi::ContactSearchJob::ExactMatch);
    QObject::connect(job, &KJob::result, receiver, [receiverGuard, job](KJob *) {
        if (!receiverGuard) {
            job->deleteLater();
            return;
        }
        const auto items = job->items();
        if (job->error() || items.isEmpty()) {
            invokeOperation(receiverGuard.data(), false, QStringLiteral("No se encontró el contacto que quieres editar."));
        } else if (items.size() != 1) {
            invokeOperation(receiverGuard.data(), false, QStringLiteral("El identificador del contacto no es único; no se abrió ningún registro."));
        } else {
            showEditor(receiverGuard.data(), items.at(0), false);
        }
        job->deleteLater();
    });
}

void choose_contact_for_key(QObject *receiver, const QString &fingerprint)
{
    if (!receiver || !isFullFingerprint(fingerprint)) {
        invokeOperation(receiver, false, QStringLiteral("La huella seleccionada no es completa o no es válida."));
        return;
    }

    auto *dialog = new Akonadi::EmailAddressSelectionDialog(QApplication::activeWindow());
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    QObject::connect(receiver, &QObject::destroyed, dialog, &QObject::deleteLater);
    const QPointer<QObject> receiverGuard(receiver);
    QObject::connect(dialog, &QDialog::accepted, receiver, [receiverGuard, dialog, fingerprint] {
        if (!receiverGuard) {
            return;
        }
        const auto selections = dialog->selectedAddresses();
        if (selections.size() != 1) {
            invokeOperation(receiverGuard.data(), false, QStringLiteral("Selecciona un solo contacto para vincular."));
            return;
        }
        const auto selection = selections.constFirst();
        const QString contactName = selection.email().isEmpty()
            ? selection.name()
            : QStringLiteral("%1 <%2>").arg(selection.name(), selection.email());
        finishLink(receiverGuard.data(), selection.item(), fingerprint.toUpper(), contactName);
    });
    QObject::connect(dialog, &QDialog::rejected, receiver, [receiverGuard] {
        if (receiverGuard) {
            invokeOperation(receiverGuard.data(), false, {});
        }
    });
    dialog->open();
}

bool open_contacts()
{
    return QProcess::startDetached(QStringLiteral("kaddressbook"), {});
}

} // namespace kryptopass::desktop::native
