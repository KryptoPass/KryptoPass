pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Dialog {
    id: dialog
    required property string requestToken
    required property bool vaultLocked
    signal decisionRequested(string token, bool approve)
    anchors.centerIn: parent
    width: Math.min(480, parent.width - Kirigami.Units.gridUnit * 4)
    modal: true
    title: KI18n.i18nc("@title:window", "Connect the Browser")
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
        text: KI18n.i18n("The extension is asking to pair with this vault. Accept only if you just started the connection from your browser.\n\nThe pairing is kept when KryptoPass restarts. Every fill request will require your authorization.")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }
    footer: RowLayout {
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            id: rejectButton
            text: KI18n.i18nc("@action:button", "Reject Connection")
            onClicked: dialog.decide(false)
            Keys.onReturnPressed: dialog.decide(false)
            Keys.onEnterPressed: dialog.decide(false)
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Pair Browser")
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
