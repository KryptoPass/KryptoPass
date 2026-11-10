pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// gpg-agent asks to use a key through kryptopass-pinentry. Deny is the default:
// closing or pressing Escape refuses. Works while the vault is locked.
Controls.Dialog {
    id: dialog
    required property string requestJson
    signal decisionRequested(string id, bool approve)
    signal attentionRequested
    readonly property var request: requestJson ? JSON.parse(requestJson) : null
    property string decidedId: ""
    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 30, parent.width - Kirigami.Units.gridUnit * 4)
    modal: true
    title: request && request.ssh ? qsTr("Usar clave SSH") : qsTr("Usar clave OpenPGP")
    closePolicy: Controls.Popup.CloseOnEscape

    onRequestChanged: {
        if (request) {
            decidedId = "";
            open();
            attentionRequested();
            denyButton.forceActiveFocus();
        } else {
            close();
        }
    }
    onClosed: {
        if (request && decidedId !== request.id)
            decide(false);
    }
    function decide(approve) {
        if (!request || decidedId === request.id)
            return;
        decidedId = request.id;
        decisionRequested(request.id, approve);
    }
    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Controls.Label {
            Layout.fillWidth: true
            text: dialog.request ? dialog.request.description : ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        Kirigami.FormLayout {
            Layout.fillWidth: true
            Controls.Label {
                Kirigami.FormData.label: qsTr("Lo pide:")
                text: dialog.request && dialog.request.requester ? dialog.request.requester : qsTr("Proceso desconocido")
                textFormat: Text.PlainText
            }
            Controls.Label {
                Kirigami.FormData.label: qsTr("Clave:")
                visible: !!(dialog.request && dialog.request.key)
                text: dialog.request && dialog.request.key ? dialog.request.key : ""
                textFormat: Text.PlainText
                elide: Text.ElideMiddle
                Layout.maximumWidth: Kirigami.Units.gridUnit * 18
            }
        }
    }
    footer: RowLayout {
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            id: denyButton
            text: qsTr("Denegar")
            onClicked: dialog.decide(false)
            Keys.onReturnPressed: dialog.decide(false)
            Keys.onEnterPressed: dialog.decide(false)
        }
        Controls.Button {
            text: qsTr("Permitir")
            onClicked: dialog.decide(true)
        }
    }
}
