pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Dialog {
    id: root
    required property KeyStore keyStore
    property string fingerprint: ""
    property bool saving: false
    parent: Controls.Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(parent.width - Kirigami.Units.largeSpacing * 2, Kirigami.Units.gridUnit * 30)
    modal: true
    title: KI18n.i18nc("@title:window", "Alias and Organization")
    standardButtons: Controls.Dialog.Save | Controls.Dialog.Cancel
    footer: Controls.DialogButtonBox {
        standardButtons: Controls.Dialog.Save | Controls.Dialog.Cancel
        onAccepted: root.save()
        onRejected: root.close()
    }
    closePolicy: Controls.Popup.NoAutoClose
    function edit(key) {
        fingerprint = key.id;
        label.text = key.title;
        own.checked = key.own;
        person.currentIndex = Math.max(0, keyStore.people.findIndex(item => item.id === key.meta.person) + 1);
        saving = false;
        keyStore.controller.clearOperationError();
        open();
    }
    onOpened: standardButton(Controls.Dialog.Save).enabled = Qt.binding(() => !!label.text.trim() && !root.keyStore.controller.busy)
    function save() {
        saving = true;
        keyStore.editCatalog({
            action: "identity",
            fingerprint,
            label: label.text,
            own: own.checked,
            person: own.checked || person.currentIndex < 1 ? null : keyStore.people[person.currentIndex - 1].id
        });
    }
    Connections {
        target: root.keyStore.controller
        function onCatalogSaved() {
            if (root.saving) {
                root.saving = false;
                root.keyStore.selectAfterRefresh(root.fingerprint);
                root.close();
            }
        }
        function onOperationFailed() {
            root.saving = false;
        }
    }
    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Controls.Label {
            text: KI18n.i18nc("@label:textbox", "Alias in KryptoPass")
        }
        Controls.TextField {
            id: label
            Layout.fillWidth: true
            maximumLength: 160
            placeholderText: KI18n.i18nc("@info:placeholder", "For example: Personal, Work")
            Accessible.name: KI18n.i18n("Name in KryptoPass")
        }
        Controls.CheckBox {
            id: own
            text: KI18n.i18nc("@option:check", "This is one of my identities")
        }
        Controls.ComboBox {
            id: person
            visible: !own.checked
            Layout.fillWidth: true
            model: [KI18n.i18nc("@item:inlistbox", "No person assigned"), ...root.keyStore.people.map(item => item.name)]
            Accessible.name: KI18n.i18n("Person it belongs to")
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: !!root.keyStore.controller.operationError
            type: Kirigami.MessageType.Error
            text: root.keyStore.controller.operationError
        }
    }
}
