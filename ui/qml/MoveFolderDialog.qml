pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Dialog {
    id: dialog
    required property SecretStore storeAdapter
    property string folderId: ""
    property string destinationId: ""
    property string operationError: ""
    property bool pending: false
    anchors.centerIn: parent
    width: Math.min(parent.width - Kirigami.Units.largeSpacing * 4, Kirigami.Units.gridUnit * 24)
    title: KI18n.i18nc("@title:window", "Move Folder")
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
            text: KI18n.i18n("Choose the parent folder. Subfolders and items will move too.")
            wrapMode: Text.Wrap
        }
        FolderPicker {
            id: destination
            Layout.fillWidth: true
            enabled: !dialog.pending
            folderRows: dialog.storeAdapter.folderRows.filter(folder => dialog.storeAdapter.canMoveFolder(dialog.folderId, folder.id))
            selectedId: dialog.destinationId
            emptyLabel: KI18n.i18nc("@item:inlistbox", "Top-level folders")
            accessibleLabel: KI18n.i18n("Destination folder")
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
            text: KI18n.i18nc("@action:button", "Cancel")
            enabled: !dialog.pending
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
        }
        Controls.Button {
            text: dialog.pending ? KI18n.i18nc("@info:progress", "Moving…") : KI18n.i18nc("@action:button", "Move")
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
