pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

RowLayout {
    id: root

    required property Controls.Action editAction
    required property Controls.Action favoriteAction
    required property Controls.Action moveAction
    required property Controls.Action trashAction
    required property Controls.Action restoreAction
    required property Controls.Action deleteAction
    required property bool trashed
    required property bool hasRelatedEntry
    required property bool locked
    required property bool busy
    required property bool editorBusy

    signal createRelatedEntryRequested

    spacing: Kirigami.Units.smallSpacing
    Accessible.role: Accessible.ToolBar
    Accessible.name: qsTr("Acciones del elemento")

    Controls.ToolButton {
        action: root.editAction
        visible: !root.trashed
        display: Controls.AbstractButton.TextBesideIcon
    }
    Controls.ToolButton {
        action: root.favoriteAction
        visible: !root.trashed
        display: Controls.AbstractButton.TextBesideIcon
        Controls.ToolTip.visible: hovered || visualFocus
        Controls.ToolTip.text: text
    }
    Controls.ToolButton {
        action: root.moveAction
        visible: !root.trashed
        display: Controls.AbstractButton.TextBesideIcon
        Controls.ToolTip.visible: hovered || visualFocus
        Controls.ToolTip.text: text
    }
    Controls.ToolButton {
        action: root.restoreAction
        visible: root.trashed
        display: Controls.AbstractButton.TextBesideIcon
        Accessible.name: text
        Controls.ToolTip.visible: hovered || visualFocus
        Controls.ToolTip.text: text
    }
    Controls.ToolButton {
        objectName: "createRelatedEntryButton"
        text: qsTr("Crear elemento vinculado")
        icon.name: "link"
        display: Controls.AbstractButton.TextBesideIcon
        visible: !root.trashed && !root.hasRelatedEntry
        enabled: !root.locked && !root.busy && !root.editorBusy
        Accessible.name: qsTr("Crear un elemento vinculado a este")
        onClicked: root.createRelatedEntryRequested()
        Controls.ToolTip.visible: hovered || visualFocus
        Controls.ToolTip.text: text
    }
    Controls.ToolSeparator {}
    Controls.ToolButton {
        action: root.trashAction
        visible: !root.trashed
        display: Controls.AbstractButton.IconOnly
        Accessible.name: text
        Controls.ToolTip.visible: hovered || visualFocus
        Controls.ToolTip.text: text
    }
    Controls.ToolButton {
        action: root.deleteAction
        visible: root.trashed
        display: Controls.AbstractButton.IconOnly
        Accessible.name: text
        Controls.ToolTip.visible: hovered || visualFocus
        Controls.ToolTip.text: text
    }
}
