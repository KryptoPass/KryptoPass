pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

Controls.Dialog {
    id: dialog
    anchors.centerIn: parent
    width: Math.min(560, parent.width - Kirigami.Units.gridUnit * 2)
    height: Math.min(680, parent.height - Kirigami.Units.gridUnit * 2)
    modal: true
    title: qsTr("Vincular con Android — pruebas")
    closePolicy: Controls.Popup.CloseOnEscape
    readonly property bool available: pairing.available
    signal copyRequested(string code)

    Backend.PairingLabController {
        id: pairing
    }
    onOpened: {
        pairing.loadNetworkAddresses();
        pairing.loadDevices();
    }
    onClosed: pairing.cancel()

    contentItem: Controls.ScrollView {
        contentWidth: availableWidth
        ColumnLayout {
            width: parent.width
            spacing: Kirigami.Units.largeSpacing
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Prueba Desktop ↔ Android. Este perfil está separado de tu bóveda y no transfiere contraseñas ni claves OpenPGP.")
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
            }
            Controls.Label {
                Layout.fillWidth: true
                text: pairing.status
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                Accessible.role: Accessible.StaticText
            }
            Controls.CheckBox {
                text: qsTr("Conectar por LAN con WebRTC")
                checked: pairing.lanEnabled
                enabled: !pairing.busy
                onToggled: pairing.configureNetwork(checked, pairing.networkIndex)
            }
            Controls.ComboBox {
                Layout.fillWidth: true
                visible: pairing.lanEnabled
                enabled: !pairing.busy
                model: pairing.networkAddresses
                currentIndex: pairing.networkIndex
                displayText: currentIndex < 0 ? qsTr("Sin dirección local disponible") : qsTr("Dirección de Desktop: %1").arg(currentText)
                Accessible.name: qsTr("Interfaz de red local de Desktop")
                onActivated: index => pairing.configureNetwork(true, index)
            }
            Controls.Label {
                Layout.fillWidth: true
                text: pairing.lanEnabled ? qsTr("Conecta ambos dispositivos a la misma red. Crea un código y pégalo en Android; buscar Desktop en la red es opcional.") : qsTr("Modo anterior para el emulador de esta computadora.")
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Dispositivos autorizados")
                visible: pairing.deviceIds.length > 0
            }
            Controls.ComboBox {
                Layout.fillWidth: true
                visible: pairing.deviceIds.length > 0
                enabled: !pairing.busy
                model: pairing.deviceIds
                currentIndex: pairing.selectedIndex
                displayText: currentIndex < 0 ? qsTr("Selecciona un dispositivo") : qsTr("Dispositivo %1…").arg(currentText.slice(0, 16))
                Accessible.name: qsTr("Dispositivo autorizado para reconectar")
                onActivated: index => pairing.selectDevice(index)
            }
            Controls.TextArea {
                Layout.fillWidth: true
                visible: pairing.bootstrap.length > 0
                text: pairing.bootstrap
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                Accessible.name: qsTr("Código de invitación de un solo uso")
            }
            Controls.Button {
                visible: pairing.bootstrap.length > 0
                text: qsTr("Copiar código")
                onClicked: dialog.copyRequested(pairing.bootstrap)
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: pairing.sas.length > 0
                text: pairing.sas
                textFormat: Text.PlainText
                font.pixelSize: 22
                wrapMode: Text.Wrap
                Accessible.name: qsTr("Siete emojis y nombres para comparar")
            }
            RowLayout {
                visible: pairing.awaiting
                Controls.Button {
                    text: qsTr("No coinciden")
                    onClicked: pairing.decide(false)
                }
                Controls.Button {
                    text: qsTr("Los siete coinciden")
                    onClicked: pairing.decide(true)
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: pairing.peer.length > 0
                text: qsTr("Dispositivo seleccionado: %1").arg(pairing.peer)
                wrapMode: Text.WrapAnywhere
                textFormat: Text.PlainText
            }
            Item {
                Layout.fillHeight: true
            }
        }
    }
    footer: RowLayout {
        Controls.Button {
            text: qsTr("Nueva invitación")
            enabled: !pairing.busy && pairing.available
            onClicked: pairing.startPairing()
        }
        Controls.Button {
            text: qsTr("Reconectar")
            enabled: !pairing.busy && pairing.canReconnect
            onClicked: pairing.startReconnect()
        }
        Controls.Button {
            text: qsTr("Actualizar")
            enabled: !pairing.busy && pairing.available
            onClicked: {
                pairing.loadNetworkAddresses();
                pairing.loadDevices();
            }
        }
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            text: pairing.busy ? qsTr("Cancelar") : qsTr("Cerrar")
            onClicked: {
                if (pairing.busy)
                    pairing.cancel();
                else
                    dialog.close();
            }
        }
    }
}
