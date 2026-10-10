pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Pane {
    id: root

    required property bool keysMode
    required property var activeStore
    required property SecretStore storeAdapter
    required property real headerHeight
    required property Controls.Action newFolderAction

    signal sectionSelectionRequested(bool showKeys)
    signal scopeSelectionRequested(string scopeId)
    signal renameFolderRequested()
    signal deleteFolderRequested()

    Controls.SplitView.preferredWidth: Kirigami.Units.gridUnit * 14
    Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 11
    padding: 0
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.View

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        PanelHeader {
            Layout.fillWidth: true
            headerHeight: root.headerHeight
            contentItem: RowLayout {
                Controls.ToolButton {
                    id: sectionPicker
                    objectName: "sectionPicker"
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    leftPadding: Kirigami.Units.largeSpacing
                    rightPadding: Kirigami.Units.largeSpacing
                    topPadding: Kirigami.Units.smallSpacing
                    bottomPadding: Kirigami.Units.smallSpacing
                    readonly property string sectionTitle: root.keysMode ? qsTranslate("Main", "Claves") : qsTranslate("Main", "Carpetas")
                    Accessible.name: qsTranslate("Main", "Cambiar sección: %1").arg(sectionTitle)
                    onClicked: sectionMenu.popup(sectionPicker, 0, sectionPicker.height)
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Kirigami.Heading {
                            Layout.fillWidth: true
                            level: 3
                            text: sectionPicker.sectionTitle
                            elide: Text.ElideRight
                        }
                        Kirigami.Icon {
                            source: "arrow-down"
                            Layout.preferredWidth: Kirigami.Units.iconSizes.small
                            Layout.preferredHeight: width
                        }
                    }
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.text: qsTranslate("Main", "Cambiar entre carpetas y claves")
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    Controls.Menu {
                        id: sectionMenu
                        Controls.MenuItem {
                            text: qsTranslate("Main", "Carpetas")
                            icon.name: "folder"
                            onTriggered: root.sectionSelectionRequested(false)
                        }
                        Controls.MenuItem {
                            text: qsTranslate("Main", "Claves")
                            icon.name: "view-certificate"
                            onTriggered: root.sectionSelectionRequested(true)
                        }
                    }
                }
                Controls.ToolButton {
                    action: root.newFolderAction
                    visible: !root.keysMode
                    display: Controls.AbstractButton.IconOnly
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.text: text
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }
        }
        Repeater {
            model: root.keysMode ? [
                {
                    id: "all",
                    name: qsTranslate("Main", "Todas las claves"),
                    icon: "view-certificate"
                },
                {
                    id: "own",
                    name: qsTranslate("Main", "Claves propias"),
                    icon: "user-identity"
                },
                {
                    id: "contacts",
                    name: qsTranslate("Main", "Destinatarios"),
                    icon: "view-pim-contacts"
                }
            ] : [
                {
                    id: "all",
                    name: qsTranslate("Main", "Todos los elementos"),
                    icon: "view-list-details"
                },
                {
                    id: "favorites",
                    name: qsTranslate("Main", "Favoritos"),
                    icon: "rating"
                },
                {
                    id: "trash",
                    name: qsTranslate("Main", "Papelera"),
                    icon: "user-trash"
                }
            ]
            delegate: NavigationItem {
                required property var modelData
                selectedScope: root.activeStore.scope
                itemCount: root.activeStore.count(modelData.id)
                Layout.fillWidth: true
                scopeId: modelData.id
                text: modelData.name
                icon.name: modelData.icon
                onScopeSelected: function(scopeId) {
                    root.scopeSelectionRequested(scopeId);
                }
            }
        }
        Kirigami.ListSectionHeader {
            Layout.fillWidth: true
            visible: !root.keysMode
            text: qsTranslate("Main", "Mis carpetas")
        }
        Controls.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            ListView {
                id: folderList
                model: root.keysMode ? [] : root.storeAdapter.folderRows
                activeFocusOnTab: true
                keyNavigationWraps: true
                Accessible.role: Accessible.List
                delegate: NavigationItem {
                    required property var modelData
                    selectedScope: root.activeStore.scope
                    itemCount: root.activeStore.count(modelData.id)
                    width: folderList.width
                    scopeId: modelData.id
                    depth: modelData.depth
                    folderColor: modelData.color || "transparent"
                    text: modelData.name
                    icon.name: modelData.icon || "folder"
                    onScopeSelected: function(scopeId) {
                        root.scopeSelectionRequested(scopeId);
                    }
                    Keys.onReturnPressed: clicked()
                    Keys.onEnterPressed: clicked()
                }
            }
        }
        PanelFooter {
            Layout.fillWidth: true
            headerHeight: root.headerHeight
            contentItem: RowLayout {
                Controls.ToolButton {
                    visible: !root.keysMode
                    text: qsTranslate("Main", "Editar carpeta")
                    icon.name: "edit-rename"
                    display: Controls.AbstractButton.IconOnly
                    enabled: !root.storeAdapter.controller.busy && !["all", "favorites", "trash"].includes(root.storeAdapter.scope)
                    onClicked: root.renameFolderRequested()
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.text: text
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                Controls.ToolButton {
                    visible: !root.keysMode
                    text: qsTranslate("Main", "Eliminar carpeta")
                    icon.name: "edit-delete"
                    display: Controls.AbstractButton.IconOnly
                    enabled: !root.storeAdapter.controller.busy && !["all", "favorites", "trash"].includes(root.storeAdapter.scope)
                    onClicked: root.deleteFolderRequested()
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.text: text
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                Item {
                    Layout.fillWidth: true
                }
            }
        }
    }
}
