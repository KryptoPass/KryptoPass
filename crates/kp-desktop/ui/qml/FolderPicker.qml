pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Button {
    id: picker

    property var folderRows: []
    property string selectedId: ""
    property string emptyLabel: qsTr("Sin carpeta")
    property string accessibleLabel: qsTr("Carpeta")

    readonly property bool menuOpen: folderMenu.opened
    readonly property var selectedFolder: (folderRows || []).find(folder => folder.id === selectedId) || null
    readonly property string selectedName: selectedId === ""
            ? emptyLabel
            : selectedFolder ? selectedFolder.name : qsTr("Carpeta no disponible")
    readonly property var options: [{
        id: "",
        name: emptyLabel,
        depth: 0
    }].concat(folderRows || [])

    signal folderSelected(string folderId)

    // KDE's native button background can paint `text`; keep the label in contentItem only.
    text: ""
    leftPadding: Kirigami.Units.mediumSpacing
    rightPadding: Kirigami.Units.mediumSpacing
    topPadding: Kirigami.Units.smallSpacing
    bottomPadding: Kirigami.Units.smallSpacing
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("%1: %2").arg(accessibleLabel).arg(selectedName)
    Accessible.description: qsTr("Abre la lista de carpetas.")
    onClicked: folderMenu.open()

    contentItem: RowLayout {
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: picker.selectedName
            textFormat: Text.PlainText
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }

        Kirigami.Icon {
            source: "arrow-down"
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: width
            color: Kirigami.Theme.textColor
        }
    }

    Controls.Menu {
        id: folderMenu
        y: picker.height
        width: picker.width

        Instantiator {
            model: picker.options

            delegate: Controls.MenuItem {
                id: folderMenuItem
                required property var modelData
                text: ""
                checkable: true
                checked: modelData.id === picker.selectedId
                Accessible.name: modelData.name
                Accessible.description: checked ? qsTr("Seleccionada") : qsTr("Seleccionar carpeta")

                contentItem: Controls.Label {
                    leftPadding: folderMenuItem.mirrored
                            ? 0
                            : folderMenuItem.indicator.width + Kirigami.Units.smallSpacing
                    rightPadding: folderMenuItem.mirrored
                            ? folderMenuItem.indicator.width + Kirigami.Units.smallSpacing
                            : 0
                    text: folderMenuItem.modelData.id === ""
                            ? folderMenuItem.modelData.name
                            : "\u00a0".repeat(Math.min(12, folderMenuItem.modelData.depth || 0) * 2) + folderMenuItem.modelData.name
                    textFormat: Text.PlainText
                    font: folderMenuItem.font
                    color: !folderMenuItem.enabled
                            ? Kirigami.Theme.disabledTextColor
                            : folderMenuItem.pressed ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                }

                onTriggered: {
                    picker.folderSelected(modelData.id);
                    folderMenu.close();
                }
            }

            onObjectAdded: function(index, object) {
                folderMenu.insertItem(index, object);
            }
            onObjectRemoved: function(index, object) {
                folderMenu.removeItem(object);
            }
        }
    }
}
