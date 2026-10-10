import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls

Controls.Dialog {
    id: moveDialog
    required property MockStore storeAdapter
    signal moveRequested(string entryId, string folderId)
    anchors.centerIn: parent
    width: 420
    modal: true
    title: qsTranslate("Main", "Mover elemento")
    standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
    function openMove() {
        destination.currentIndex = Math.max(0, destination.model.findIndex(f => f.id === moveDialog.storeAdapter.selected.folder));
        open();
    }
    onAccepted: moveDialog.moveRequested(moveDialog.storeAdapter.selectedId, destination.currentValue)
    contentItem: ColumnLayout {
        Controls.Label {
            Layout.fillWidth: true
            text: qsTranslate("Main", "Mover solo cambia la carpeta. Los destinatarios se conservan.")
            wrapMode: Text.Wrap
        }
        Controls.ComboBox {
            id: destination
            Layout.fillWidth: true
            model: [
                {
                    id: "",
                    name: qsTranslate("Main", "Sin carpeta")
                }
            ].concat(moveDialog.storeAdapter.folderRows)
            textRole: "name"
            valueRole: "id"
        }
    }
}
