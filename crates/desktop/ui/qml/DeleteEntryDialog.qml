import QtQuick
import QtQuick.Controls as Controls

Controls.Dialog {
    id: deleteEntryDialog
    required property MockStore storeAdapter
    signal deleteRequested(string entryId)
    anchors.centerIn: parent
    modal: true
    title: qsTranslate("Main", "Eliminar definitivamente")
    standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
    onAccepted: deleteEntryDialog.deleteRequested(storeAdapter.selectedId)
    Controls.Label {
        text: qsTranslate("Main", "¿Eliminar este elemento de la demostración?")
    }
}
