pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// Floating Sync status anchored to the footer indicator, like the Sync view of the phone:
// what is happening with each device, which keys are synced and what to do about it,
// without opening the whole devices window.
Controls.Popup {
    id: popup
    // The devices window (it owns the key store and the device list) and the controller.
    required property var host
    required property var controller
    signal openDevicesRequested

    readonly property var syncDevices: (host.devices || []).filter(device => controller.vaultSyncPeers.indexOf(String(device.deviceId)) >= 0)

    modal: false
    focus: true
    closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside
    padding: Kirigami.Units.largeSpacing
    width: Math.min(Kirigami.Units.gridUnit * 25, (parent ? parent.Window.width : 0) - Kirigami.Units.gridUnit * 2)
    height: Math.min(content.implicitHeight + topPadding + bottomPadding, (parent ? parent.Window.height : 0) * 0.75)
    // Opens upwards from the indicator, aligned to its right edge.
    x: parent ? parent.width - width : 0
    y: parent ? -height - Kirigami.Units.smallSpacing : 0

    contentItem: Controls.ScrollView {
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            id: content
            width: popup.availableWidth
            spacing: Kirigami.Units.largeSpacing

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 4
                text: qsTr("Sincronización de la bóveda")
                wrapMode: Text.Wrap
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: popup.controller.vaultSyncFailed && popup.controller.vaultSyncMessage.length > 0
                type: Kirigami.MessageType.Error
                text: popup.controller.vaultSyncMessage
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: popup.controller.vaultSyncLockedKey.length > 0
                type: Kirigami.MessageType.Warning
                text: qsTr("La sincronización está detenida: falta la contraseña de la clave de %1.").arg(popup.host.keyTitle(popup.controller.vaultSyncLockedKey))
                actions: [
                    Kirigami.Action {
                        text: qsTr("Desbloquear clave…")
                        icon.name: "dialog-password"
                        onTriggered: popup.controller.unlockSyncKey(popup.controller.vaultSyncLockedKey)
                    }
                ]
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: popup.syncDevices.length === 0
                text: qsTr("Ningún dispositivo sincroniza esta bóveda. Actívalo desde la ventana de dispositivos.")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }

            Repeater {
                model: popup.syncDevices
                delegate: ColumnLayout {
                    id: deviceBlock
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.largeSpacing
                        Kirigami.Icon {
                            source: "smartphone"
                            implicitWidth: Kirigami.Units.iconSizes.smallMedium
                            implicitHeight: implicitWidth
                            Accessible.ignored: true
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            Controls.Label {
                                Layout.fillWidth: true
                                text: popup.host.deviceName(deviceBlock.modelData)
                                textFormat: Text.PlainText
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: deviceBlock.modelData.connection === "connected" ? qsTr("Conectado") + (deviceBlock.modelData.routeLabel ? " · " + deviceBlock.modelData.routeLabel : "") : qsTr("Sin conexión. Se sincroniza al reconectar.")
                                textFormat: Text.PlainText
                                font: Kirigami.Theme.smallFont
                                color: Kirigami.Theme.disabledTextColor
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                    SyncKeyList {
                        Layout.fillWidth: true
                        host: popup.host
                        deviceId: String(deviceBlock.modelData.deviceId)
                        device: deviceBlock.modelData
                    }
                }
            }

            Controls.Button {
                Layout.alignment: Qt.AlignRight
                flat: true
                text: qsTr("Administrar dispositivos…")
                icon.name: "configure"
                onClicked: popup.openDevicesRequested()
            }
        }
    }
}
