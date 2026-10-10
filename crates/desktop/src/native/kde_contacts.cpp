#include "native/kde_contacts.h"

#include <Akonadi/ContactEditorDialog>
#include <Akonadi/ContactSearchJob>
#include <Akonadi/Item>
#include <Akonadi/ItemModifyJob>
#include <KContacts/Addressee>
#include <KJob>

#include <QApplication>
#include <QAbstractItemView>
#include <QDialog>
#include <QDialogButtonBox>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMessageBox>
#include <QMetaObject>
#include <QProcess>
#include <QPointer>
#include <QPushButton>
#include <QRegularExpression>
#include <QSet>
#include <QTimer>
#include <QVBoxLayout>
#include <QWidget>

namespace {

constexpr auto kFingerprintApp = "KADDRESSBOOK";
constexpr auto kFingerprintField = "OPENPGPFP";

bool isFullFingerprint(const QString &fingerprint)
{
    static const QRegularExpression expression(QStringLiteral("^(?:[0-9A-Fa-f]{40}|[0-9A-Fa-f]{64})$"));
    return expression.match(fingerprint).hasMatch();
}

QStringList normalizedFingerprints(const QString &encodedFingerprints)
{
    QStringList fingerprints;
    QSet<QString> seen;
    const auto values = encodedFingerprints.split(QLatin1Char(','), Qt::SkipEmptyParts);
    fingerprints.reserve(values.size());
    for (const QString &value : values) {
        const QString fingerprint = value.trimmed().toUpper();
        if (isFullFingerprint(fingerprint) && !seen.contains(fingerprint)) {
            seen.insert(fingerprint);
            fingerprints.push_back(fingerprint);
        }
    }
    return fingerprints;
}

const QRegularExpression &canonicalFingerprintLineExpression()
{
    static const QRegularExpression expression(
        QStringLiteral("^[\\t ]*OpenPGP:[\\t ]+([A-Fa-f0-9]{40}|[A-Fa-f0-9]{64})[\\t ]*$"),
        QRegularExpression::CaseInsensitiveOption);
    return expression;
}

const QRegularExpression &openPgpFingerprintUriLineExpression()
{
    static const QRegularExpression expression(
        QStringLiteral("^[\\t ]*openpgp4fpr:([A-Fa-f0-9]{40})[\\t ]*$"),
        QRegularExpression::CaseInsensitiveOption);
    return expression;
}

QString completeMarkerFingerprint(const QString &line)
{
    const auto canonicalMatch = canonicalFingerprintLineExpression().match(line);
    if (canonicalMatch.hasMatch()) {
        return canonicalMatch.captured(1).toUpper();
    }

    const auto uriMatch = openPgpFingerprintUriLineExpression().match(line);
    return uriMatch.hasMatch() ? uriMatch.captured(1).toUpper() : QString();
}

QStringList noteFingerprints(const QString &notes)
{
    QStringList fingerprints;
    QSet<QString> seen;
    const auto append = [&fingerprints, &seen](const QString &value) {
        const QString fingerprint = value.toUpper();
        if (isFullFingerprint(fingerprint) && !seen.contains(fingerprint)) {
            seen.insert(fingerprint);
            fingerprints.push_back(fingerprint);
        }
    };

    const auto lines = notes.split(QRegularExpression(QStringLiteral("\\r\\n|\\n|\\r")), Qt::KeepEmptyParts);
    for (const QString &line : lines) {
        const QString fingerprint = completeMarkerFingerprint(line);
        if (!fingerprint.isEmpty()) {
            append(fingerprint);
        }
    }

    // The URI form is deliberately accepted only as a complete token. Its
    // fingerprint is exactly 40 hex digits; URL suffixes and 64-digit values
    // must not be partially interpreted as this compatibility format.
    static const QRegularExpression uriExpression(
        QStringLiteral("(^|[\\s<\\[(\"',;])openpgp4fpr:([A-Fa-f0-9]{40})"
                       "(?=$|[\\s>\\]),;\"']|\\.(?=$|\\s))"),
        QRegularExpression::CaseInsensitiveOption);
    auto matches = uriExpression.globalMatch(notes);
    while (matches.hasNext()) {
        append(matches.next().captured(2));
    }
    return fingerprints;
}

QStringList contactFingerprints(const KContacts::Addressee &contact)
{
    QStringList fingerprints = normalizedFingerprints(
        contact.custom(QLatin1String(kFingerprintApp), QLatin1String(kFingerprintField)));
    QSet<QString> seen(fingerprints.cbegin(), fingerprints.cend());
    for (const QString &fingerprint : noteFingerprints(contact.note())) {
        if (!seen.contains(fingerprint)) {
            seen.insert(fingerprint);
            fingerprints.push_back(fingerprint);
        }
    }
    return fingerprints;
}

QString notesWithFingerprintBlock(const QString &notes, const QStringList &fingerprints)
{
    QString preserved;
    QString preferredLineEnding = QStringLiteral("\n");
    qsizetype lineStart = 0;
    for (qsizetype index = 0; index < notes.size();) {
        if (notes.at(index) != QLatin1Char('\n') && notes.at(index) != QLatin1Char('\r')) {
            ++index;
            continue;
        }

        QString lineEnding(1, notes.at(index));
        if (notes.at(index) == QLatin1Char('\r') && index + 1 < notes.size()
            && notes.at(index + 1) == QLatin1Char('\n')) {
            lineEnding = QStringLiteral("\r\n");
        }
        const QString line = notes.mid(lineStart, index - lineStart);
        if (completeMarkerFingerprint(line).isEmpty()) {
            preserved += line + lineEnding;
        }
        preferredLineEnding = lineEnding;
        index += lineEnding.size();
        lineStart = index;
    }
    if (lineStart < notes.size()) {
        const QString lastLine = notes.mid(lineStart);
        if (completeMarkerFingerprint(lastLine).isEmpty()) {
            preserved += lastLine;
        }
    }

    // Normalize only the trailing boundary before adding the marker block.
    while (preserved.endsWith(QLatin1Char('\n')) || preserved.endsWith(QLatin1Char('\r'))) {
        preserved.chop(1);
    }
    if (!preserved.isEmpty()) {
        preserved += preferredLineEnding + preferredLineEnding;
    }
    for (qsizetype index = 0; index < fingerprints.size(); ++index) {
        if (index > 0) {
            preserved += preferredLineEnding;
        }
        preserved += QStringLiteral("OpenPGP: %1").arg(fingerprints.at(index));
    }
    return preserved;
}

QString recordJson(const KContacts::Addressee &contact)
{
    const QStringList fingerprints = contactFingerprints(contact);
    QJsonArray fingerprintArray;
    for (const QString &fingerprint : fingerprints) {
        fingerprintArray.append(fingerprint);
    }

    const QJsonObject record{
        {QStringLiteral("uid"), contact.uid()},
        {QStringLiteral("name"), contact.realName()},
        {QStringLiteral("email"), contact.preferredEmail()},
        {QStringLiteral("openPgpFingerprint"), fingerprints.value(0)},
        {QStringLiteral("openPgpFingerprints"), fingerprintArray},
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
    const QStringList fieldFingerprints = normalizedFingerprints(previous);
    const QStringList detectedFingerprints = contactFingerprints(contact);
    QStringList fingerprintsForNotes = detectedFingerprints;
    const QString normalizedFingerprint = fingerprint.toUpper();
    if (!fingerprintsForNotes.contains(normalizedFingerprint)) {
        fingerprintsForNotes.push_back(normalizedFingerprint);
    }
    const QString updatedField = fieldFingerprints.contains(normalizedFingerprint)
        ? previous
        : (previous.isEmpty() ? normalizedFingerprint : previous + QLatin1Char(',') + normalizedFingerprint);
    const QString updatedNotes = notesWithFingerprintBlock(contact.note(), fingerprintsForNotes);
    const QString existingDisplay = detectedFingerprints.isEmpty()
        ? QStringLiteral("(ninguna)")
        : detectedFingerprints.join(QStringLiteral(", "));
    const QString confirmation = QStringLiteral(
        "¿Vincular esta huella con el contacto?\n\nContacto: %1\nHuella: %2\n\n"
        "KryptoPass añadirá la huella a las preferencias de KDE si falta y mantendrá un bloque de huellas al final de las notas para facilitar la sincronización. "
        "Conservará el resto del contenido y las preferencias existentes.\n\n"
        "Huella(s) detectadas:\n%3")
                                    .arg(contactName, normalizedFingerprint, existingDisplay);
    const auto answer = QMessageBox::question(
        QApplication::activeWindow(), QStringLiteral("Añadir huella OpenPGP"), confirmation);
    if (!receiverGuard) {
        return;
    }
    if (answer != QMessageBox::Yes) {
        invokeOperation(receiverGuard.data(), false, {});
        return;
    }

    if (updatedField == previous && updatedNotes == contact.note()) {
        invokeOperation(receiverGuard.data(), true, {});
        return;
    }

    if (updatedField != previous) {
        contact.insertCustom(
            QLatin1String(kFingerprintApp), QLatin1String(kFingerprintField), updatedField);
    }
    if (updatedNotes != contact.note()) {
        contact.setNote(updatedNotes);
    }
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

void showContactSelector(QObject *receiver, const QString &fingerprint, const Akonadi::Item::List &items)
{
    const QPointer<QObject> receiverGuard(receiver);
    QHash<QString, Akonadi::Item> itemsByUid;
    QHash<QString, QString> namesByUid;
    QHash<QString, QString> searchTextByUid;
    QSet<QString> ambiguousUids;

    for (const Akonadi::Item &item : items) {
        if (!item.isValid() || !item.hasPayload<KContacts::Addressee>()) {
            continue;
        }

        KContacts::Addressee contact;
        try {
            contact = item.payload<KContacts::Addressee>();
        } catch (...) {
            continue;
        }

        const QString uid = contact.uid();
        if (uid.isEmpty() || ambiguousUids.contains(uid)) {
            continue;
        }
        if (itemsByUid.contains(uid)) {
            itemsByUid.remove(uid);
            namesByUid.remove(uid);
            searchTextByUid.remove(uid);
            ambiguousUids.insert(uid);
            continue;
        }

        const QString name = contact.realName().trimmed();
        const QString email = contact.preferredEmail().trimmed();
        QString displayName = name;
        if (!email.isEmpty()) {
            displayName = displayName.isEmpty()
                ? email
                : QStringLiteral("%1 <%2>").arg(displayName, email);
        }
        if (displayName.isEmpty()) {
            displayName = uid;
        }

        itemsByUid.insert(uid, item);
        namesByUid.insert(uid, displayName);
        searchTextByUid.insert(uid, QStringLiteral("%1 %2").arg(name, uid));
    }

    if (itemsByUid.isEmpty()) {
        invokeOperation(
            receiverGuard.data(),
            false,
            QStringLiteral("No hay contactos únicos con una identidad válida para vincular."));
        return;
    }

    auto *dialog = new QDialog(QApplication::activeWindow());
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    dialog->setWindowTitle(QStringLiteral("Elegir contacto para vincular"));
    dialog->resize(520, 440);

    auto *layout = new QVBoxLayout(dialog);
    auto *description = new QLabel(
        QStringLiteral("Selecciona un contacto de KDE Contacts para añadirle esta huella OpenPGP."),
        dialog);
    description->setWordWrap(true);
    layout->addWidget(description);

    auto *search = new QLineEdit(dialog);
    search->setClearButtonEnabled(true);
    search->setPlaceholderText(QStringLiteral("Buscar por nombre o UID"));
    layout->addWidget(search);

    auto *contactList = new QListWidget(dialog);
    contactList->setAlternatingRowColors(true);
    contactList->setSelectionMode(QAbstractItemView::SingleSelection);
    contactList->setSelectionBehavior(QAbstractItemView::SelectRows);
    contactList->setSortingEnabled(true);
    layout->addWidget(contactList);

    for (auto it = namesByUid.cbegin(); it != namesByUid.cend(); ++it) {
        auto *row = new QListWidgetItem(it.value(), contactList);
        row->setData(Qt::UserRole, it.key());
        row->setData(Qt::UserRole + 1, searchTextByUid.value(it.key()));
        row->setToolTip(it.key());
    }

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, dialog);
    auto *chooseButton = buttons->button(QDialogButtonBox::Ok);
    chooseButton->setEnabled(false);
    layout->addWidget(buttons);

    QObject::connect(
        contactList,
        &QListWidget::currentRowChanged,
        dialog,
        [chooseButton](int row) { chooseButton->setEnabled(row >= 0); });
    QObject::connect(
        search,
        &QLineEdit::textChanged,
        dialog,
        [contactList](const QString &query) {
            const QString needle = query.trimmed();
            for (int index = 0; index < contactList->count(); ++index) {
                auto *row = contactList->item(index);
                const bool matches = needle.isEmpty()
                    || row->data(Qt::UserRole + 1).toString().contains(needle, Qt::CaseInsensitive);
                row->setHidden(!matches);
            }
            if (contactList->currentItem() && contactList->currentItem()->isHidden()) {
                contactList->setCurrentRow(-1);
            }
        });
    QObject::connect(
        contactList,
        &QListWidget::itemDoubleClicked,
        dialog,
        [dialog](QListWidgetItem *) { dialog->accept(); });
    QObject::connect(buttons, &QDialogButtonBox::accepted, dialog, &QDialog::accept);
    QObject::connect(buttons, &QDialogButtonBox::rejected, dialog, &QDialog::reject);

    QObject::connect(
        dialog,
        &QDialog::accepted,
        receiver,
        [receiverGuard, contactList, fingerprint, itemsByUid, namesByUid]() {
            if (!receiverGuard) {
                return;
            }
            const QListWidgetItem *selected = contactList->currentItem();
            if (!selected || selected->isHidden()) {
                invokeOperation(
                    receiverGuard.data(),
                    false,
                    QStringLiteral("Selecciona un contacto para vincular."));
                return;
            }

            const QString uid = selected->data(Qt::UserRole).toString();
            const auto item = itemsByUid.constFind(uid);
            if (item == itemsByUid.cend()) {
                invokeOperation(
                    receiverGuard.data(),
                    false,
                    QStringLiteral("El contacto seleccionado ya no está disponible."));
                return;
            }

            const Akonadi::Item selectedItem = item.value();
            const QString contactName = namesByUid.value(uid, uid);
            QTimer::singleShot(0, receiverGuard.data(), [receiverGuard, selectedItem, fingerprint, contactName] {
                if (receiverGuard) {
                    finishLink(
                        receiverGuard.data(), selectedItem, fingerprint.toUpper(), contactName);
                }
            });
        });
    QObject::connect(dialog, &QDialog::rejected, receiver, [receiverGuard] {
        if (receiverGuard) {
            invokeOperation(receiverGuard.data(), false, {});
        }
    });
    QObject::connect(receiver, &QObject::destroyed, dialog, &QObject::deleteLater);

    if (contactList->count() > 0) {
        contactList->setCurrentRow(0);
    }
    dialog->open();
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

    const QPointer<QObject> receiverGuard(receiver);
    auto *job = new Akonadi::ContactSearchJob(receiver);
    QObject::connect(job, &KJob::result, receiver, [receiverGuard, job, fingerprint](KJob *) {
        if (!receiverGuard) {
            job->deleteLater();
            return;
        }
        if (job->error()) {
            invokeOperation(
                receiverGuard.data(), false, QStringLiteral("No se pudieron consultar los contactos de KDE."));
        } else {
            showContactSelector(receiverGuard.data(), fingerprint.toUpper(), job->items());
        }
        job->deleteLater();
    });
}

bool open_contacts()
{
    return QProcess::startDetached(QStringLiteral("kaddressbook"), {});
}

} // namespace kryptopass::desktop::native
