import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Control {
    id: root
    property url source: ""
    property string fallbackIcon: "user-identity"
    property bool selected: false
    property int size: Kirigami.Units.iconSizes.large
    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    contentItem: Item {
        Image {
            id: photo
            anchors.fill: parent
            source: root.source
            sourceSize.width: root.size
            sourceSize.height: root.size
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            visible: status === Image.Ready
        }
        Kirigami.Icon {
            anchors.fill: parent
            source: root.fallbackIcon
            selected: root.selected
            visible: photo.status !== Image.Ready
        }
    }
}
