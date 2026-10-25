pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

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
    title: qsTr("Claves para sincronizar")
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
                text: qsTr("Para: %1").arg(picker.recipient)
                textFormat: Text.PlainText
                font.weight: Font.DemiBold
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: picker.phoneKeys === null ? qsTr("Se sincronizan los elementos cifrados solo con una de las claves marcadas. No se pudo comprobar qué claves tiene el teléfono (desconectado o versión anterior): marca únicamente las que tiene, o no podrá abrir esos elementos.") : qsTr("Se sincronizan los elementos cifrados solo con una de las claves marcadas. Solo se muestran las claves que tienen este equipo y el teléfono.")
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
                text: picker.phoneKeys === null ? qsTr("No hay claves con clave privada válida en este equipo.") : qsTr("El teléfono no tiene ninguna de las claves de este equipo. Envíale una clave desde Dispositivos primero.")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }
}
