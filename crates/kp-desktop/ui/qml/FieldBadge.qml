pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Rectangle {
    id: root

    property string text: ""

    visible: text.length > 0
    implicitWidth: badgeLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
    implicitHeight: badgeLabel.implicitHeight + 2
    color: Qt.rgba(Kirigami.Theme.neutralTextColor.r, Kirigami.Theme.neutralTextColor.g, Kirigami.Theme.neutralTextColor.b, 0.15)
    radius: height / 2
    Accessible.role: Accessible.StaticText
    Accessible.name: text

    Controls.Label {
        id: badgeLabel
        anchors.centerIn: parent
        text: root.text
        color: Kirigami.Theme.neutralTextColor
        font: Kirigami.Theme.smallFont
        Accessible.ignored: true
    }
}
