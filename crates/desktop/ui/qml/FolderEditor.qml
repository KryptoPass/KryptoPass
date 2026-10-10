pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: folderEditor
    objectName: "folderEditor"

    required property SecretStore storeAdapter
    property string folderId: ""
    property string selectedParentId: ""
    property string folderIcon: "folder"
    property string folderColor: "transparent"
    property string baselineJson: ""
    property string saveError: ""
    property bool pending: false
    readonly property bool dirty: baselineJson !== JSON.stringify({
        id: folderId,
        name: folderNameField.text,
        parent: selectedParentId,
        icon: folderIcon,
        color: folderColor
    })
    readonly property bool canSave: !pending && !storeAdapter.controller.busy && folderNameField.text.trim().length > 0 && dirty

    signal saveRequested(string folderId, string name, string parentId, string icon, string color)

    anchors.centerIn: parent
    width: Math.min(460, parent.width - Kirigami.Units.gridUnit * 4)
    height: Math.min(implicitHeight, parent.height - Kirigami.Units.gridUnit * 4)
    padding: 0
    modal: true
    closePolicy: Controls.Popup.NoAutoClose
    title: folderId ? qsTr("Editar carpeta") : qsTr("Nueva carpeta")

    function currentFolderDraft() {
        return {
            id: folderId,
            name: folderNameField.text,
            parent: selectedParentId,
            icon: folderIcon,
            color: folderColor
        };
    }

    function openFolder(rename) {
        if (pending || storeAdapter.controller.busy)
            return;
        folderId = rename ? storeAdapter.scope : "";
        const existing = rename ? storeAdapter.folders.find(folder => folder.id === folderId) : null;
        folderNameField.text = existing ? existing.name : "";
        selectedParentId = existing
                ? existing.parent || ""
                : storeAdapter.folderRows.some(folder => folder.id === storeAdapter.scope) ? storeAdapter.scope : "";
        folderIcon = existing && existing.icon ? existing.icon : "folder";
        folderColor = existing && existing.color ? existing.color : "transparent";
        saveError = "";
        baselineJson = JSON.stringify(currentFolderDraft());
        open();
        Qt.callLater(folderNameField.forceActiveFocus);
    }

    function submit() {
        if (!canSave)
            return;
        pending = true;
        saveError = "";
        saveRequested(folderId, folderNameField.text.trim(), folderId ? "" : selectedParentId, folderIcon, folderColor);
    }

    function requestClose() {
        if (pending || storeAdapter.controller.busy)
            return;
        if (dirty)
            discardDialog.open();
        else
            close();
    }

    function clearAndClose() {
        pending = false;
        folderId = "";
        selectedParentId = "";
        folderIcon = "folder";
        folderColor = "transparent";
        folderNameField.text = "";
        baselineJson = "";
        saveError = "";
        close();
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        context: Qt.WindowShortcut
        enabled: folderEditor.opened && !folderEditor.pending && !folderEditor.storeAdapter.controller.busy && !discardDialog.opened && !parentField.menuOpen
        onActivated: folderEditor.requestClose()
    }

    readonly property Connections controllerConnections: Connections {
        target: folderEditor.storeAdapter.controller

        function onFolderSaved(id) {
            if (!folderEditor.pending || (folderEditor.folderId && folderEditor.folderId !== id))
                return;
            folderEditor.clearAndClose();
        }

        function onOperationFailed() {
            if (!folderEditor.pending || folderEditor.storeAdapter.controller.busy)
                return;
            folderEditor.pending = false;
            folderEditor.saveError = folderEditor.storeAdapter.controller.errorMessage || qsTr("No se pudo guardar la carpeta.");
        }
    }

    contentItem: Controls.ScrollView {
        id: folderScroll
        implicitWidth: folderForm.implicitWidth + 40
        implicitHeight: Math.min(folderForm.implicitHeight + 36,
                                 Math.max(Kirigami.Units.gridUnit * 12,
                                          folderEditor.parent.height - Kirigami.Units.gridUnit * 8))
        contentWidth: availableWidth
        clip: true

        Controls.Pane {
            width: folderScroll.availableWidth
            leftPadding: 20
            rightPadding: 20
            topPadding: 16
            bottomPadding: 20
            background: null

            contentItem: ColumnLayout {
                id: folderForm
                spacing: 20

                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !!folderEditor.saveError
                    type: Kirigami.MessageType.Error
                    text: folderEditor.saveError
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 20

                    FolderIcon {
                        Layout.preferredWidth: 40
                        Layout.alignment: Qt.AlignBottom
                        Layout.preferredHeight: 40
                        iconName: folderEditor.folderIcon
                        colorValue: folderEditor.folderColor
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Controls.Label {
                            text: qsTr("Nombre")
                        }
                        Controls.TextField {
                            id: folderNameField
                            Layout.fillWidth: true
                            enabled: !folderEditor.pending && !folderEditor.storeAdapter.controller.busy
                            Accessible.name: qsTr("Nombre de la carpeta")
                            placeholderText: qsTr("Nombre de la carpeta")
                            onTextEdited: folderEditor.saveError = ""
                        }
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    visible: !folderEditor.folderId
                    spacing: 8

                    Controls.Label {
                        text: qsTr("Dentro de:")
                    }
                    FolderPicker {
                        id: parentField
                        Layout.fillWidth: true
                        enabled: !folderEditor.pending && !folderEditor.storeAdapter.controller.busy
                        folderRows: folderEditor.storeAdapter.folderRows
                        selectedId: folderEditor.selectedParentId
                        emptyLabel: qsTr("Carpetas principales")
                        accessibleLabel: qsTr("Carpeta superior")
                        onFolderSelected: function(folderId) {
                            folderEditor.selectedParentId = folderId;
                            folderEditor.saveError = "";
                        }
                    }
                }

                FolderAppearancePicker {
                    id: folderAppearance
                    Layout.fillWidth: true
                    enabled: !folderEditor.pending && !folderEditor.storeAdapter.controller.busy
                    selectedIcon: folderEditor.folderIcon
                    selectedColor: folderEditor.folderColor
                    onIconSelected: function(iconName) {
                        folderEditor.folderIcon = iconName;
                        folderEditor.saveError = "";
                    }
                    onColorSelected: function(colorValue) {
                        folderEditor.folderColor = colorValue;
                        folderEditor.saveError = "";
                    }
                }
            }
        }
    }

    footer: Controls.Pane {
        padding: 16

        contentItem: RowLayout {
            spacing: 8

            Item {
                Layout.fillWidth: true
            }
            Controls.BusyIndicator {
                visible: folderEditor.pending
                running: visible
                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                Layout.preferredHeight: width
            }
            Controls.Button {
                text: qsTr("Cancelar")
                enabled: !folderEditor.pending && !folderEditor.storeAdapter.controller.busy
                onClicked: folderEditor.requestClose()
            }
            Controls.Button {
                text: folderEditor.pending ? qsTr("Guardando…") : qsTr("Guardar")
                icon.name: "document-save"
                highlighted: true
                enabled: folderEditor.canSave
                onClicked: folderEditor.submit()
            }
        }
    }

    Controls.Dialog {
        id: discardDialog
        anchors.centerIn: parent
        modal: true
        title: qsTr("Descartar cambios")
        standardButtons: Controls.Dialog.Discard | Controls.Dialog.Cancel
        onDiscarded: {
            discardDialog.close();
            folderEditor.clearAndClose();
        }
        Controls.Label {
            width: Math.min(folderEditor.width - Kirigami.Units.largeSpacing * 4, Kirigami.Units.gridUnit * 28)
            text: qsTr("Hay cambios sin guardar. Si continúas, se descartarán.")
            wrapMode: Text.Wrap
        }
    }
}
