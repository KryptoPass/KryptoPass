pragma ComponentBehavior: Bound
import QtQuick
import org.kde.kirigami as Kirigami

Item {
    id: root

    property string iconName: "folder"
    property string colorValue: "transparent"
    property bool highlighted: false
    property bool monochrome: false
    property color foregroundColor: foregroundFor(colorValue)

    readonly property bool hasTint: colorValue.length > 0 && colorValue !== "transparent"
    readonly property string sourceName: {
        switch (iconName) {
        case "folder": return "folder-symbolic";
        case "user-home": return "go-home";
        case "applications-office": return "applications-office-symbolic";
        case "applications-games": return "applications-games-symbolic";
        case "preferences-system": return "configure";
        case "office-address-book": return "address-book-new";
        case "text-x-generic": return "document-new";
        case "folder-favorites": return "starred";
        case "wallet-open": return "wallet-open";
        case "view-calendar": return "view-calendar";
        default: return iconName || "folder-symbolic";
        }
    }
    readonly property bool maskableIcon: [
        "folder-symbolic",
        "go-home",
        "applications-office-symbolic",
        "applications-games-symbolic",
        "configure",
        "address-book-new",
        "document-new",
        "starred",
        "wallet-open",
        "view-calendar"
    ].includes(sourceName)

    function foregroundFor(colorValue) {
        if (!colorValue || colorValue === "transparent")
            return highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor;
        const color = Qt.color(colorValue);
        const luminance = 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
        return luminance > 0.55 ? "#202020" : "#ffffff";
    }

    implicitWidth: Kirigami.Units.iconSizes.smallMedium
    implicitHeight: Kirigami.Units.iconSizes.smallMedium
    Accessible.ignored: true

    Rectangle {
        width: Math.min(root.width, root.height)
        height: width
        anchors.centerIn: parent
        radius: width / 2
        visible: root.hasTint
        color: root.hasTint ? root.colorValue : "transparent"
    }

    Kirigami.Icon {
        anchors.centerIn: parent
        width: Kirigami.Units.iconSizes.smallMedium
        height: width
        source: root.sourceName
        fallback: "folder-symbolic"
        isMask: (root.hasTint || root.monochrome) && root.maskableIcon
        color: root.foregroundColor
    }
}
