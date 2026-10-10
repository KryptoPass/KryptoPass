pragma ComponentBehavior: Bound
import QtQuick
import org.kde.kirigami as Kirigami

Kirigami.Separator {
    id: root

    implicitWidth: 1
    implicitHeight: 1
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    weight: Kirigami.Separator.Weight.Light
    containmentMask: Item {
        readonly property real margin: Kirigami.Units.largeSpacing
        x: -margin
        width: root.width + margin * 2
        height: root.height
    }
    HoverHandler {
        cursorShape: Qt.SplitHCursor
    }
}
