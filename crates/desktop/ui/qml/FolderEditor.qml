import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls

Controls.Dialog {
    id: folderEditor
    objectName: "folderEditor"
    required property MockStore storeAdapter
    signal saveRequested(string folderId, string name, string parentId)
    property string folderId: ""
    anchors.centerIn: parent
    width: 420
    modal: true
    title: folderId ? qsTranslate("Main", "Renombrar carpeta") : qsTranslate("Main", "Nueva carpeta")
    standardButtons: Controls.Dialog.Save | Controls.Dialog.Cancel
    function openFolder(rename) {
        folderId = rename ? folderEditor.storeAdapter.scope : "";
        folderNameField.text = rename ? folderEditor.storeAdapter.folderName(folderEditor.storeAdapter.scope) : "";
        parentField.currentIndex = Math.max(0, parentField.model.findIndex(f => f.id === (rename ? folderEditor.storeAdapter.folders.find(f => f.id === folderId).parent : folderEditor.storeAdapter.scope)));
        open();
        folderNameField.forceActiveFocus();
    }
    onOpened: standardButton(Controls.Dialog.Save).enabled = Qt.binding(() => folderNameField.text.trim().length > 0)
    onAccepted: folderEditor.saveRequested(folderId, folderNameField.text, parentField.currentValue)
    contentItem: ColumnLayout {
        Controls.Label {
            text: qsTranslate("Main", "Nombre:")
        }
        Controls.TextField {
            id: folderNameField
            Layout.fillWidth: true
        }
        Controls.Label {
            text: qsTranslate("Main", "Dentro de:")
            visible: !folderEditor.folderId
        }
        Controls.ComboBox {
            id: parentField
            Layout.fillWidth: true
            visible: !folderEditor.folderId
            model: [
                {
                    id: "",
                    name: qsTranslate("Main", "Carpetas principales")
                }
            ].concat(folderEditor.storeAdapter.folderRows)
            textRole: "name"
            valueRole: "id"
        }
    }
}
