pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.ItemDelegate {
    id: root

    required property string selectedScope
    required property int itemCount
    required property string scopeId
    property int depth: 0
    property string folderColor: ""
    property bool acceptsEntryDrop: false
    property var dropStore: null
    property bool folderDraggingEnabled: false
    property bool acceptsFolderDrop: false
    property string parentFolderId: ""
    signal folderDropped(string folderId, string parentId, string beforeId)
    signal entryDropped(string entryId, string folderId)

    function acceptsSource(source) {
        return !!source && source.storeAdapter === dropStore;
    }

    function sourceFolderId(source) {
        return source.folderId;
    }

    function folderPlacement(y) {
        if (y < height * 0.25)
            return {
                parent: parentFolderId,
                before: scopeId,
                edge: "before"
            };
        if (y > height * 0.75) {
            const siblings = dropStore.folderRows.filter(folder => folder.parent === parentFolderId);
            const index = siblings.findIndex(folder => folder.id === scopeId);
            return {
                parent: parentFolderId,
                before: index + 1 < siblings.length ? siblings[index + 1].id : "",
                edge: "after"
            };
        }
        return {
            parent: scopeId,
            before: "",
            edge: "inside"
        };
    }
    function canDropFolder(source, y) {
        if (!acceptsFolderDrop || !acceptsSource(source) || !source.folderId || source.folderId === scopeId)
            return false;
        const placement = folderPlacement(y);
        return source.folderId !== placement.before && dropStore.canMoveFolder(source.folderId, placement.parent);
    }

    DragHandler {
        id: folderDrag
        target: null
        acceptedButtons: Qt.LeftButton
        enabled: root.folderDraggingEnabled
        function positionPreview() {
            const point = root.mapToItem(folderPreview.parent, centroid.position.x, centroid.position.y);
            folderPreview.x = point.x;
            folderPreview.y = point.y;
        }
        onActiveTranslationChanged: {
            if (active)
                positionPreview();
        }
        onActiveChanged: {
            if (active) {
                positionPreview();
                folderPreview.Drag.active = true;
            } else if (folderPreview.Drag.active) {
                folderPreview.Drag.drop();
                folderPreview.Drag.active = false;
            }
        }
    }
    Shortcut {
        sequence: "Esc"
        enabled: folderPreview.Drag.active
        onActivated: folderPreview.Drag.cancel()
    }
    Rectangle {
        id: folderPreview
        parent: Controls.Overlay.overlay
        readonly property string folderId: root.scopeId
        readonly property var storeAdapter: root.dropStore
        visible: Drag.active
        z: 1000
        width: folderLabel.width + Kirigami.Units.largeSpacing * 2
        height: folderLabel.implicitHeight + Kirigami.Units.smallSpacing * 4
        radius: Kirigami.Units.cornerRadius
        color: Kirigami.Theme.backgroundColor
        border.color: Kirigami.Theme.highlightColor
        Drag.source: folderPreview
        Drag.keys: ["org.kryptopass.folder"]
        Drag.supportedActions: Qt.MoveAction
        Drag.proposedAction: Qt.MoveAction
        Controls.Label {
            id: folderLabel
            anchors.centerIn: parent
            width: Math.min(implicitWidth, Kirigami.Units.gridUnit * 14)
            text: root.text
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }
    }
    DropArea {
        id: folderDrop
        anchors.fill: parent
        enabled: root.acceptsFolderDrop
        keys: ["org.kryptopass.folder"]
        property string edge: "inside"
        property bool validTarget: false
        onEntered: function (drag) {
            validTarget = root.canDropFolder(drag.source, drag.y);
            edge = root.folderPlacement(drag.y).edge;
            drag.accepted = root.acceptsSource(drag.source);
        }
        onPositionChanged: function (drag) {
            validTarget = root.canDropFolder(drag.source, drag.y);
            edge = root.folderPlacement(drag.y).edge;
        }
        onExited: validTarget = false
        onDropped: function (drop) {
            if (!root.canDropFolder(drop.source, drop.y))
                return;
            const placement = root.folderPlacement(drop.y);
            const sourceId = root.sourceFolderId(drop.source);
            validTarget = false;
            drop.accept(Qt.MoveAction);
            root.folderDropped(sourceId, placement.parent, placement.before);
        }
    }
    DropArea {
        id: entryDrop
        anchors.fill: parent
        enabled: root.acceptsEntryDrop
        keys: ["org.kryptopass.entry"]
        onEntered: function (drag) {
            drag.accepted = root.acceptsSource(drag.source);
        }
        onDropped: function (drop) {
            if (!root.acceptsEntryDrop || !root.acceptsSource(drop.source))
                return;
            const entry = root.dropStore.entries.find(candidate => candidate.id === drop.source.entryId);
            if (!entry || entry.trashed || entry.folder === root.scopeId)
                return;
            drop.accept(Qt.MoveAction);
            root.entryDropped(entry.id, root.scopeId);
        }
    }
    Rectangle {
        anchors.fill: parent
        visible: entryDrop.containsDrag || (folderDrop.containsDrag && folderDrop.validTarget && folderDrop.edge === "inside")
        color: "transparent"
        border.color: Kirigami.Theme.highlightColor
        border.width: 2
    }
    Rectangle {
        width: parent.width
        height: 3
        y: folderDrop.edge === "before" ? 0 : parent.height - height
        visible: folderDrop.containsDrag && folderDrop.validTarget && folderDrop.edge !== "inside"
        color: Kirigami.Theme.highlightColor
    }

    signal scopeSelected(string scopeId)

    leftPadding: Kirigami.Units.largeSpacing + depth * Kirigami.Units.iconSizes.small
    rightPadding: Kirigami.Units.largeSpacing
    highlighted: selectedScope === scopeId
    onClicked: root.scopeSelected(scopeId)
    Controls.ToolTip.visible: (folderDrop.containsDrag && folderDrop.validTarget) || (folderDraggingEnabled && hovered && !folderDrag.active)
    Controls.ToolTip.delay: folderDrop.containsDrag ? 0 : Kirigami.Units.toolTipDelay
    Controls.ToolTip.text: folderDrop.containsDrag ? folderDrop.edge === "inside" ? qsTr("Mover dentro de «%1»").arg(text) : folderDrop.edge === "before" ? qsTr("Colocar antes de «%1»").arg(text) : qsTr("Colocar después de «%1» y sus subcarpetas").arg(text) : qsTr("Arrastra para mover esta carpeta con sus hijos. Suelta en el centro para anidar, o en un borde para ordenar.")

    contentItem: RowLayout {
        FolderIcon {
            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
            Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
            Layout.alignment: Qt.AlignVCenter
            iconName: root.icon.name
            colorValue: root.folderColor || "transparent"
            highlighted: root.highlighted || root.down
        }
        Controls.Label {
            Layout.fillWidth: true
            text: root.text
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: root.highlighted || root.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
            font: root.font
        }
        Controls.Label {
            text: root.itemCount
            textFormat: Text.PlainText
            color: root.highlighted || root.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
        }
    }
}
