pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: deleteEntryDialog

    required property SecretStore storeAdapter
    property string entryId: ""
    property bool pending: false
    property string operationError: ""

    signal deleteRequested(string entryId)

    anchors.centerIn: parent
    width: Math.min(420, parent.width - Kirigami.Units.gridUnit * 4)
    modal: true
    closePolicy: Controls.Popup.NoAutoClose
    title: qsTr("Eliminar definitivamente")

    function openForSelected() {
        if (pending || storeAdapter.controller.busy || !storeAdapter.selected)
            return;
        entryId = storeAdapter.selectedId;
        operationError = "";
        open();
    }

    function submit() {
        if (pending || storeAdapter.controller.busy || !entryId)
            return;
        pending = true;
        operationError = "";
        deleteRequested(entryId);
    }

    function finishSuccess() {
        pending = false;
        entryId = "";
        close();
    }

    readonly property Connections controllerConnections: Connections {
        target: deleteEntryDialog.storeAdapter.controller
        function onOperationSucceeded() {
            if (deleteEntryDialog.pending)
                deleteEntryDialog.finishSuccess();
        }
        function onOperationFailed() {
            if (!deleteEntryDialog.pending || deleteEntryDialog.storeAdapter.controller.busy)
                return;
            deleteEntryDialog.pending = false;
            deleteEntryDialog.operationError = deleteEntryDialog.storeAdapter.controller.errorMessage || qsTr("No se pudo eliminar el elemento.");
        }
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: !!deleteEntryDialog.operationError
            type: Kirigami.MessageType.Error
            text: deleteEntryDialog.operationError
        }
        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("¿Eliminar definitivamente «%1»? Esta acción no se puede deshacer.").arg(deleteEntryDialog.storeAdapter.entries.find(entry => entry.id === deleteEntryDialog.entryId)?.title || "")
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
    }

    footer: RowLayout {
        Controls.BusyIndicator {
            visible: deleteEntryDialog.pending
            running: visible
            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
            Layout.preferredHeight: width
        }
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            text: qsTr("Cancelar")
            enabled: !deleteEntryDialog.pending && !deleteEntryDialog.storeAdapter.controller.busy
            onClicked: deleteEntryDialog.close()
        }
        Controls.Button {
            text: deleteEntryDialog.pending ? qsTr("Eliminando…") : qsTr("Eliminar definitivamente")
            icon.name: "edit-delete"
            enabled: !deleteEntryDialog.pending && !deleteEntryDialog.storeAdapter.controller.busy && !!deleteEntryDialog.entryId
            onClicked: deleteEntryDialog.submit()
        }
    }
}
