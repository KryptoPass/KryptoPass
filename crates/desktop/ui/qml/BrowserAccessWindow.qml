pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls

Controls.ApplicationWindow {
    id: requestWindow
    required property string requestJson
    required property bool vaultLocked
    property var entryMetadata: []
    property var kindOptions: []
    property var folders: []
    property var folderRows: []
    property bool dismissed: false

    signal decisionRequested(string token, string entryId, bool approve)
    signal usernameRequested(string token, string entryId)

    transientParent: null
    flags: Qt.Dialog | Qt.WindowTitleHint | Qt.WindowCloseButtonHint | Qt.WindowStaysOnTopHint
    modality: Qt.NonModal
    title: picker.title
    width: 640
    height: 520
    minimumWidth: 400
    minimumHeight: 480
    x: screen.virtualX + Math.round((screen.width - width) / 2)
    y: screen.virtualY + Math.round((screen.height - height) / 2)
    visible: !!requestJson && !vaultLocked && !dismissed

    onRequestJsonChanged: dismissed = false
    onVisibleChanged: {
        if (visible) {
            Qt.callLater(function() {
                if (requestWindow.visible) {
                    requestWindow.raise();
                    requestWindow.requestActivate();
                    picker.open();
                }
            });
        } else {
            if (requestJson)
                picker.decide("", false);
            picker.resetSelection();
            picker.close();
        }
    }
    onClosing: function(event) {
        event.accepted = false;
        picker.decide("", false);
        dismissed = true;
    }

    BrowserAccessDialog {
        id: picker
        objectName: "browserAccessPicker"
        parent: requestWindow.contentItem
        margins: 0
        width: parent.width
        height: parent.height
        requestJson: requestWindow.requestJson
        vaultLocked: requestWindow.vaultLocked
        entryMetadata: requestWindow.entryMetadata
        kindOptions: requestWindow.kindOptions
        folders: requestWindow.folders
        folderRows: requestWindow.folderRows
        onUsernameRequested: function(token, entryId) { requestWindow.usernameRequested(token, entryId); }
        onDecisionRequested: function(token, entryId, approve) {
            requestWindow.dismissed = true;
            requestWindow.decisionRequested(token, entryId, approve);
        }
    }
}
