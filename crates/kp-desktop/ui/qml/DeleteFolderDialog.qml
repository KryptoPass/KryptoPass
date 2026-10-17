pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

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
    title: qsTr("Eliminar carpeta")

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
            deleteFolderDialog.operationError = deleteFolderDialog.storeAdapter.controller.errorMessage || qsTr("No se pudo eliminar la carpeta.");
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
            text: qsTr("¿Eliminar la carpeta «%1»?").arg(deleteFolderDialog.storeAdapter.folders.find(folder => folder.id === deleteFolderDialog.folderId)?.name || "")
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        Controls.Label {
            Layout.fillWidth: true
            text: deleteContentsCheck.checked
                ? qsTr("La carpeta, sus subcarpetas y todos sus elementos, incluidos los de la papelera, se eliminarán definitivamente. Esta acción no se puede deshacer.")
                : qsTr("Sus elementos y subcarpetas pasarán a la carpeta superior.")
            wrapMode: Text.Wrap
        }
        Controls.CheckBox {
            id: deleteContentsCheck
            Layout.fillWidth: true
            text: qsTr("Eliminar también los elementos y subcarpetas")
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
        Item { Layout.fillWidth: true }
        Controls.Button {
            text: qsTr("Cancelar")
            enabled: !deleteFolderDialog.pending && !deleteFolderDialog.storeAdapter.controller.busy
            onClicked: deleteFolderDialog.close()
        }
        Controls.Button {
            text: deleteFolderDialog.pending ? qsTr("Eliminando…")
                : deleteContentsCheck.checked ? qsTr("Eliminar definitivamente") : qsTr("Eliminar carpeta")
            icon.name: "edit-delete"
            enabled: !deleteFolderDialog.pending && !deleteFolderDialog.storeAdapter.controller.busy && !!deleteFolderDialog.folderId
            onClicked: deleteFolderDialog.submit()
        }
    }
}
