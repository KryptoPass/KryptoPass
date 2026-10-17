pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: dialog
    required property string requestToken
    required property bool vaultLocked
    signal decisionRequested(string token, bool approve)
    anchors.centerIn: parent
    width: Math.min(480, parent.width - Kirigami.Units.gridUnit * 4)
    modal: true
    title: qsTr("Conectar el navegador")
    closePolicy: Controls.Popup.CloseOnEscape

    onRequestTokenChanged: {
        if (requestToken) {
            open();
            rejectButton.forceActiveFocus();
        } else {
            close();
        }
    }
    onClosed: {
        if (requestToken)
            decide(false);
    }
    function decide(approve) {
        decisionRequested(requestToken, approve);
    }
    contentItem: Controls.Label {
        text: qsTr("La extensión solicita vincularse con esta bóveda. Acepta solo si acabas de iniciar la conexión desde tu navegador.\n\nLa vinculación se conservará al reiniciar KryptoPass. Cada solicitud de relleno requerirá tu autorización.")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }
    footer: RowLayout {
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            id: rejectButton
            text: qsTr("Rechazar conexión")
            onClicked: dialog.decide(false)
            Keys.onReturnPressed: dialog.decide(false)
            Keys.onEnterPressed: dialog.decide(false)
        }
        Controls.Button {
            text: qsTr("Vincular navegador")
            enabled: !dialog.vaultLocked
            onClicked: dialog.decide(true)
            Keys.onReturnPressed: {
                if (enabled)
                    dialog.decide(true);
            }
            Keys.onEnterPressed: {
                if (enabled)
                    dialog.decide(true);
            }
        }
    }
}
