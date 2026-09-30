import QtQuick

Rectangle {
    id: root

    property string text: ""
    property bool healthy: true

    implicitWidth: statusRow.implicitWidth + 24
    implicitHeight: 32
    radius: 16
    color: healthy ? "#e9f8f1" : "#fff3e5"
    border.width: 1
    border.color: healthy ? "#c7ead9" : "#f1d6b4"

    Row {
        id: statusRow

        anchors.centerIn: parent
        spacing: 8

        Rectangle {
            width: 8
            height: 8
            radius: 4
            color: root.healthy ? "#24a66a" : "#df8c2f"

            SequentialAnimation on opacity {
                running: root.healthy
                loops: Animation.Infinite
                NumberAnimation { to: 0.35; duration: 900 }
                NumberAnimation { to: 1; duration: 900 }
            }
        }

        Text {
            text: root.text
            color: root.healthy ? "#187449" : "#9a5b19"
            font.pixelSize: 12
            font.weight: Font.DemiBold
        }
    }
}

