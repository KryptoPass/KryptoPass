pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: picker
    required property var host
    property string deviceId: ""
    property string connectionGeneration: ""
    property string recipient: ""
    property string selectedFingerprint: ""
    readonly property var keys: host.keyStore.entries.filter(key => key.hasSecret)
    readonly property var selectedKey: keys.find(key => String(key.id) === selectedFingerprint) || null
    readonly property var device: host.devices.find(candidate => String(candidate.deviceId) === deviceId) || ({})
    readonly property bool sessionCurrent: device.connection === "connected" && String(device.connectionGeneration) === connectionGeneration && String(device.syntheticTransferPhase) === "ready" && Number(device.keyTransferVersion || 0) >= 2
    readonly property bool keyCurrent: selectedKey !== null && host.canTransferKey(selectedKey)
    readonly property bool canContinue: selectedKey !== null && sessionCurrent && keyCurrent && !host.keyStore.controller.busy && !host.controller.busy && !host.controller.syncBusy && !host.controller.maintenanceBusy && !host.controller.syntheticTransferPreparing
    signal keyChosen(string fingerprint, string title)
    title: qsTr("Elegir clave para enviar")
    modal: true
    // The desktop style removes dialog padding for ScrollView content by default.
    padding: Kirigami.Units.largeSpacing
    spacing: 0
    width: Math.min(Kirigami.Units.gridUnit * 28, host.width - Kirigami.Units.gridUnit * 2)
    standardButtons: Controls.Dialog.NoButton
    onClosed: selectedFingerprint = ""

    contentItem: Controls.ScrollView {
        id: keyScroll
        contentWidth: availableWidth
        implicitHeight: Math.min(keyChoices.implicitHeight, Math.max(Kirigami.Units.gridUnit * 6, picker.host.height - Kirigami.Units.gridUnit * 9))
        clip: true
        ColumnLayout {
            id: keyChoices
            width: keyScroll.availableWidth
            spacing: Kirigami.Units.largeSpacing

            ColumnLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Para: %1").arg(picker.recipient)
                    textFormat: Text.PlainText
                    font.weight: Font.DemiBold
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Elige cuál de tus claves privadas quieres enviar al teléfono.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                visible: picker.keys.length > 0
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: picker.keys
                    delegate: KeyListDelegate {
                        required property var modelData
                        Layout.fillWidth: true
                        padding: Kirigami.Units.smallSpacing
                        horizontalPadding: 0
                        entry: modelData
                        primary: picker.host.keyStore.isPrimary(String(modelData.id))
                        highlighted: picker.selectedFingerprint === String(modelData.id)
                        enabled: picker.host.canTransferKey(modelData) && !picker.host.keyStore.controller.busy
                        Accessible.selected: highlighted
                        onClicked: picker.selectedFingerprint = String(modelData.id)
                    }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    visible: picker.selectedKey !== null
                    text: qsTr("Huella completa: %1").arg(picker.selectedFingerprint.replace(/(.{4})/g, "$1 ").trim())
                    textFormat: Text.PlainText
                    font.family: "monospace"
                    wrapMode: Text.WrapAnywhere
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: picker.selectedKey !== null && !picker.host.keyStore.isPrimary(picker.selectedFingerprint)
                    text: qsTr("Esta clave no es la principal de la bóveda activa. Enviarla no cambia la principal ni da acceso a los elementos cifrados solo con ella.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Por ahora, el teléfono admite una sola clave importada. No reemplaza una existente.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                visible: picker.host.keyStore.controller.busy || !picker.sessionCurrent || (picker.selectedKey !== null && !picker.keyCurrent) || picker.host.keyStore.controller.loadError.length > 0 || picker.keys.length === 0
                spacing: Kirigami.Units.smallSpacing
                Controls.BusyIndicator {
                    Layout.alignment: Qt.AlignHCenter
                    running: picker.host.keyStore.controller.busy
                    visible: running
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !picker.sessionCurrent || (picker.selectedKey !== null && !picker.keyCurrent)
                    type: Kirigami.MessageType.Warning
                    text: qsTr("La conexión o la clave elegida ya no está disponible. Cierra y vuelve a elegir antes de enviarla.")
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: picker.host.keyStore.controller.loadError.length > 0
                    type: Kirigami.MessageType.Error
                    text: picker.host.keyStore.controller.loadError
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !picker.host.keyStore.controller.busy && picker.keys.length === 0
                    text: qsTr("No hay claves privadas disponibles en GnuPG. Revisa tus claves antes de enviarlas.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }
            Controls.ToolButton {
                text: qsTr("Actualizar claves")
                icon.name: "view-refresh"
                enabled: !picker.host.keyStore.controller.busy
                onClicked: picker.host.keyStore.controller.refreshKeys()
            }
        }
    }
    footer: Controls.DialogButtonBox {
        padding: picker.padding
        spacing: Kirigami.Units.smallSpacing * 2
        Controls.Button {
            text: qsTr("Cancelar")
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
        }
        Controls.Button {
            text: qsTr("Revisar envío")
            icon.name: "go-next"
            enabled: picker.canContinue
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
            onClicked: {
                if (!picker.canContinue)
                    return;
                const fingerprint = picker.selectedFingerprint;
                const key = picker.selectedKey;
                const title = String(key.title || "") + (key.email ? " <" + key.email + ">" : "");
                picker.close();
                picker.keyChosen(fingerprint, title);
            }
        }
        onRejected: picker.close()
    }
}
