pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

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
    title: KI18n.i18nc("@title:window", "Choose Key to Send")
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
                    text: KI18n.i18n("To: %1", picker.recipient)
                    textFormat: Text.PlainText
                    font.weight: Font.DemiBold
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: KI18n.i18n("Choose which of your private keys to send to the phone.")
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
                    text: KI18n.i18n("Full fingerprint: %1", picker.selectedFingerprint.replace(/(.{4})/g, "$1 ").trim())
                    textFormat: Text.PlainText
                    font.family: "monospace"
                    wrapMode: Text.WrapAnywhere
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: picker.selectedKey !== null && !picker.host.keyStore.isPrimary(picker.selectedFingerprint)
                    text: KI18n.i18n("This key is not the active vault's main key. Sending it does not change the main key or give access to items encrypted only with it.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: KI18n.i18n("For now, the phone accepts a single imported key. It does not replace an existing one.")
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
                    text: KI18n.i18n("The connection or the chosen key is no longer available. Close and choose again before sending it.")
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
                    text: KI18n.i18n("No private keys are available in GnuPG. Check your keys before sending them.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }
            Controls.ToolButton {
                text: KI18n.i18nc("@action:button", "Refresh Keys")
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
            text: KI18n.i18nc("@action:button", "Cancel")
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Review Transfer")
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
