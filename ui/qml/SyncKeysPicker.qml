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
    property string recipient: ""
    // Upper-case fingerprints ticked in the list.
    property var chosen: []
    // Fingerprints the phone declared holding, or null while unknown (offline or an older app).
    property var phoneKeys: null
    readonly property var localKeys: host.keyStore.entries.filter(key => key.hasSecret && key.statusCode === "valid" && /^[A-Fa-f0-9]{40}$/.test(String(key.id)))
    readonly property var keys: phoneKeys === null ? localKeys : localKeys.filter(key => phoneKeys.indexOf(String(key.id).toUpperCase()) >= 0)
    signal keysChosen(string deviceId, string fingerprints)
    title: KI18n.i18nc("@title:window", "Keys to Sync")
    modal: true
    padding: Kirigami.Units.largeSpacing
    width: Math.min(Kirigami.Units.gridUnit * 28, host.width - Kirigami.Units.gridUnit * 2)
    standardButtons: Controls.Dialog.Cancel | Controls.Dialog.Ok
    onAboutToShow: {
        host.keyStore.controller.refreshKeys();
        // Known phone keys: offer all that both have. Unknown: only the primary, as a starting point.
        const start = phoneKeys === null ? keys.filter(key => host.keyStore.isPrimary(String(key.id))) : keys;
        chosen = start.map(key => String(key.id).toUpperCase());
    }
    onAccepted: keysChosen(deviceId, chosen.join(","))
    Component.onCompleted: standardButton(Controls.Dialog.Ok).enabled = Qt.binding(() => picker.chosen.length > 0)

    function toggle(fingerprint, on) {
        const id = String(fingerprint).toUpperCase();
        const rest = chosen.filter(item => item !== id);
        chosen = on ? rest.concat([id]) : rest;
    }

    contentItem: Controls.ScrollView {
        id: scroll
        contentWidth: availableWidth
        implicitHeight: Math.min(column.implicitHeight, Math.max(Kirigami.Units.gridUnit * 6, picker.host.height - Kirigami.Units.gridUnit * 9))
        clip: true
        ColumnLayout {
            id: column
            width: scroll.availableWidth
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
                text: picker.phoneKeys === null ? KI18n.i18n("Items encrypted only with one of the checked keys are synced. Which keys the phone has could not be checked (disconnected or an older version): check only the ones it has, or it will not be able to open those items.") : KI18n.i18n("Items encrypted only with one of the checked keys are synced. Only keys that both this computer and the phone have are shown.")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }
            Repeater {
                model: picker.keys
                delegate: Controls.CheckDelegate {
                    id: row
                    required property var modelData
                    Layout.fillWidth: true
                    text: modelData.title + (modelData.email ? " · " + modelData.email : "")
                    checked: picker.chosen.indexOf(String(modelData.id).toUpperCase()) >= 0
                    onToggled: picker.toggle(modelData.id, checked)
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.text: String(modelData.id)
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: picker.keys.length === 0
                text: picker.phoneKeys === null ? KI18n.i18n("There are no keys with a valid private key on this computer.") : KI18n.i18n("The phone has none of this computer's keys. Send it a key from Devices first.")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }
}
