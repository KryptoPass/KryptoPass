import QtQuick
import QtQuick.Controls

Item {
    id: root

    required property string label
    required property string iconText
    property bool selected: false
    property bool compact: false
    signal activated

    implicitWidth: 208
    implicitHeight: 46

    Rectangle {
        id: surface

        anchors.fill: parent
        radius: 13
        color: root.selected ? "#edeaff" : (tapHandler.hovered ? "#f3f4f8" : "transparent")

        Behavior on color {
            ColorAnimation { duration: 140 }
        }
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: root.compact ? 0 : 14
        anchors.rightMargin: root.compact ? 0 : 14
        spacing: 12

        Text {
            width: root.compact ? parent.width : 24
            height: parent.height
            text: root.iconText
            color: root.selected ? "#6c5ce7" : "#747b8b"
            font.pixelSize: 17
            font.weight: Font.DemiBold
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }

        Text {
            visible: !root.compact
            width: parent.width - 50
            height: parent.height
            text: root.label
            color: root.selected ? "#30276d" : "#555d6d"
            font.pixelSize: 14
            font.weight: root.selected ? Font.DemiBold : Font.Medium
            verticalAlignment: Text.AlignVCenter
        }
    }

    HoverHandler { id: tapHandler }

    TapHandler {
        onTapped: root.activated()
    }

    states: State {
        name: "selected"
        when: root.selected
        PropertyChanges { target: surface; scale: 1.0 }
    }

    transitions: Transition {
        NumberAnimation {
            target: surface
            property: "scale"
            duration: 120
        }
    }
}

