pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop as Desktop
import org.kde.kirigami as Kirigami

Item {
    id: root

    property string iconName: "folder"
    property string colorValue: "transparent"
    property bool highlighted: false
    property bool monochrome: false
    readonly property bool darkTheme: Kirigami.ColorUtils.brightnessForColor(Kirigami.Theme.backgroundColor) === Kirigami.ColorUtils.Dark
    readonly property color foregroundColor: darkTheme ? "#ffffff" : "#000000"
    readonly property color displayColor: Desktop.FolderPalette.displayColor(colorValue, darkTheme)

    readonly property bool hasTint: colorValue.length > 0 && colorValue !== "transparent"
    readonly property string sourceName: {
        switch (iconName) {
        case "folder": return "folder-symbolic";
        case "user-home": return "go-home";
        case "applications-office": return "view-bank-symbolic";
        case "applications-games": return "applications-games-symbolic";
        case "preferences-system": return "configure";
        case "office-address-book": return "view-pim-contacts-symbolic";
        case "text-x-generic": return "document-new";
        case "folder-favorites": return "starred";
        case "wallet-open": return "wallet-open-symbolic";
        case "view-calendar": return "view-calendar-symbolic";
        case "user-identity": return "user-identity-symbolic";
        case "view-financial-account-credit-card": return "view-financial-account-credit-card-symbolic";
        case "applications-education": return "applications-education-symbolic";
        case "headphones": return "headphones-symbolic";
        case "mail-message": return "mail-message-symbolic";
        case "globe": return "globe-symbolic";
        case "cloudstatus": return "cloudstatus-symbolic";
        case "network-server": return "network-server-symbolic";
        case "security-high": return "firewall-applet-shields_up-symbolic";
        case "dialog-password": return "lock-symbolic";
        case "format-text-code": return "format-text-code-symbolic";
        case "utilities-terminal": return "utilities-terminal-symbolic";
        case "phone": return "phone-symbolic";
        case "camera-photo": return "camera-photo-symbolic";
        default: return iconName || "folder-symbolic";
        }
    }
    readonly property bool maskableIcon: [
        "folder-symbolic",
        "go-home",
        "view-bank-symbolic",
        "applications-games-symbolic",
        "configure",
        "view-pim-contacts-symbolic",
        "document-new",
        "starred",
        "wallet-open-symbolic",
        "view-calendar-symbolic",
        "user-identity-symbolic",
        "view-financial-account-credit-card-symbolic",
        "applications-education-symbolic",
        "headphones-symbolic",
        "mail-message-symbolic",
        "globe-symbolic",
        "cloudstatus-symbolic",
        "network-server-symbolic",
        "firewall-applet-shields_up-symbolic",
        "lock-symbolic",
        "format-text-code-symbolic",
        "utilities-terminal-symbolic",
        "phone-symbolic",
        "camera-photo-symbolic"
    ].includes(sourceName)

    implicitWidth: Kirigami.Units.iconSizes.smallMedium
    implicitHeight: Kirigami.Units.iconSizes.smallMedium
    Accessible.ignored: true

    Rectangle {
        width: Math.min(root.width, root.height)
        height: width
        anchors.centerIn: parent
        radius: width / 2
        visible: root.hasTint
        color: root.hasTint ? root.displayColor : "transparent"
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
