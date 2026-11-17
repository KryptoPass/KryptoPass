pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

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
            title: personId ? KI18n.i18nc("@title:window", "Rename Person") : KI18n.i18nc("@title:window", "Add Person")
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
                    placeholderText: KI18n.i18nc("@info:placeholder", "Person's name")
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
            title: KI18n.i18nc("@title:window", "Remove Person from KryptoPass")
            modal: true
            standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
            contentItem: Controls.Label {
                text: KI18n.i18n("Their keys stay in the keyring, without an assigned person and without your verifications.")
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
        text: KI18n.i18nc("@title key scope", "People")
    }
    RowLayout {
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Add Person…")
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
            text: KI18n.i18nc("@action:button", "Import Public Key…")
            enabled: !root.keyStore.controller.busy
            onClicked: root.operationDialog.openFor("import")
        }
    }
    Controls.Button {
        text: root.addressBookVisible ? KI18n.i18nc("@action:button", "Back to People") : KI18n.i18nc("@action:button", "Link with the KDE Address Book…")
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
                            text: KI18n.i18nc("@action:inmenu", "Remove Person…")
                            enabled: !root.keyStore.controller.busy
                            onClicked: {
                                removePerson.personId = personCard.modelData.id;
                                removePerson.open();
                            }
                        }
                        RowLayout {
                            Controls.Button {
                                text: KI18n.i18nc("@action:inmenu", "View Keys")
                                onClicked: {
                                    root.keyStore.scope = "contacts";
                                    root.keyStore.query = "";
                                    root.keyStore.selectedId = "person:" + personCard.modelData.id;
                                    root.keyStore.contactsOverviewVisible = false;
                                }
                            }
                            Controls.Button {
                                text: KI18n.i18nc("@action:inmenu", "Rename…")
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
                text: KI18n.i18n("No people.")
                wrapMode: Text.Wrap
            }
        }
    }
}
