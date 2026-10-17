pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: dialog
    required property SecretStore storeAdapter
    property string folderId: ""
    property string destinationId: ""
    property string operationError: ""
    property bool pending: false
    anchors.centerIn: parent
    width: Math.min(parent.width - Kirigami.Units.largeSpacing * 4, Kirigami.Units.gridUnit * 24)
    title: qsTr("Mover carpeta")
    modal: true
    closePolicy: pending ? Controls.Popup.NoAutoClose : Controls.Popup.CloseOnEscape
    standardButtons: Controls.Dialog.NoButton

    function prepare(id) {
        folderId = id;
        destinationId = storeAdapter.folders.find(folder => folder.id === id)?.parent || "";
        operationError = "";
        pending = false;
        open();
        Qt.callLater(destination.forceActiveFocus);
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Elige la carpeta superior. También se moverán las subcarpetas y los elementos.")
            wrapMode: Text.Wrap
        }
        FolderPicker {
            id: destination
            Layout.fillWidth: true
            enabled: !dialog.pending
            folderRows: dialog.storeAdapter.folderRows.filter(folder => dialog.storeAdapter.canMoveFolder(dialog.folderId, folder.id))
            selectedId: dialog.destinationId
            emptyLabel: qsTr("Carpetas principales")
            accessibleLabel: qsTr("Carpeta de destino")
            onFolderSelected: function (id) {
                dialog.destinationId = id;
            }
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: !!dialog.operationError
            type: Kirigami.MessageType.Error
            text: dialog.operationError
        }
    }
    footer: Controls.DialogButtonBox {
        Controls.Button {
            text: qsTr("Cancelar")
            enabled: !dialog.pending
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
        }
        Controls.Button {
            text: dialog.pending ? qsTr("Moviendo…") : qsTr("Mover")
            enabled: !dialog.pending && !dialog.storeAdapter.busy
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.AcceptRole
        }
        onRejected: dialog.close()
        onAccepted: {
            dialog.pending = true;
            dialog.operationError = "";
            dialog.storeAdapter.controller.moveFolder(dialog.folderId, dialog.destinationId, "");
        }
    }
    Connections {
        target: dialog.storeAdapter.controller
        function onOperationSucceeded() {
            if (dialog.pending) {
                dialog.pending = false;
                dialog.close();
            }
        }
        function onOperationFailed() {
            if (dialog.pending) {
                dialog.pending = false;
                dialog.operationError = dialog.storeAdapter.errorMessage;
            }
        }
        function onVaultLocked() {
            dialog.close();
        }
    }
}
