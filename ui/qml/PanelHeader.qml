pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.ToolBar {
    id: root

    // The shell supplies the shared toolbar height so split panels stay aligned.
    required property real headerHeight

    implicitHeight: headerHeight
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.Header
    leftPadding: Kirigami.Units.largeSpacing
    rightPadding: Kirigami.Units.largeSpacing
    topPadding: Kirigami.Units.smallSpacing
    bottomPadding: Kirigami.Units.smallSpacing
    background: Rectangle {
        color: Kirigami.Theme.backgroundColor
        Kirigami.Separator {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: root.position === Controls.ToolBar.Footer ? parent.top : undefined
            anchors.bottom: root.position === Controls.ToolBar.Header ? parent.bottom : undefined
        }
    }
}
