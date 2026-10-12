pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

ColumnLayout {
    id: chooser

    required property KeyStore keyStore
    readonly property Backend.ContactsController contactsController: keyStore.contactsController
    property var selectedFingerprints: []
    readonly property var encryptableKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.canEncrypt)
    readonly property var matchingKeys: chooser.encryptableKeys.filter(key => (key.title + " " + key.email + " " + key.contactName + " " + key.id).toLowerCase().includes(recipientSearch.text.trim().toLowerCase()) || key.id.toLowerCase().includes(recipientSearch.text.replace(/\s/g, "").toLowerCase()))
    readonly property var unavailableFingerprints: selectedFingerprints.filter(fingerprint => !encryptableKeys.some(key => key.id.toUpperCase() === fingerprint.toUpperCase()))
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
    readonly property var linkedRecipientRows: {
        const rows = [];
        for (const contact of contactRows) {
            for (const fingerprint of keyStore.contactFingerprints(contact)) {
                if (Array.from(contactsController.recipientCandidates).includes(fingerprint)
                        && encryptableKeys.some(key => key.id.toUpperCase() === fingerprint))
                    rows.push(Object.assign({}, contact, { openPgpFingerprint: fingerprint }));
            }
        }
        return rows;
    }
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

    KeyOperationDialog {
        id: recoveryDialog
        keyStore: chooser.keyStore
    }

    function setChecked(fingerprint, checked) {
        const next = selectedFingerprints.filter(candidate => candidate !== fingerprint);
        if (checked)
            next.push(fingerprint);
        selectedFingerprints = next;
        selectionChanged(next);
    }

    function contactsForFingerprint(fingerprint) {
        const normalized = fingerprint.toUpperCase();
        return contactRows.filter(contact => keyStore.contactFingerprints(contact).includes(normalized));
    }

    Connections {
        target: chooser.contactsController
        function onRecipientSelected(fingerprint) {
            chooser.setChecked(fingerprint, true);
        }
    }

    Controls.Label {
        text: qsTr("Quién tiene acceso")
        font.weight: Font.DemiBold
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 8

        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Elige quién puede abrir este elemento con su clave privada OpenPGP.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Kirigami.SearchField {
            id: recipientSearch
            Layout.fillWidth: true
            placeholderText: qsTr("Buscar nombre, correo o huella…")
            Accessible.name: qsTr("Buscar personas con clave OpenPGP")
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
                text: qsTr("Importar clave…")
                icon.name: "document-import"
                enabled: !chooser.keyStore.controller.busy && !chooser.keyStore.controller.operationBusy
                onClicked: recoveryDialog.openFor("import")
            }
            Controls.Button {
                text: qsTr("Obtener del servidor…")
                icon.name: "download"
                enabled: !chooser.keyStore.controller.busy && !chooser.keyStore.controller.operationBusy
                onClicked: recoveryDialog.openFor("refreshExternal")
            }
            Controls.Button {
                text: qsTr("Elegir desde contactos")
                icon.name: "contact-new"
                enabled: !chooser.contactsController.busy && chooser.contactRows.length > 0 && chooser.encryptableKeys.length > 0
                onClicked: {
                    chooser.contactsController.clearRecipientSelection();
                    chooser.contactsController.chooseRecipient(JSON.stringify(chooser.encryptableKeys.map(key => key.id)));
                    if (Array.from(chooser.contactsController.recipientCandidates).length > 0)
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

        Controls.Label {
            Layout.fillWidth: true
            visible: !!recipientSearch.text && chooser.matchingKeys.length === 0 && chooser.encryptableKeys.length > 0
            text: qsTr("No hay claves que coincidan con la búsqueda.")
            wrapMode: Text.Wrap
        }
        Repeater {
            model: chooser.matchingKeys

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
                        Accessible.name: qsTr("Dar acceso a %1, %2, huella %3").arg(keyRow.modelData.title).arg(keyRow.modelData.email || "").arg(keyRow.modelData.id)
                        onToggled: chooser.setChecked(keyRow.modelData.id, checked)
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        spacing: 2
                        Controls.Label {
                            Layout.fillWidth: true
                            text: keyRow.modelData.contactName || keyRow.modelData.title
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
                Controls.Button {
                    Layout.leftMargin: keyCheck.implicitWidth + 8
                    visible: keyDetails.checked
                    enabled: !chooser.keyStore.controller.busy && !chooser.keyStore.controller.operationBusy
                    text: qsTr("Copiar clave pública")
                    icon.name: "edit-copy"
                    onClicked: recoveryDialog.openFor("copyPublic", keyRow.modelData.id)
                }
                ColumnLayout {
                    id: contactLinkArea
                    visible: keyDetails.checked
                    Layout.fillWidth: true
                    Layout.leftMargin: keyCheck.implicitWidth + 8
                    spacing: Kirigami.Units.smallSpacing
                    readonly property var linkedContacts: chooser.contactsForFingerprint(keyRow.modelData.id)
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: contactLinkArea.linkedContacts.length > 0
                        text: qsTr("Contactos vinculados: %1").arg(contactLinkArea.linkedContacts.map(contact => contact.name || contact.email || contact.uid).join(", "))
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Repeater {
                            model: contactLinkArea.linkedContacts
                            delegate: Controls.Button {
                                id: linkedContactEdit
                                required property var modelData
                                text: qsTr("Editar %1").arg(linkedContactEdit.modelData.name || linkedContactEdit.modelData.email || linkedContactEdit.modelData.uid)
                                icon.name: "document-edit"
                                enabled: !chooser.contactsController.busy
                                Accessible.name: qsTr("Editar contacto %1 vinculado a %2").arg(linkedContactEdit.modelData.name || linkedContactEdit.modelData.email || linkedContactEdit.modelData.uid).arg(keyRow.modelData.title)
                                onClicked: chooser.contactsController.editContact(linkedContactEdit.modelData.uid)
                            }
                        }
                    }
                    Controls.Button {
                        text: qsTr("Vincular a contacto de KDE…")
                        icon.name: "link"
                        enabled: !chooser.contactsController.busy
                        Accessible.name: qsTr("Añadir vínculo de contacto para la clave %1").arg(keyRow.modelData.title)
                        onClicked: chooser.contactsController.chooseContactForKey(keyRow.modelData.id)
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
                    Accessible.name: qsTr("Clave de acceso original no disponible, huella %1").arg(unavailableRow.modelData)
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
