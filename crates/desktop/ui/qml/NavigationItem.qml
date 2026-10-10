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

    signal scopeSelected(string scopeId)

    leftPadding: Kirigami.Units.largeSpacing + depth * Kirigami.Units.iconSizes.small
    rightPadding: Kirigami.Units.largeSpacing
    highlighted: selectedScope === scopeId
    onClicked: root.scopeSelected(scopeId)

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
