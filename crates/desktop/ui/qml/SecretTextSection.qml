pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Frame {
    id: root

    required property string heading
    required property string value
    required property string accessibleLabel
    property real minimumContentHeight: Kirigami.Units.gridUnit * 3

    padding: Kirigami.Units.largeSpacing
    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        Controls.Label {
            Layout.fillWidth: true
            text: root.heading
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
        Controls.TextArea {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.max(implicitHeight, root.minimumContentHeight)
            leftPadding: 0
            rightPadding: 0
            background: Rectangle {
                color: "transparent"
                radius: Kirigami.Units.smallSpacing
                border.width: parent.activeFocus ? 1 : 0
                border.color: Kirigami.Theme.highlightColor
            }
            readOnly: true
            selectByMouse: true
            wrapMode: TextEdit.Wrap
            textFormat: TextEdit.PlainText
            text: root.value
            Accessible.name: root.accessibleLabel
        }
    }
}
