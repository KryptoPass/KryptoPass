pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Dialog {
    id: deleteFolderDialog

    required property SecretStore storeAdapter
    property string folderId: ""
    property bool pending: false
    property string operationError: ""

    signal deleteRequested(string folderId, bool deleteContents)

    anchors.centerIn: parent
    width: Math.min(440, parent.width - Kirigami.Units.gridUnit * 4)
    modal: true
    closePolicy: Controls.Popup.NoAutoClose
    title: KI18n.i18nc("@action:inmenu", "Delete Folder")

    function openForSelected() {
        if (pending || storeAdapter.controller.busy || ["all", "favorites", "trash"].includes(storeAdapter.scope))
            return;
        folderId = storeAdapter.scope;
        operationError = "";
        deleteContentsCheck.checked = false;
        open();
    }

    function submit() {
        if (pending || storeAdapter.controller.busy || !folderId)
            return;
        pending = true;
        operationError = "";
        deleteRequested(folderId, deleteContentsCheck.checked);
    }

    function finishSuccess() {
        pending = false;
        folderId = "";
        close();
    }

    readonly property Connections controllerConnections: Connections {
        target: deleteFolderDialog.storeAdapter.controller
        function onOperationSucceeded() {
            if (deleteFolderDialog.pending)
                deleteFolderDialog.finishSuccess();
        }
        function onOperationFailed() {
            if (!deleteFolderDialog.pending || deleteFolderDialog.storeAdapter.controller.busy)
                return;
            deleteFolderDialog.pending = false;
            deleteFolderDialog.operationError = deleteFolderDialog.storeAdapter.controller.errorMessage || KI18n.i18n("The folder could not be deleted.");
        }
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: !!deleteFolderDialog.operationError
            type: Kirigami.MessageType.Error
            text: deleteFolderDialog.operationError
        }
        Controls.Label {
            Layout.fillWidth: true
            text: KI18n.i18n("Delete the folder “%1”?", deleteFolderDialog.storeAdapter.folders.find(folder => folder.id === deleteFolderDialog.folderId)?.name || "")
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        Controls.Label {
            Layout.fillWidth: true
            text: deleteContentsCheck.checked ? KI18n.i18n("The folder, its subfolders and all their items, including those in the trash, will be permanently deleted. This action cannot be undone.") : KI18n.i18n("Its items and subfolders will move to the parent folder.")
            wrapMode: Text.Wrap
        }
        Controls.CheckBox {
            id: deleteContentsCheck
            Layout.fillWidth: true
            text: KI18n.i18nc("@option:check", "Also delete the items and subfolders")
            enabled: !deleteFolderDialog.pending && !deleteFolderDialog.storeAdapter.controller.busy
        }
    }

    footer: RowLayout {
        Controls.BusyIndicator {
            visible: deleteFolderDialog.pending
            running: visible
            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
            Layout.preferredHeight: width
        }
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Cancel")
            enabled: !deleteFolderDialog.pending && !deleteFolderDialog.storeAdapter.controller.busy
            onClicked: deleteFolderDialog.close()
        }
        Controls.Button {
            text: deleteFolderDialog.pending ? KI18n.i18nc("@info:progress", "Deleting…") : deleteContentsCheck.checked ? KI18n.i18nc("@action:button", "Delete Permanently") : KI18n.i18nc("@action:inmenu", "Delete Folder")
            icon.name: "edit-delete"
            enabled: !deleteFolderDialog.pending && !deleteFolderDialog.storeAdapter.controller.busy && !!deleteFolderDialog.folderId
            onClicked: deleteFolderDialog.submit()
        }
    }
}
