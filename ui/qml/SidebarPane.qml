pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Pane {
    id: root

    required property bool keysMode
    required property var activeStore
    required property SecretStore storeAdapter
    required property real headerHeight
    required property Controls.Action newFolderAction

    property bool showBackButton: false
    signal backRequested

    property bool entryDraggingEnabled: false
    signal entryMoveRequested(string entryId, string folderId)
    signal folderMoveRequested(string folderId, string parentId, string beforeId)

    signal sectionSelectionRequested(bool showKeys)
    signal scopeSelectionRequested(string scopeId)
    signal renameFolderRequested
    signal deleteFolderRequested
    signal folderStepRequested(string folderId, int delta)
    signal folderMoveDialogRequested(string folderId)

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
                    visible: root.showBackButton
                    text: KI18n.i18nc("@action:button", "Back to List")
                    icon.name: "go-previous"
                    display: Controls.AbstractButton.IconOnly
                    onClicked: root.backRequested()
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text
                }
                Controls.ToolButton {
                    id: sectionPicker
                    objectName: "sectionPicker"
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    leftPadding: Kirigami.Units.largeSpacing
                    rightPadding: Kirigami.Units.largeSpacing
                    topPadding: Kirigami.Units.smallSpacing
                    bottomPadding: Kirigami.Units.smallSpacing
                    readonly property string sectionTitle: root.keysMode ? KI18n.i18nc("@title sidebar section", "Keys") : KI18n.i18nc("@title sidebar section", "Folders")
                    Accessible.name: KI18n.i18n("Switch section: %1", sectionTitle)
                    onClicked: {
                        sectionToolTipTimer.stop();
                        sectionToolTip.close();
                        sectionMenu.popup(sectionPicker, 0, sectionPicker.height);
                    }
                    onHoveredChanged: {
                        if (hovered) {
                            sectionToolTipTimer.start();
                        } else {
                            sectionToolTipTimer.stop();
                            sectionToolTip.close();
                        }
                    }
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
                    Controls.Menu {
                        id: sectionMenu
                        Controls.MenuItem {
                            text: KI18n.i18nc("@title sidebar section", "Folders")
                            icon.name: "folder"
                            onTriggered: root.sectionSelectionRequested(false)
                        }
                        Controls.MenuItem {
                            text: KI18n.i18nc("@title sidebar section", "Keys")
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
                    id: "own",
                    name: KI18n.i18nc("@title key scope", "My Identities"),
                    icon: "user-identity"
                },
                {
                    id: "contacts",
                    name: KI18n.i18nc("@title key scope", "People"),
                    icon: "view-pim-contacts"
                },
                {
                    id: "all",
                    name: KI18n.i18nc("@title key scope", "All Keys"),
                    icon: "view-certificate"
                }
            ] : [
                {
                    id: "all",
                    name: KI18n.i18nc("@title vault scope", "All Items"),
                    icon: "view-list-details"
                },
                {
                    id: "favorites",
                    name: KI18n.i18nc("@title vault scope", "Favorites"),
                    icon: "rating"
                },
                {
                    id: "trash",
                    name: KI18n.i18nc("@title vault scope", "Trash"),
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
                onScopeSelected: function (scopeId) {
                    root.scopeSelectionRequested(scopeId);
                }
            }
        }
        Item {
            Layout.fillWidth: true
            implicitHeight: folderSectionHeader.implicitHeight
            visible: !root.keysMode
            Kirigami.ListSectionHeader {
                id: folderSectionHeader
                anchors.fill: parent
                text: rootFolderDrop.containsDrag ? KI18n.i18nc("@label drop target", "Move to top level") : KI18n.i18nc("@title", "My Folders")
            }
            DropArea {
                id: rootFolderDrop
                anchors.fill: parent
                enabled: root.entryDraggingEnabled && !root.keysMode
                keys: ["org.kryptopass.folder"]
                function acceptsSource(source) {
                    return !!source && source.storeAdapter === root.storeAdapter && root.storeAdapter.canMoveFolder(source.folderId, "");
                }
                function sourceFolderId(source) {
                    return source.folderId;
                }
                onEntered: function (drag) {
                    drag.accepted = acceptsSource(drag.source);
                }
                onDropped: function (drop) {
                    if (!enabled || !acceptsSource(drop.source))
                        return;
                    const sourceId = sourceFolderId(drop.source);
                    drop.accept(Qt.MoveAction);
                    root.folderMoveRequested(sourceId, "", "");
                }
                Rectangle {
                    anchors.fill: parent
                    visible: rootFolderDrop.containsDrag
                    color: "transparent"
                    border.width: 2
                    border.color: Kirigami.Theme.highlightColor
                }
            }
        }
        Controls.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            AppListView {
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
                    acceptsEntryDrop: root.entryDraggingEnabled && !root.keysMode
                    dropStore: root.storeAdapter
                    onEntryDropped: function (entryId, folderId) {
                        root.entryMoveRequested(entryId, folderId);
                    }
                    folderDraggingEnabled: root.entryDraggingEnabled && !root.keysMode
                    scrollPosition: folderList.contentY
                    acceptsFolderDrop: folderDraggingEnabled
                    parentFolderId: modelData.parent
                    onFolderDropped: function (folderId, parentId, beforeId) {
                        root.folderMoveRequested(folderId, parentId, beforeId);
                    }
                    depth: modelData.depth
                    folderColor: modelData.color || "transparent"
                    text: modelData.name
                    icon.name: modelData.icon || "folder"
                    onScopeSelected: function (scopeId) {
                        root.scopeSelectionRequested(scopeId);
                    }
                    onContextMenuRequested: function (item, x, y) {
                        if (!root.entryDraggingEnabled)
                            return;
                        folderMenu.folderId = modelData.id;
                        folderMenu.popup(item, x, y);
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
                    text: KI18n.i18nc("@action:inmenu", "Edit Folder")
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
                    text: KI18n.i18nc("@action:inmenu", "Delete Folder")
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
    Controls.Menu {
        id: folderMenu
        property string folderId: ""
        Controls.MenuItem {
            text: KI18n.i18nc("@action:inmenu", "Move Folder Up")
            icon.name: "go-up"
            onTriggered: root.folderStepRequested(folderMenu.folderId, -1)
        }
        Controls.MenuItem {
            text: KI18n.i18nc("@action:inmenu", "Move Folder Down")
            icon.name: "go-down"
            onTriggered: root.folderStepRequested(folderMenu.folderId, 1)
        }
        Controls.MenuSeparator {}
        Controls.MenuItem {
            text: KI18n.i18nc("@action:inmenu", "Move Folder…")
            icon.name: "folder-move"
            onTriggered: root.folderMoveDialogRequested(folderMenu.folderId)
        }
    }
    Controls.ToolTip {
        id: sectionToolTip
        parent: sectionPicker
        x: Math.round((sectionPicker.width - width) / 2)
        y: sectionPicker.height + Kirigami.Units.smallSpacing
        text: KI18n.i18nc("@info:tooltip", "Switch between folders and keys")
        closePolicy: Controls.Popup.NoAutoClose
        background: Rectangle {
            color: Kirigami.Theme.backgroundColor
            border.width: 1
            border.color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.25)
            radius: Kirigami.Units.smallSpacing
        }
    }
    Timer {
        id: sectionToolTipTimer
        interval: 600
        onTriggered: sectionToolTip.open()
    }
}
