import QtQuick
import QtQuick.Controls as Controls

Controls.Dialog {
    id: deleteFolderDialog
    required property MockStore storeAdapter
    signal deleteRequested(string folderId)
    anchors.centerIn: parent
    modal: true
    title: qsTranslate("Main", "Eliminar carpeta")
    standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
    onAccepted: deleteFolderDialog.deleteRequested(storeAdapter.scope)
    Controls.Label {
        text: qsTranslate("Main", "Se eliminará la carpeta. Sus elementos y subcarpetas pasarán a la carpeta superior.")
        width: 380
        wrapMode: Text.Wrap
    }
}
