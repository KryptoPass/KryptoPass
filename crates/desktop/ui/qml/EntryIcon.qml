import QtQuick
import org.kde.kirigami as Kirigami

Item {
    id: root

    property var entryIcon: null
    property url imageSource: ""
    property string fallbackIcon: "document-encrypted"
    property bool selected: false
    property int iconSize: Kirigami.Units.iconSizes.smallMedium

    implicitWidth: iconSize
    implicitHeight: iconSize
    Accessible.ignored: true

    Image {
        id: image
        anchors.centerIn: parent
        width: Math.min(root.width, implicitWidth)
        height: Math.min(root.height, implicitHeight)
        source: root.entryIcon?.kind === "theme" ? "" : root.imageSource
        sourceSize.width: 256
        sourceSize.height: 256
        asynchronous: true
        fillMode: Image.PreserveAspectFit
        visible: status === Image.Ready
        Accessible.ignored: true
    }
    Kirigami.Icon {
        anchors.fill: parent
        source: root.entryIcon?.kind === "theme" ? root.entryIcon.name : root.fallbackIcon
        fallback: root.fallbackIcon
        selected: root.selected
        visible: image.status !== Image.Ready
        Accessible.ignored: true
    }
}
