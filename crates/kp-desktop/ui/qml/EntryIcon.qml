import QtQuick
import org.kde.kirigami as Kirigami

Item {
    id: root

    property var entryIcon: null
    property url imageSource: ""
    property string fallbackIcon: "document-encrypted"
    property bool selected: false
    property int iconSize: Kirigami.Units.iconSizes.smallMedium
    readonly property bool favicon: entryIcon?.kind === "favicon"

    implicitWidth: iconSize
    implicitHeight: iconSize
    Accessible.ignored: true

    Image {
        id: image
        anchors.centerIn: parent
        width: Math.min(root.width, root.iconSize)
        height: Math.min(root.height, root.iconSize)
        source: root.entryIcon?.kind === "theme" || root.favicon ? "" : root.imageSource
        sourceSize: Qt.size(Math.ceil(root.iconSize * Screen.devicePixelRatio), Math.ceil(root.iconSize * Screen.devicePixelRatio))
        asynchronous: true
        fillMode: Image.PreserveAspectFit
        visible: status === Image.Ready
        Accessible.ignored: true
    }
    Kirigami.Icon {
        anchors.fill: parent
        source: root.entryIcon?.kind === "theme" ? root.entryIcon.name : root.favicon && root.imageSource.toString().length > 0 ? root.imageSource : root.fallbackIcon
        fallback: root.fallbackIcon
        placeholder: root.fallbackIcon
        selected: root.selected
        visible: image.status !== Image.Ready
        Accessible.ignored: true
    }
}
