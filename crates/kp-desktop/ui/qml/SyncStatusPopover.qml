pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// Floating Sync status anchored to the footer indicator, as compact as the phone's: one
// row per device with its name, a refresh icon and what it is doing. Everything it says
// comes from the shared SyncStatusModel.
Controls.Popup {
    id: popup
    required property SyncStatusModel model
    // Asks the owner to open the devices window, on one device when its id is given.
    signal openDevices(string deviceId)

    modal: false
    focus: true
    closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside
    padding: Kirigami.Units.largeSpacing
    width: Math.min(Kirigami.Units.gridUnit * 20, (parent ? parent.Window.width : 0) - Kirigami.Units.gridUnit * 2)
    // Opens upwards from the indicator, aligned to its right edge.
    x: parent ? parent.width - width : 0
    y: parent ? -height - Kirigami.Units.smallSpacing : 0

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        Controls.Label {
            Layout.fillWidth: true
            visible: popup.model.devices.length === 0
            text: qsTr("Ningún dispositivo vinculado. La bóveda solo está en este equipo.")
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Kirigami.Theme.disabledTextColor
        }
        Controls.Button {
            visible: popup.model.devices.length === 0
            icon.name: "list-add"
            text: qsTr("Vincular un dispositivo")
            onClicked: popup.openDevices("")
        }
        Repeater {
            model: popup.model.devices
            delegate: RowLayout {
                id: row
                required property var modelData
                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Controls.Label {
                        Layout.fillWidth: true
                        text: row.modelData.name
                        textFormat: Text.PlainText
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: row.modelData.stats + (row.modelData.route ? " · " + row.modelData.route : "") + " · " + qsTr("%n clave", "", row.modelData.keys)
                        textFormat: Text.PlainText
                        font: Kirigami.Theme.smallFont
                        color: Kirigami.Theme.disabledTextColor
                        elide: Text.ElideRight
                    }
                }
                Controls.Button {
                    visible: !row.modelData.syncs
                    text: qsTr("Elegir claves")
                    onClicked: popup.openDevices(row.modelData.id)
                }
                Controls.ToolButton {
                    visible: row.modelData.syncs
                    icon.name: "view-refresh"
                    display: Controls.AbstractButton.IconOnly
                    enabled: row.modelData.connected
                    text: qsTr("Actualizar")
                    onClicked: popup.model.retry(row.modelData.id)
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.text: text
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }
        }
    }
}
