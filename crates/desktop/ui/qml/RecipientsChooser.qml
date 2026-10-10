pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

ColumnLayout {
    id: chooser

    required property KeyStore keyStore
    property Backend.ContactsController contactsController: Backend.ContactsController {}
    property var selectedFingerprints: []
    readonly property var encryptableKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.subkeys.some(subkey => subkey.statusCode === "valid" && subkey.capabilities.includes("Cifrar")))
    readonly property var unavailableFingerprints: selectedFingerprints.filter(fingerprint => !encryptableKeys.some(key => key.id === fingerprint))
    readonly property bool eligibleSelection: selectedFingerprints.length > 0 && unavailableFingerprints.length === 0
    readonly property var contactRows: {
        const parsed = [];
        for (const record of contactsController.contactRecords) {
            try {
                parsed.push(JSON.parse(record));
            } catch (error) {
                // Ignore malformed contact rows from the native projection.
            }
        }
        return parsed;
    }
    readonly property var linkedRecipientRows: contactRows.filter(contact => {
        const fingerprint = (contact.openPgpFingerprint || "").toUpperCase();
        return contactsController.recipientCandidates.includes(fingerprint)
                && encryptableKeys.some(key => key.id.toUpperCase() === fingerprint);
    })
    readonly property QtObject recipientDialogOwner: QtObject {
        readonly property Controls.Dialog dialog: Controls.Dialog {
            id: contactRecipientDialog
            parent: Controls.Overlay.overlay
            title: qsTr("Elegir un contacto vinculado")
            modal: true
            standardButtons: Controls.Dialog.Cancel
            implicitWidth: Math.min(Kirigami.Units.gridUnit * 28, chooser.width)
            implicitHeight: Math.min(contentItem.implicitHeight + topPadding + bottomPadding + header.implicitHeight + footer.implicitHeight,
                                     parent ? parent.height - Kirigami.Units.gridUnit * 4 : 480)
            x: parent ? (parent.width - width) / 2 : 0
            y: parent ? (parent.height - height) / 2 : 0
            onRejected: chooser.contactsController.clearRecipientSelection()

            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Elige un contacto ya vinculado a una de estas claves. La coincidencia se hace por huella completa, nunca por correo.")
                    wrapMode: Text.Wrap
                }
                Controls.ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.min(contentHeight, Kirigami.Units.gridUnit * 16)
                    clip: true
                    ColumnLayout {
                        width: contactRecipientDialog.availableWidth
                        Repeater {
                            model: chooser.linkedRecipientRows
                            delegate: Controls.ItemDelegate {
                                id: contactChoice
                                required property var modelData
                                Layout.fillWidth: true
                                text: contactChoice.modelData.name || contactChoice.modelData.email || contactChoice.modelData.uid
                                Accessible.name: qsTr("Elegir contacto %1, huella %2").arg(text).arg(contactChoice.modelData.openPgpFingerprint)
                                onClicked: {
                                    chooser.contactsController.selectRecipient(contactChoice.modelData.openPgpFingerprint);
                                    contactRecipientDialog.close();
                                }
                                contentItem: ColumnLayout {
                                    spacing: 2
                                    Controls.Label {
                                        Layout.fillWidth: true
                                        text: contactChoice.modelData.name || contactChoice.modelData.email || contactChoice.modelData.uid
                                        textFormat: Text.PlainText
                                        wrapMode: Text.Wrap
                                    }
                                    Controls.Label {
                                        Layout.fillWidth: true
                                        text: contactChoice.modelData.email || contactChoice.modelData.openPgpFingerprint
                                        textFormat: Text.PlainText
                                        color: Kirigami.Theme.disabledTextColor
                                        wrapMode: Text.WrapAnywhere
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

    }
    signal selectionChanged(var fingerprints)

    spacing: 8

    function setChecked(fingerprint, checked) {
        const next = selectedFingerprints.filter(candidate => candidate !== fingerprint);
        if (checked)
            next.push(fingerprint);
        selectedFingerprints = next;
        selectionChanged(next);
    }

    function contactForFingerprint(fingerprint) {
        const normalized = fingerprint.toUpperCase();
        return contactRows.find(contact => (contact.openPgpFingerprint || "").toUpperCase() === normalized) || null;
    }

    Connections {
        target: chooser.contactsController
        function onRecipientSelected(fingerprint) {
            chooser.setChecked(fingerprint, true);
        }
    }

    Controls.Label {
        text: qsTr("Claves autorizadas")
        font.weight: Font.DemiBold
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 8

        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Elige las claves OpenPGP que podrán descifrar este elemento.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Controls.Button {
                text: qsTr("Actualizar contactos")
                icon.name: "view-refresh"
                enabled: !chooser.contactsController.busy
                onClicked: chooser.contactsController.refreshContacts()
            }
            Controls.Button {
                text: qsTr("Nuevo contacto")
                icon.name: "contact-new"
                enabled: !chooser.contactsController.busy
                onClicked: chooser.contactsController.newContact()
            }
            Controls.Button {
                text: qsTr("Abrir agenda KDE")
                icon.name: "view-pim-contacts"
                enabled: !chooser.contactsController.busy
                onClicked: chooser.contactsController.openContacts()
            }
            Controls.Button {
                text: qsTr("Elegir desde contactos")
                icon.name: "contact-new"
                enabled: !chooser.contactsController.busy && chooser.contactRows.length > 0 && chooser.encryptableKeys.length > 0
                onClicked: {
                    chooser.contactsController.clearRecipientSelection();
                    chooser.contactsController.chooseRecipient(JSON.stringify(chooser.encryptableKeys.map(key => key.id)));
                    if (chooser.contactsController.recipientCandidates.length > 0)
                        contactRecipientDialog.open();
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: chooser.contactsController.busy
            text: qsTr("Consultando o actualizando KDE Contacts…")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !!chooser.contactsController.errorMessage
            text: chooser.contactsController.errorMessage
            color: Kirigami.Theme.negativeTextColor
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        Repeater {
            model: chooser.encryptableKeys

            delegate: ColumnLayout {
                id: keyRow
                required property var modelData
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                spacing: 4
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Controls.CheckBox {
                        id: keyCheck
                        checked: chooser.selectedFingerprints.includes(keyRow.modelData.id)
                        Accessible.name: qsTr("Autorizar %1, %2, huella %3").arg(keyRow.modelData.title).arg(keyRow.modelData.email || "").arg(keyRow.modelData.id)
                        onToggled: chooser.setChecked(keyRow.modelData.id, checked)
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        spacing: 2
                        Controls.Label {
                            Layout.fillWidth: true
                            text: keyRow.modelData.title
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: !!keyRow.modelData.email
                            text: keyRow.modelData.email || ""
                            textFormat: Text.PlainText
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.WrapAnywhere
                        }
                    }
                    Controls.ToolButton {
                        id: keyDetails
                        text: checked ? qsTr("Ocultar detalles de la clave") : qsTr("Ver detalles de la clave")
                        icon.name: checked ? "arrow-up" : "arrow-down"
                        display: Controls.AbstractButton.IconOnly
                        checkable: true
                        Accessible.name: qsTr("%1: %2").arg(text).arg(keyRow.modelData.title)
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: keyCheck.implicitWidth + 8
                    visible: keyDetails.checked
                    text: qsTr("Huella OpenPGP: %1").arg(keyRow.modelData.id)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                    color: Kirigami.Theme.disabledTextColor
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: keyCheck.implicitWidth + 8
                    spacing: Kirigami.Units.smallSpacing
                    readonly property var linkedContact: chooser.contactForFingerprint(keyRow.modelData.id)
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !!parent.linkedContact
                        text: parent.linkedContact ? qsTr("Contacto vinculado: %1").arg(parent.linkedContact.name || parent.linkedContact.email || parent.linkedContact.uid) : ""
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Button {
                        text: parent.linkedContact ? qsTr("Editar contacto") : qsTr("Vincular contacto")
                        icon.name: parent.linkedContact ? "document-edit" : "link"
                        enabled: !chooser.contactsController.busy
                        Accessible.name: qsTr("%1 para la clave %2").arg(text).arg(keyRow.modelData.title)
                        onClicked: {
                            if (parent.linkedContact)
                                chooser.contactsController.editContact(parent.linkedContact.uid);
                            else
                                chooser.contactsController.chooseContactForKey(keyRow.modelData.id);
                        }
                    }
                }
            }
        }

        Repeater {
            model: chooser.unavailableFingerprints

            delegate: RowLayout {
                id: unavailableRow
                required property string modelData
                Layout.fillWidth: true
                spacing: 8
                Controls.CheckBox {
                    checked: chooser.selectedFingerprints.includes(unavailableRow.modelData)
                    Accessible.name: qsTr("Destinatario original no disponible, huella %1").arg(unavailableRow.modelData)
                    onToggled: chooser.setChecked(unavailableRow.modelData, checked)
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("%1 · sin clave válida para cifrar").arg(unavailableRow.modelData)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: chooser.keyStore.controller.busy && chooser.encryptableKeys.length === 0
            text: qsTr("Consultando claves OpenPGP…")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !chooser.keyStore.controller.busy && chooser.encryptableKeys.length === 0 && chooser.unavailableFingerprints.length === 0
            text: qsTr("No hay claves válidas con capacidad de cifrado. Actualiza el llavero o revisa las claves.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: chooser.unavailableFingerprints.length > 0
            text: qsTr("Se conserva cada huella original. Desmarca las claves no disponibles o elige una clave válida antes de guardar.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
    }
}
