pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: root

    property var options: []
    property var kindOptions: []
    property var folderRows: []
    property string selectedId: ""
    property string folderFilterId: ""
    property bool initialFocusReject: false
    property string listAccessibleName: qsTr("Elementos disponibles para vincular")
    property string emptyText: qsTr("No hay elementos disponibles. Los que ya tienen un vínculo no aparecen aquí.")
    property string noMatchesText: qsTr("No hay coincidencias. Prueba con otro nombre, tipo o carpeta.")
    property string confirmText: qsTr("Vincular")
    property string cancelText: qsTr("Cancelar")
    property Component introduction: null

    property string searchObjectName: "entryPickerSearch"
    property string folderPickerObjectName: "entryPickerFolderPicker"
    property string listObjectName: "entryPickerList"
    property string candidateIconObjectName: "entryPickerCandidateIcon"
    property string cancelButtonObjectName: "cancelEntryPickerButton"
    property string confirmButtonObjectName: "confirmEntryPickerButton"

    readonly property var filteredOptions: {
        const needle = relatedEntrySearch.text.trim().toLocaleLowerCase();
        return options.filter(option => option.id && !option.unavailable
            && (!folderFilterId || (option.folder || "") === (folderFilterId === "unfiled" ? "" : folderFilterId))
            && (!needle || [option.title || "", option.subtitle || ""].join(" ").toLocaleLowerCase().includes(needle)));
    }
    readonly property string pendingId: relatedEntryList.currentIndex >= 0
        && relatedEntryList.currentIndex < filteredOptions.length
        ? filteredOptions[relatedEntryList.currentIndex].id : ""

    signal selectionRequested(string entryId)
    signal modalVisibilityChanged(bool opened)

    function kindIcon(kind) {
        const option = kindOptions.find(candidate => candidate.id === kind);
        return option?.icon || "document-encrypted";
    }

    function resetSelection() {
        folderFilterId = "";
        relatedEntrySearch.text = "";
        relatedEntryList.currentIndex = -1;
        if (opened && initialFocusReject)
            cancelButton.forceActiveFocus();
    }

    function confirmSelection() {
        if (!pendingId)
            return;
        selectionRequested(pendingId);
        close();
    }

    objectName: "entryPickerDialog"
    parent: Controls.Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(640, Math.max(0, parent.width - Kirigami.Units.largeSpacing * 2))
    height: Math.min(520, Math.max(0, parent.height - Kirigami.Units.largeSpacing * 2))
    modal: true
    closePolicy: Controls.Popup.CloseOnEscape
    title: qsTr("Vincular elemento")

    onOpened: {
        relatedEntryList.currentIndex = filteredOptions.findIndex(option => option.id === selectedId);
        if (initialFocusReject)
            cancelButton.forceActiveFocus();
        else
            relatedEntrySearch.forceActiveFocus();
    }
    onVisibleChanged: {
        modalVisibilityChanged(visible);
        if (!visible)
            resetSelection();
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Loader {
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            visible: root.introduction !== null
            sourceComponent: root.introduction
        }

        GridLayout {
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            columns: width < 480 ? 1 : 2
            columnSpacing: 16
            rowSpacing: 16
            ColumnLayout {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                spacing: 6
                Controls.Label { text: qsTr("Buscar elemento") }
                Kirigami.SearchField {
                    id: relatedEntrySearch
                    objectName: root.searchObjectName
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    Accessible.name: qsTr("Buscar por nombre, tipo o carpeta")
                    onTextChanged: relatedEntryList.currentIndex = -1
                    Keys.onReturnPressed: function(event) {
                        root.confirmSelection();
                        event.accepted = true;
                    }
                    Keys.onEnterPressed: function(event) {
                        root.confirmSelection();
                        event.accepted = true;
                    }
                    Keys.onDownPressed: function(event) {
                        if (relatedEntryList.count > 0) {
                            relatedEntryList.currentIndex = Math.max(0, relatedEntryList.currentIndex);
                            relatedEntryList.forceActiveFocus();
                        }
                        event.accepted = true;
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                spacing: 6
                Controls.Label { text: qsTr("Carpeta") }
                FolderPicker {
                    objectName: root.folderPickerObjectName
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    folderRows: [{ id: "unfiled", name: qsTr("Sin carpeta"), depth: 0 }].concat(root.folderRows)
                    selectedId: root.folderFilterId
                    emptyLabel: qsTr("Todas las carpetas")
                    accessibleLabel: qsTr("Filtrar por carpeta")
                    onFolderSelected: function(folderId) { root.folderFilterId = folderId; }
                }
            }
        }

        Controls.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 0
            clip: true
            Controls.ScrollBar.horizontal.policy: Controls.ScrollBar.AlwaysOff
            ListView {
                id: relatedEntryList
                objectName: root.listObjectName
                model: root.filteredOptions
                currentIndex: -1
                activeFocusOnTab: true
                keyNavigationEnabled: true
                keyNavigationWraps: false
                Accessible.role: Accessible.List
                Accessible.name: root.listAccessibleName
                onModelChanged: currentIndex = -1
                Keys.onReturnPressed: root.confirmSelection()
                Keys.onEnterPressed: root.confirmSelection()
                delegate: Controls.ItemDelegate {
                    id: candidateDelegate
                    required property var modelData
                    required property int index
                    width: relatedEntryList.width
                    text: ""
                    highlighted: relatedEntryList.currentIndex === index
                    Accessible.name: modelData.title || ""
                    Accessible.description: modelData.subtitle || ""
                    onClicked: {
                        relatedEntryList.currentIndex = index;
                        relatedEntryList.forceActiveFocus();
                    }
                    onDoubleClicked: root.confirmSelection()
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        EntryIcon {
                            objectName: root.candidateIconObjectName
                            entryIcon: candidateDelegate.modelData.icon || null
                            imageSource: candidateDelegate.modelData.iconSource || ""
                            fallbackIcon: root.kindIcon(candidateDelegate.modelData.kind || "")
                            selected: candidateDelegate.highlighted || candidateDelegate.down
                            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                            Layout.preferredHeight: width
                        }
                        Kirigami.TitleSubtitle {
                            Layout.fillWidth: true
                            textFormat: Text.PlainText
                            title: candidateDelegate.modelData.title || ""
                            subtitle: candidateDelegate.modelData.subtitle || ""
                            selected: candidateDelegate.highlighted || candidateDelegate.down
                            font: candidateDelegate.font
                            subtitleFont: candidateDelegate.font
                        }
                    }
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            visible: relatedEntryList.count === 0
            text: relatedEntrySearch.text.trim() || root.folderFilterId ? root.noMatchesText : root.emptyText
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
    }

    footer: Controls.DialogButtonBox {
        Controls.Button {
            id: cancelButton
            objectName: root.cancelButtonObjectName
            text: root.cancelText
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
            onClicked: root.close()
            Keys.onReturnPressed: function(event) {
                root.close();
                event.accepted = true;
            }
            Keys.onEnterPressed: function(event) {
                root.close();
                event.accepted = true;
            }
        }
        Controls.Button {
            objectName: root.confirmButtonObjectName
            text: root.confirmText
            enabled: !!root.pendingId
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
            onClicked: root.confirmSelection()
            Keys.onReturnPressed: function(event) {
                root.confirmSelection();
                event.accepted = true;
            }
            Keys.onEnterPressed: function(event) {
                root.confirmSelection();
                event.accepted = true;
            }
        }
    }
}
