pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root
    required property KeyStore keyStore
    required property KeyOperationDialog operationDialog
    property bool addressBookVisible: false
    // Popups live in the overlay; this holder keeps them out of the layout.
    Item {
        visible: false
        Controls.Dialog {
            id: personEditor
            property string personId: ""
            property bool saving: false
            parent: Controls.Overlay.overlay
            anchors.centerIn: parent
            width: Math.min(parent.width - Kirigami.Units.largeSpacing * 2, Kirigami.Units.gridUnit * 28)
            modal: true
            title: personId ? qsTr("Cambiar nombre de la persona") : qsTr("Añadir persona")
            standardButtons: Controls.Dialog.Save | Controls.Dialog.Cancel
            footer: Controls.DialogButtonBox {
                standardButtons: Controls.Dialog.Save | Controls.Dialog.Cancel
                onAccepted: personEditor.save()
                onRejected: personEditor.close()
            }
            closePolicy: Controls.Popup.NoAutoClose
            onOpened: standardButton(Controls.Dialog.Save).enabled = Qt.binding(() => !!name.text.trim() && !root.keyStore.controller.busy)
            function save() {
                saving = true;
                root.keyStore.editCatalog({
                    action: "person",
                    id: personId || null,
                    name: name.text
                });
            }
            contentItem: ColumnLayout {
                Controls.TextField {
                    id: name
                    Layout.fillWidth: true
                    maximumLength: 160
                    placeholderText: qsTr("Nombre de la persona")
                    Accessible.name: placeholderText
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !!root.keyStore.controller.operationError
                    text: root.keyStore.controller.operationError
                    type: Kirigami.MessageType.Error
                }
            }
        }
        Controls.Dialog {
            id: removePerson
            property string personId: ""
            parent: Controls.Overlay.overlay
            anchors.centerIn: parent
            width: Math.min(parent.width - Kirigami.Units.largeSpacing * 2, Kirigami.Units.gridUnit * 28)
            title: qsTr("Quitar persona de KryptoPass")
            modal: true
            standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
            contentItem: Controls.Label {
                text: qsTr("Sus claves siguen en el llavero, sin persona asignada y sin tus verificaciones.")
                wrapMode: Text.Wrap
            }
            onAccepted: root.keyStore.editCatalog({
                action: "removePerson",
                id: personId
            })
        }
    }
    Connections {
        target: root.keyStore.controller
        function onCatalogSaved() {
            if (personEditor.saving) {
                personEditor.saving = false;
                personEditor.close();
            }
        }
        function onOperationFailed() {
            personEditor.saving = false;
        }
    }
    Kirigami.Heading {
        level: 2
        text: qsTr("Personas")
    }
    RowLayout {
        Controls.Button {
            text: qsTr("Añadir persona…")
            icon.name: "list-add-user"
            enabled: !root.keyStore.controller.busy
            onClicked: {
                personEditor.personId = "";
                name.text = "";
                personEditor.saving = false;
                personEditor.open();
            }
        }
        Controls.Button {
            text: qsTr("Importar clave pública…")
            enabled: !root.keyStore.controller.busy
            onClicked: root.operationDialog.openFor("import")
        }
    }
    Controls.Button {
        text: root.addressBookVisible ? qsTr("Volver a personas") : qsTr("Vincular con la agenda de KDE…")
        onClicked: root.addressBookVisible = !root.addressBookVisible
    }
    KeyAddressBook {
        visible: root.addressBookVisible
        Layout.fillWidth: true
        Layout.fillHeight: true
        keyStore: root.keyStore
        operationDialog: root.operationDialog
    }
    Controls.ScrollView {
        id: peopleScroll
        visible: !root.addressBookVisible
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            width: peopleScroll.availableWidth
            Repeater {
                model: root.keyStore.people
                delegate: Controls.Frame {
                    id: personCard
                    required property var modelData
                    Layout.fillWidth: true
                    contentItem: ColumnLayout {
                        Controls.Label {
                            Layout.fillWidth: true
                            text: personCard.modelData.name
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Controls.Button {
                            text: qsTr("Quitar persona…")
                            enabled: !root.keyStore.controller.busy
                            onClicked: {
                                removePerson.personId = personCard.modelData.id;
                                removePerson.open();
                            }
                        }
                        RowLayout {
                            Controls.Button {
                                text: qsTr("Ver claves")
                                onClicked: {
                                    root.keyStore.scope = "contacts";
                                    root.keyStore.query = "";
                                    root.keyStore.selectedId = "person:" + personCard.modelData.id;
                                    root.keyStore.contactsOverviewVisible = false;
                                }
                            }
                            Controls.Button {
                                text: qsTr("Cambiar nombre…")
                                enabled: !root.keyStore.controller.busy
                                onClicked: {
                                    personEditor.personId = personCard.modelData.id;
                                    name.text = personCard.modelData.name;
                                    personEditor.saving = false;
                                    personEditor.open();
                                }
                            }
                        }
                    }
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !root.keyStore.people.length
                text: qsTr("Sin personas.")
                wrapMode: Text.Wrap
            }
        }
    }
}
