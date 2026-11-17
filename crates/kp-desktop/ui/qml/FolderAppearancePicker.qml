pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop as Desktop
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: picker

    property string selectedIcon: "folder"
    property string selectedColor: "transparent"
    // Entries pick theme icons by their real name and have no color yet.
    property bool showColors: true
    property bool themeNames: false

    readonly property real cellSize: Kirigami.Units.gridUnit * 2.5
    readonly property real cellSpacing: Kirigami.Units.largeSpacing
    readonly property color selectionAccent: Kirigami.Theme.highlightColor

    component SelectionBackground: Rectangle {
        required property Controls.ToolButton control

        radius: Kirigami.Units.cornerRadius
        color: control.checked ? Qt.alpha(picker.selectionAccent, 0.14) : control.hovered ? Qt.alpha(Kirigami.Theme.textColor, 0.07) : "transparent"
        border.width: 1
        border.color: control.checked || control.activeFocus ? picker.selectionAccent : "transparent"
    }

    readonly property bool darkTheme: Kirigami.ColorUtils.brightnessForColor(Kirigami.Theme.backgroundColor) === Kirigami.ColorUtils.Dark
    readonly property var colorOptions: Desktop.FolderPalette.options
    property var iconOptions: [
        {
            value: "folder",
            name: KI18n.i18nc("@label", "Folder")
        },
        {
            value: "user-home",
            name: KI18n.i18nc("@item folder icon", "Home")
        },
        {
            value: "applications-office",
            name: KI18n.i18nc("@item folder icon", "Work")
        },
        {
            value: "user-identity",
            name: KI18n.i18nc("@item folder icon", "Personal")
        },
        {
            value: "folder-favorites",
            name: KI18n.i18nc("@title vault scope", "Favorites")
        },
        {
            value: "office-address-book",
            name: KI18n.i18nc("@item folder icon", "Contacts")
        },
        {
            value: "wallet-open",
            name: KI18n.i18nc("@item folder icon", "Wallet")
        },
        {
            value: "view-financial-account-credit-card",
            name: KI18n.i18nc("@item folder icon", "Cards")
        },
        {
            value: "view-calendar",
            name: KI18n.i18nc("@item folder icon", "Calendar")
        },
        {
            value: "text-x-generic",
            name: KI18n.i18nc("@title:group", "Documents")
        },
        {
            value: "applications-education",
            name: KI18n.i18nc("@item folder icon", "Studies")
        },
        {
            value: "headphones",
            name: KI18n.i18nc("@item folder icon", "Music")
        },
        {
            value: "mail-message",
            name: KI18n.i18nc("@label:textbox", "Email")
        },
        {
            value: "globe",
            name: KI18n.i18nc("@item folder icon", "Internet")
        },
        {
            value: "cloudstatus",
            name: KI18n.i18nc("@item folder icon", "Cloud")
        },
        {
            value: "network-server",
            name: KI18n.i18nc("@item folder icon", "Servers")
        },
        {
            value: "security-high",
            name: KI18n.i18nc("@label Wi-Fi security", "Security")
        },
        {
            value: "dialog-password",
            name: KI18n.i18nc("@item folder icon", "Passwords")
        },
        {
            value: "format-text-code",
            name: KI18n.i18nc("@item folder icon", "Development")
        },
        {
            value: "utilities-terminal",
            name: KI18n.i18nc("@item folder icon", "Terminal")
        },
        {
            value: "phone",
            name: KI18n.i18nc("@label:textbox", "Phone")
        },
        {
            value: "camera-photo",
            name: KI18n.i18nc("@item folder icon", "Photos")
        },
        {
            value: "applications-games",
            name: KI18n.i18nc("@item folder icon", "Games")
        },
        {
            value: "preferences-system",
            name: KI18n.i18nc("@item folder icon", "System")
        }
    ]

    signal iconSelected(string iconName)
    signal colorSelected(string colorValue)
    signal revealRequested(real top, real height)

    function revealButton(button: Item) {
        const position = button.mapToItem(picker, 0, 0);
        revealRequested(position.y, button.height);
    }

    spacing: Kirigami.Units.smallSpacing * 2
    opacity: enabled ? 1 : 0.45

    Controls.Label {
        visible: picker.showColors
        text: KI18n.i18nc("@title:group", "Color")
        font.weight: Font.DemiBold
    }

    GridLayout {
        visible: picker.showColors
        Layout.alignment: Qt.AlignLeft
        columns: 6
        rowSpacing: picker.cellSpacing
        columnSpacing: picker.cellSpacing

        Repeater {
            model: picker.colorOptions

            delegate: Controls.ToolButton {
                id: colorButton
                required property var modelData

                Layout.preferredWidth: picker.cellSize
                Layout.preferredHeight: picker.cellSize
                Layout.minimumWidth: picker.cellSize
                Layout.maximumWidth: picker.cellSize
                Layout.minimumHeight: picker.cellSize
                Layout.maximumHeight: picker.cellSize
                checkable: true
                autoExclusive: true
                checked: modelData.value === Desktop.FolderPalette.canonicalColor(picker.selectedColor)
                text: ""
                Accessible.name: modelData.name
                Accessible.description: checked ? KI18n.i18n("Selected color") : KI18n.i18n("Select color")
                focusPolicy: Qt.StrongFocus
                onActiveFocusChanged: {
                    if (activeFocus)
                        picker.revealButton(colorButton);
                }
                onClicked: picker.colorSelected(modelData.value)

                contentItem: Item {
                    Rectangle {
                        anchors.centerIn: parent
                        visible: colorButton.modelData.value !== "transparent"
                        width: Kirigami.Units.iconSizes.smallMedium
                        height: width
                        radius: width / 2
                        color: Desktop.FolderPalette.displayColor(colorButton.modelData.value, picker.darkTheme)
                        border.width: 1
                        border.color: Qt.alpha(Kirigami.Theme.textColor, 0.22)
                    }
                    Rectangle {
                        anchors.centerIn: parent
                        width: Kirigami.Units.iconSizes.smallMedium
                        height: width
                        radius: width / 2
                        visible: colorButton.modelData.value === "transparent"
                        color: "transparent"
                        border.width: 1
                        border.color: Kirigami.Theme.textColor

                        Rectangle {
                            anchors.centerIn: parent
                            width: Kirigami.Units.iconSizes.smallMedium
                            height: 1
                            rotation: -45
                            color: Kirigami.Theme.textColor
                        }
                    }
                }

                background: SelectionBackground {
                    control: colorButton
                }

                Controls.ToolTip.visible: hovered || activeFocus
                Controls.ToolTip.text: modelData.name
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }
    }

    Controls.Label {
        Layout.topMargin: picker.showColors ? Kirigami.Units.largeSpacing : 0
        text: KI18n.i18nc("@title:group", "Icon")
        font.weight: Font.DemiBold
    }

    GridLayout {
        Layout.alignment: Qt.AlignLeft
        columns: 6
        rowSpacing: picker.cellSpacing
        columnSpacing: picker.cellSpacing

        Repeater {
            model: picker.iconOptions

            delegate: Controls.ToolButton {
                id: iconButton
                required property var modelData

                Layout.preferredWidth: picker.cellSize
                Layout.preferredHeight: picker.cellSize
                Layout.minimumWidth: picker.cellSize
                Layout.maximumWidth: picker.cellSize
                Layout.minimumHeight: picker.cellSize
                Layout.maximumHeight: picker.cellSize
                checkable: true
                autoExclusive: true
                checked: modelData.value === picker.selectedIcon
                text: ""
                Accessible.name: modelData.name
                Accessible.description: checked ? KI18n.i18n("Selected icon") : KI18n.i18n("Select icon")
                focusPolicy: Qt.StrongFocus
                onActiveFocusChanged: {
                    if (activeFocus)
                        picker.revealButton(iconButton);
                }
                onClicked: picker.iconSelected(modelData.value)

                contentItem: Item {
                    FolderIcon {
                        anchors.centerIn: parent
                        visible: !picker.themeNames
                        width: Kirigami.Units.iconSizes.smallMedium
                        height: width
                        iconName: iconButton.modelData.value
                        monochrome: true
                    }
                    Kirigami.Icon {
                        anchors.centerIn: parent
                        visible: picker.themeNames
                        width: Kirigami.Units.iconSizes.smallMedium
                        height: width
                        source: iconButton.modelData.value
                        isMask: true
                        color: Kirigami.Theme.textColor
                    }
                }

                background: SelectionBackground {
                    control: iconButton
                }

                Controls.ToolTip.visible: hovered || activeFocus
                Controls.ToolTip.text: modelData.name
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }
    }
}
