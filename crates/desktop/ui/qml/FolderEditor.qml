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
    property string sensitiveClearAction: ""
    readonly property bool dirty: baselineJson !== JSON.stringify({
        id: folderId,
        name: folderNameField.text,
        parent: selectedParentId,
        icon: folderIcon,
        color: folderColor
    })
    readonly property bool canSave: !pending && !storeAdapter.controller.busy && folderNameField.text.trim().length > 0 && dirty

    signal saveRequested(string folderId, string name, string parentId, string icon, string color)
    signal vaultLockApproved()
    signal vaultLockCancelled()
    signal vaultSwitchApproved()
    signal vaultSwitchCancelled()

    anchors.centerIn: parent
    width: Math.min(340, parent.width - Kirigami.Units.gridUnit * 4)
    height: Math.min(implicitHeight, parent.height - Kirigami.Units.gridUnit * 4)
    padding: 0
    spacing: 0
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
        sensitiveClearAction = "";
        if (dirty)
            discardDialog.open();
        else
            close();
    }

    function requestVaultLock() {
        requestSensitiveClear("lock");
    }

    function requestVaultSwitch() {
        requestSensitiveClear("switch");
    }

    function requestSensitiveClear(action) {
        if (pending || storeAdapter.controller.busy)
            return;
        sensitiveClearAction = action;
        if (dirty) {
            discardDialog.open();
        } else {
            clearAndClose();
            emitSensitiveClearApproved(action);
        }
    }

    function emitSensitiveClearApproved(action) {
        if (action === "lock")
            vaultLockApproved();
        else if (action === "switch")
            vaultSwitchApproved();
    }

    function emitSensitiveClearCancelled(action) {
        if (action === "lock")
            vaultLockCancelled();
        else if (action === "switch")
            vaultSwitchCancelled();
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

    header: Controls.Pane {
        leftPadding: 20
        rightPadding: 20
        topPadding: 12
        bottomPadding: 0
        background: null

        contentItem: RowLayout {
            spacing: 8

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 2
                text: folderEditor.title
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }
            Controls.ToolButton {
                icon.name: "window-close-symbolic"
                text: qsTr("Cerrar")
                display: Controls.AbstractButton.IconOnly
                enabled: !folderEditor.pending && !folderEditor.storeAdapter.controller.busy
                onClicked: folderEditor.requestClose()
            }
        }
    }

    contentItem: ColumnLayout {
        spacing: 0

        Controls.Pane {
            Layout.fillWidth: true
            leftPadding: 20
            rightPadding: 20
            topPadding: 8
            bottomPadding: 20
            background: null

            contentItem: ColumnLayout {
                spacing: 20

                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !!folderEditor.saveError
                    type: Kirigami.MessageType.Error
                    text: folderEditor.saveError
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    Controls.Label {
                        text: qsTr("Nombre")
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 12

                        Controls.TextField {
                            id: folderNameField
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignVCenter
                            enabled: !folderEditor.pending && !folderEditor.storeAdapter.controller.busy
                            Accessible.name: qsTr("Nombre de la carpeta")
                            placeholderText: qsTr("Nombre de la carpeta")
                            onTextEdited: folderEditor.saveError = ""
                        }
                        FolderIcon {
                            Layout.preferredWidth: 40
                            Layout.preferredHeight: 40
                            Layout.alignment: Qt.AlignVCenter
                            iconName: folderEditor.folderIcon
                            colorValue: folderEditor.folderColor
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
            }
        }

        Controls.ScrollView {
            id: appearanceScroll
            objectName: "folderAppearanceScroll"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 0
            Layout.preferredHeight: Math.min(280, appearancePane.implicitHeight)
            contentWidth: availableWidth
            contentHeight: appearancePane.implicitHeight
            clip: true
            Controls.ScrollBar.horizontal.policy: Controls.ScrollBar.AlwaysOff

            Controls.Pane {
                id: appearancePane
                width: appearanceScroll.availableWidth
                leftPadding: 20
                rightPadding: 20
                topPadding: 0
                bottomPadding: 12
                background: null

                contentItem: FolderAppearancePicker {
                    id: folderAppearance
                    enabled: !folderEditor.pending && !folderEditor.storeAdapter.controller.busy
                    selectedIcon: folderEditor.folderIcon
                    selectedColor: folderEditor.folderColor
                    onRevealRequested: function(top, height) {
                        const flickable = appearanceScroll.contentItem as Flickable;
                        if (!flickable)
                            return;
                        if (top < flickable.contentY)
                            flickable.contentY = top;
                        else if (top + height > flickable.contentY + flickable.height)
                            flickable.contentY = Math.min(top + height - flickable.height,
                                                         Math.max(0, flickable.contentHeight - flickable.height));
                    }
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
        leftPadding: 20
        rightPadding: 20
        topPadding: 8
        bottomPadding: 8
        background: null

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
            const action = folderEditor.sensitiveClearAction;
            folderEditor.sensitiveClearAction = "";
            discardDialog.close();
            folderEditor.clearAndClose();
            folderEditor.emitSensitiveClearApproved(action);
        }
        onRejected: {
            const action = folderEditor.sensitiveClearAction;
            folderEditor.sensitiveClearAction = "";
            folderEditor.emitSensitiveClearCancelled(action);
        }
        Controls.Label {
            width: Math.min(folderEditor.width - Kirigami.Units.largeSpacing * 4, Kirigami.Units.gridUnit * 28)
            text: qsTr("Hay cambios sin guardar. Si continúas, se descartarán.")
            wrapMode: Text.Wrap
        }
    }
}
