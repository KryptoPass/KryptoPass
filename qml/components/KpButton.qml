import QtQuick
import QtQuick.Controls

Button {
    id: control

    property color accentColor: "#6c5ce7"
    property color foregroundColor: "#ffffff"
    property bool outlined: false

    implicitWidth: Math.max(148, contentItem.implicitWidth + 40)
    implicitHeight: 46
    hoverEnabled: true
    padding: 0

    contentItem: Text {
        text: control.text
        color: control.enabled
            ? (control.outlined ? control.accentColor : control.foregroundColor)
            : "#99a0ae"
        font.pixelSize: 14
        font.weight: Font.DemiBold
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        id: buttonSurface

        radius: 13
        color: control.outlined
            ? (control.hovered ? "#f1efff" : "transparent")
            : (control.hovered ? "#5d4bd8" : control.accentColor)
        border.width: control.outlined ? 1 : 0
        border.color: control.accentColor
        opacity: control.enabled ? 1 : 0.55

        Behavior on color {
            ColorAnimation { duration: 130 }
        }
    }

    states: [
        State {
            name: "pressed"
            when: control.down
            PropertyChanges { target: buttonSurface; scale: 0.97 }
        },
        State {
            name: "resting"
            when: !control.down
            PropertyChanges { target: buttonSurface; scale: 1.0 }
        }
    ]

    transitions: Transition {
        NumberAnimation {
            target: buttonSurface
            property: "scale"
            duration: 110
            easing.type: Easing.OutCubic
        }
    }
}

