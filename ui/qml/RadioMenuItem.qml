pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// Menu entries that switch a mutually exclusive view carry a radio mark, not a
// check mark that would suggest independent toggles.
Controls.MenuItem {
    id: root

    indicator: Item {
        x: root.mirrored ? root.width - width - root.rightPadding : root.leftPadding
        y: root.topPadding + (root.availableHeight - height) / 2
        implicitWidth: Kirigami.Units.iconSizes.small
        implicitHeight: Kirigami.Units.iconSizes.small

        Rectangle {
            anchors.centerIn: parent
            width: Kirigami.Units.iconSizes.small
            height: width
            radius: width / 2
            color: "transparent"
            border.width: 1
            border.color: root.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
        }
        Rectangle {
            anchors.centerIn: parent
            visible: root.checked
            width: Kirigami.Units.iconSizes.small / 2
            height: width
            radius: width / 2
            color: root.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.highlightColor
        }
    }
}
