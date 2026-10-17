pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

ColumnLayout {
    id: root

    required property KeyStore keyStore
    required property var selectedKey
    readonly property Backend.ContactsController contactsController: keyStore.contactsController
    readonly property var linkedContacts: keyStore.contactRows.filter(contact =>
        keyStore.contactFingerprints(contact).includes((selectedKey?.id || "").toUpperCase()))

    spacing: Kirigami.Units.largeSpacing

    Controls.Label {
        Layout.fillWidth: true
        text: qsTr("Elige un contacto de tu agenda KDE para asociarlo a esta clave. Comprueba la huella con su propietario antes de usarla.")
        wrapMode: Text.Wrap
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: !!root.contactsController.errorMessage
        type: Kirigami.MessageType.Error
        text: root.contactsController.errorMessage
    }

    Controls.Frame {
        Layout.fillWidth: true
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            Kirigami.Heading {
                Layout.fillWidth: true
                level: 3
                text: qsTr("Contacto vinculado")
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: root.linkedContacts.length === 0
                text: qsTr("No hay un contacto de la agenda vinculado a esta clave.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: root.linkedContacts.length > 0
                text: root.selectedKey?.id || ""
                font: Kirigami.Theme.fixedWidthFont
                textFormat: Text.PlainText
                wrapMode: Text.WrapAnywhere
            }
            Repeater {
                model: root.linkedContacts
                delegate: RowLayout {
                    id: linkedContactRow
                    required property var modelData
                    Layout.fillWidth: true
                    Controls.Label {
                        Layout.fillWidth: true
                        text: linkedContactRow.modelData.name || linkedContactRow.modelData.email || linkedContactRow.modelData.uid
                        textFormat: Text.PlainText
                        font.weight: Font.DemiBold
                        wrapMode: Text.Wrap
                    }
                    Controls.Button {
                        enabled: !root.contactsController.busy
                        text: qsTr("Editar")
                        icon.name: "document-edit"
                        Accessible.name: qsTr("Editar contacto %1").arg(linkedContactRow.modelData.name || linkedContactRow.modelData.email || linkedContactRow.modelData.uid)
                        onClicked: root.contactsController.editContact(linkedContactRow.modelData.uid)
                    }
                }
            }
            Flow {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Controls.Button {
                    enabled: !root.contactsController.busy
                    text: qsTr("Vincular a contacto de KDE…")
                    icon.name: "link"
                    onClicked: root.contactsController.chooseContactForKey(root.selectedKey.id)
                }
            }
        }
    }

    Kirigami.Heading {
        level: 3
        text: qsTr("Agenda KDE")
    }
    Flow {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        Controls.Button {
            enabled: !root.contactsController.busy
            text: qsTr("Actualizar agenda")
            icon.name: "view-refresh"
            onClicked: root.contactsController.refreshContacts()
        }
        Controls.Button {
            enabled: !root.contactsController.busy
            text: qsTr("Nuevo contacto")
            icon.name: "contact-new"
            onClicked: root.contactsController.newContact()
        }
        Controls.Button {
            enabled: !root.contactsController.busy
            text: qsTr("Abrir KDE Contacts")
            icon.name: "view-pim-contacts"
            onClicked: root.contactsController.openContacts()
        }
    }
    Controls.Label {
        Layout.fillWidth: true
        visible: root.contactsController.busy
        text: qsTr("Consultando o actualizando KDE Contacts…")
        color: Kirigami.Theme.disabledTextColor
        wrapMode: Text.Wrap
    }
}
