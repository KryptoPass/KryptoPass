#pragma once

#include <QString>
#include <QStringList>

class QObject;

namespace kryptopass::desktop::native
{

void refresh_contacts(QObject *receiver);
void open_contact_editor(QObject *receiver, const QString &uid);
void choose_contact_for_key(QObject *receiver, const QString &fingerprint);
bool open_contacts();

} // namespace kryptopass::desktop::native
