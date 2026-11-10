pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: root
    required property KeyStore keyStore
    required property KeyOperationDialog operationDialog
    property int step: 0
    property bool waiting: false
    property bool saving: false
    property string fingerprint: ""
    readonly property var identity: keyStore.entries.find(key => key.id === fingerprint)
    parent: Controls.Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(parent.width - Kirigami.Units.largeSpacing * 2, Kirigami.Units.gridUnit * 32)
    modal: true
    title: step === 0 ? qsTr("Añadir identidad") : qsTr("Configuración en KryptoPass")
    standardButtons: Controls.Dialog.Close
    closePolicy: Controls.Popup.NoAutoClose
    function start() {
        step = 0;
        waiting = false;
        saving = false;
        fingerprint = "";
        open();
    }
    function prepare(value) {
        fingerprint = value;
        const key = keyStore.entries.find(item => item.id === value);
        if (!key)
            return;
        label.text = key.title;
        dedicated.checked = !!key.meta.forgetOnLock;
        useDefault.checked = keyStore.isPrimary(value);
        step = 1;
        open();
    }
    function perform(action) {
        waiting = true;
        close();
        operationDialog.openFor(action);
    }
    Connections {
        target: root.operationDialog
        function onClosed() {
            if (!root.waiting)
                return;
            root.waiting = false;
            if (root.operationDialog.completedFingerprint)
                root.prepare(root.operationDialog.completedFingerprint);
            else {
                root.step = 0;
                root.open();
            }
        }
    }
    Connections {
        target: root.keyStore.controller
        function onCatalogSaved() {
            if (!root.saving)
                return;
            root.saving = false;
            if (useDefault.checked && !root.keyStore.isPrimary(root.fingerprint))
                root.keyStore.primaryKeyRequested(root.fingerprint);
            root.keyStore.selectAfterRefresh(root.fingerprint);
            root.close();
        }
        function onOperationFailed() {
            root.saving = false;
        }
    }
    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        ColumnLayout {
            visible: root.step === 0
            Layout.fillWidth: true
            Controls.Button {
                text: qsTr("Crear clave OpenPGP…")
                enabled: !root.keyStore.controller.busy
                onClicked: root.perform("create")
            }
            Controls.Button {
                text: qsTr("Importar clave OpenPGP…")
                enabled: !root.keyStore.controller.busy
                onClicked: root.perform("import")
            }
            Controls.ComboBox {
                id: existing
                Layout.fillWidth: true
                model: root.keyStore.entries
                textRole: "title"
                Accessible.name: qsTr("Identidad existente en el llavero")
            }
            Controls.Button {
                text: qsTr("Usar clave existente")
                enabled: existing.currentIndex >= 0 && !root.keyStore.controller.busy
                onClicked: root.prepare(root.keyStore.entries[existing.currentIndex].id)
            }
        }
        ColumnLayout {
            visible: root.step === 1
            Layout.fillWidth: true
            Controls.Label {
                Layout.fillWidth: true
                text: root.identity?.pgpTitle || ""
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.TextField {
                id: label
                Layout.fillWidth: true
                maximumLength: 160
                placeholderText: qsTr("Alias en KryptoPass")
                Accessible.name: placeholderText
            }
            Controls.CheckBox {
                id: dedicated
                text: qsTr("Olvidar contraseña al bloquear la bóveda (gpg-agent)")
            }
            Controls.CheckBox {
                id: useDefault
                text: qsTr("Usar como clave de bóveda")
                enabled: !!root.identity?.canEncrypt && !!root.identity?.hasSecret
            }
            Controls.Button {
                text: qsTr("Guardar")
                enabled: !!label.text.trim() && !root.keyStore.controller.busy && !root.saving
                onClicked: {
                    root.saving = true;
                    root.keyStore.editCatalog({
                        action: "identity",
                        fingerprint: root.fingerprint,
                        label: label.text,
                        own: true,
                        person: null,
                        forget: dedicated.checked
                    });
                }
            }
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: !!root.keyStore.controller.operationError
            text: root.keyStore.controller.operationError
            type: Kirigami.MessageType.Error
        }
    }
}
