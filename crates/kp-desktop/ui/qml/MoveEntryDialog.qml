pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: moveDialog

    required property SecretStore storeAdapter
    property string entryId: ""
    property string destinationFolderId: ""
    property bool pending: false
    property string operationError: ""

    signal moveRequested(string entryId, string folderId)

    anchors.centerIn: parent
    width: Math.min(420, parent.width - Kirigami.Units.gridUnit * 4)
    modal: true
    closePolicy: Controls.Popup.NoAutoClose
    title: qsTr("Mover elemento")

    function openMove() {
        if (pending || storeAdapter.controller.busy || !storeAdapter.selected)
            return;
        entryId = storeAdapter.selectedId;
        destinationFolderId = storeAdapter.selected.folder || "";
        operationError = "";
        open();
    }

    function submit() {
        if (pending || storeAdapter.controller.busy || !entryId)
            return;
        pending = true;
        operationError = "";
        moveRequested(entryId, destinationFolderId);
    }

    function finishSuccess() {
        pending = false;
        entryId = "";
        destinationFolderId = "";
        close();
    }

    readonly property Connections controllerConnections: Connections {
        target: moveDialog.storeAdapter.controller
        function onOperationSucceeded() {
            if (moveDialog.pending)
                moveDialog.finishSuccess();
        }
        function onOperationFailed() {
            if (!moveDialog.pending || moveDialog.storeAdapter.controller.busy)
                return;
            moveDialog.pending = false;
            moveDialog.operationError = moveDialog.storeAdapter.controller.errorMessage || qsTr("No se pudo mover el elemento.");
        }
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: !!moveDialog.operationError
            type: Kirigami.MessageType.Error
            text: moveDialog.operationError
        }
        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Mover solo cambia la carpeta. Se conserva quién tiene acceso.")
            wrapMode: Text.Wrap
        }
        FolderPicker {
            id: destination
            Layout.fillWidth: true
            enabled: !moveDialog.pending && !moveDialog.storeAdapter.controller.busy
            folderRows: moveDialog.storeAdapter.folderRows
            selectedId: moveDialog.destinationFolderId
            accessibleLabel: qsTr("Carpeta de destino")
            onFolderSelected: function(folderId) {
                moveDialog.destinationFolderId = folderId;
            }
        }
    }

    footer: RowLayout {
        Controls.BusyIndicator {
            visible: moveDialog.pending
            running: visible
            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
            Layout.preferredHeight: width
        }
        Item { Layout.fillWidth: true }
        Controls.Button {
            text: qsTr("Cancelar")
            enabled: !moveDialog.pending && !moveDialog.storeAdapter.controller.busy
            onClicked: moveDialog.close()
        }
        Controls.Button {
            text: moveDialog.pending ? qsTr("Moviendo…") : qsTr("Mover")
            icon.name: "folder-move"
            enabled: !moveDialog.pending && !moveDialog.storeAdapter.controller.busy && !!moveDialog.entryId
            onClicked: moveDialog.submit()
        }
    }
}
