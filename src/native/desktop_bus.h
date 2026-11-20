#pragma once

#include <QObject>

namespace kryptopass::desktop::native
{

// Public D-Bus surface of the vault: commands only, never secrets or metadata.
class VaultBusAdaptor final : public QObject
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.kryptopass.desktop.Vault")

public:
    using QObject::QObject;

public Q_SLOTS:
    Q_SCRIPTABLE void Lock();

Q_SIGNALS:
    void lockRequested();
};

bool register_unique_application();
void install_activation_receiver(QObject *receiver);
void install_vault_bus(QObject *receiver);

}
