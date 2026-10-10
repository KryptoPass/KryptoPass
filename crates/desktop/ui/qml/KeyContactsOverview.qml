pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    required property KeyStore keyStore
    required property KeyOperationDialog operationDialog
    readonly property var contactsController: keyStore.contactsController
    readonly property var contactEntries: {
        const rows = [];
        for (const contact of keyStore.contactRows) {
            const fingerprints = keyStore.contactFingerprints(contact);
            if (!fingerprints.length) {
                rows.push({ contact: contact, fingerprint: "", key: null });
                continue;
            }
            for (const fingerprint of fingerprints) {
                const key = keyStore.entries.find(entry => entry.id.toUpperCase() === fingerprint) || null;
                rows.push({ contact: contact, fingerprint: fingerprint, key: key });
            }
        }
        return rows;
    }

    spacing: Kirigami.Units.largeSpacing

    Kirigami.Heading {
        Layout.fillWidth: true
        level: 2
        text: qsTr("Contactos de KDE Contacts")
    }
    Controls.Label {
        Layout.fillWidth: true
        text: qsTr("Los contactos siguen disponibles aunque no tengan una clave local. Importa un archivo o recupera una clave por su huella completa; después puedes añadir el vínculo desde los detalles de la clave. El correo nunca se usa como huella ni para confiar automáticamente.")
        wrapMode: Text.Wrap
    }
    Flow {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        Controls.Button {
            text: qsTr("Actualizar agenda")
            icon.name: "view-refresh"
            enabled: !root.contactsController.busy
            onClicked: root.contactsController.refreshContacts()
        }
        Controls.Button {
            text: qsTr("Nuevo contacto")
            icon.name: "contact-new"
            enabled: !root.contactsController.busy
            onClicked: root.contactsController.newContact()
        }
        Controls.Button {
            text: qsTr("Abrir KDE Contacts")
            icon.name: "view-pim-contacts"
            enabled: !root.contactsController.busy
            onClicked: root.contactsController.openContacts()
        }
    }
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: !!root.contactsController.errorMessage
        type: Kirigami.MessageType.Error
        text: root.contactsController.errorMessage
    }
    Controls.ScrollView {
        id: contactsScroll
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: contactsScroll.availableWidth
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                visible: root.contactsController.busy
                text: qsTr("Consultando KDE Contacts…")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: root.contactEntries.length === 0 && !root.contactsController.busy
                text: qsTr("No hay contactos en la agenda. Puedes añadir uno o abrir KDE Contacts.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            Repeater {
                model: root.contactEntries
                delegate: Controls.Frame {
                    id: contactEntry
                    required property var modelData
                    Layout.fillWidth: true
                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Controls.Label {
                            Layout.fillWidth: true
                            text: contactEntry.modelData.contact.name
                                || contactEntry.modelData.contact.email
                                || contactEntry.modelData.contact.uid
                                || qsTr("Contacto sin nombre")
                            textFormat: Text.PlainText
                            font.weight: Font.DemiBold
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: !!contactEntry.modelData.contact.email
                            text: contactEntry.modelData.contact.email || ""
                            textFormat: Text.PlainText
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.WrapAnywhere
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: contactEntry.modelData.fingerprint
                                || qsTr("Sin huella OpenPGP completa vinculada")
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.fixedWidthFont
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.WrapAnywhere
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: !!contactEntry.modelData.key
                            text: contactEntry.modelData.key
                                ? qsTr("Clave disponible en el llavero local: %1").arg(contactEntry.modelData.key.title || contactEntry.modelData.key.id)
                                : ""
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: !contactEntry.modelData.key
                            text: contactEntry.modelData.fingerprint
                                ? qsTr("La huella está vinculada al contacto, pero la clave no está en el llavero local.")
                                : qsTr("Este contacto no tiene una clave OpenPGP vinculada.")
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.Wrap
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Button {
                                visible: !!contactEntry.modelData.key
                                text: qsTr("Abrir clave")
                                icon.name: "view-certificate"
                                onClicked: {
                                    root.keyStore.scope = "all";
                                    root.keyStore.query = "";
                                    root.keyStore.selectedId = contactEntry.modelData.key.id;
                                }
                            }
                            Controls.Button {
                                visible: !contactEntry.modelData.key
                                enabled: !root.contactsController.busy
                                    && !root.keyStore.controller.busy
                                    && !root.keyStore.controller.operationBusy
                                text: qsTr("Recuperar por huella completa…")
                                icon.name: "view-refresh"
                                onClicked: {
                                    const fingerprint = contactEntry.modelData.fingerprint;
                                    root.operationDialog.openFor("refreshExternal", fingerprint);
                                }
                            }
                            Controls.Button {
                                visible: !contactEntry.modelData.key
                                enabled: !root.contactsController.busy
                                    && !root.keyStore.controller.busy
                                    && !root.keyStore.controller.operationBusy
                                text: qsTr("Importar desde archivo…")
                                icon.name: "document-open"
                                onClicked: root.operationDialog.openFor("import")
                            }
                            Controls.Button {
                                enabled: !root.contactsController.busy
                                text: qsTr("Editar contacto")
                                icon.name: "document-edit"
                                onClicked: root.contactsController.editContact(contactEntry.modelData.contact.uid)
                            }
                        }
                    }
                }
            }
        }
    }
}
