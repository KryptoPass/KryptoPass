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

    signal scopeSelected(string scopeId)

    leftPadding: Kirigami.Units.largeSpacing + depth * Kirigami.Units.iconSizes.small
    rightPadding: Kirigami.Units.largeSpacing
    highlighted: selectedScope === scopeId
    onClicked: root.scopeSelected(scopeId)
    contentItem: RowLayout {
        Kirigami.IconTitleSubtitle {
            Layout.fillWidth: true
            title: root.text
            icon: icon.fromControlsIcon(root.icon)
            selected: root.highlighted || root.down
            font: root.font
        }
        Controls.Label {
            text: root.itemCount
            color: root.highlighted || root.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
        }
    }
}
