import QtQuick

Rectangle {
    id: root

    required property string eyebrow
    required property string title
    required property string description
    required property string iconText
    property color accentColor: "#6c5ce7"

    implicitWidth: 250
    implicitHeight: 178
    radius: 20
    color: cardHover.hovered ? "#ffffff" : "#fbfbfd"
    border.width: 1
    border.color: cardHover.hovered ? Qt.alpha(accentColor, 0.35) : "#e7e8ee"
    scale: cardHover.hovered ? 1.015 : 1.0

    Behavior on color { ColorAnimation { duration: 150 } }
    Behavior on border.color { ColorAnimation { duration: 150 } }
    Behavior on scale { NumberAnimation { duration: 150; easing.type: Easing.OutCubic } }

    Column {
        anchors.fill: parent
        anchors.margins: 20
        spacing: 10

        Row {
            width: parent.width
            spacing: 10

            Rectangle {
                width: 36
                height: 36
                radius: 11
                color: Qt.alpha(root.accentColor, 0.12)

                Text {
                    anchors.centerIn: parent
                    text: root.iconText
                    color: root.accentColor
                    font.pixelSize: 17
                    font.weight: Font.Bold
                }
            }

            Text {
                width: parent.width - 46
                height: 36
                text: root.eyebrow.toUpperCase()
                color: root.accentColor
                font.pixelSize: 10
                font.letterSpacing: 1.3
                font.weight: Font.Bold
                verticalAlignment: Text.AlignVCenter
            }
        }

        Text {
            width: parent.width
            text: root.title
            color: "#232735"
            font.pixelSize: 17
            font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
        }

        Text {
            width: parent.width
            text: root.description
            color: "#747b8b"
            font.pixelSize: 13
            lineHeight: 1.25
            wrapMode: Text.WordWrap
        }
    }

    HoverHandler { id: cardHover }
}

