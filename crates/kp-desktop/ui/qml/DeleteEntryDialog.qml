pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

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
    title: KI18n.i18nc("@action:button", "Delete Permanently")

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
            deleteEntryDialog.operationError = deleteEntryDialog.storeAdapter.controller.errorMessage || KI18n.i18n("The item could not be deleted.");
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
            text: KI18n.i18n("Permanently delete “%1”? This action cannot be undone.", deleteEntryDialog.storeAdapter.entries.find(entry => entry.id === deleteEntryDialog.entryId)?.title || "")
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
            text: KI18n.i18nc("@action:button", "Cancel")
            enabled: !deleteEntryDialog.pending && !deleteEntryDialog.storeAdapter.controller.busy
            onClicked: deleteEntryDialog.close()
        }
        Controls.Button {
            text: deleteEntryDialog.pending ? KI18n.i18n("Deleting…") : KI18n.i18nc("@action:button", "Delete Permanently")
            icon.name: "edit-delete"
            enabled: !deleteEntryDialog.pending && !deleteEntryDialog.storeAdapter.controller.busy && !!deleteEntryDialog.entryId
            onClicked: deleteEntryDialog.submit()
        }
    }
}
