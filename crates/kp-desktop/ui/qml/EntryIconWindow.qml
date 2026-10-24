pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.ApplicationWindow {
    id: iconWindow
    objectName: "entryIconWindow"

    property string entryTitle: ""
    property string kindName: ""
    property string fallbackIcon: "document-encrypted"
    property string initialIcon: ""
    property string selectedIcon: ""
    readonly property bool opened: visible
    readonly property bool canAccept: selectedIcon.length > 0 && selectedIcon !== initialIcon
    // Breeze symbolic glyphs, so every option shares one monochrome style.
    readonly property var iconNames: ["insert-link", "globe", "mail-message", "applications-internet", "applications-office", "user-identity", "network-server", "drive-harddisk", "computer-laptop", "smartphone", "phone", "network-wireless", "printer", "wallet-open", "view-financial-account-credit-card", "shopping-cart", "view-calendar", "bookmarks", "favorite", "starred", "tag", "flag", "emblem-important", "lock", "security-high", "cloudstatus", "camera-photo", "video-television", "music-note-16th", "headphones", "applications-games", "applications-education", "applications-science", "applications-graphics", "applications-development", "utilities-terminal", "format-text-code", "view-pim-contacts", "document-new", "folder", "go-home", "help-about", "preferences-system"]
    readonly property var iconOptions: iconNames.map(name => ({
                value: name + "-symbolic",
                name: name.replace(/-/g, " ")
            }))

    signal iconChosen(string iconName)

    flags: Qt.Dialog | Qt.WindowTitleHint | Qt.WindowSystemMenuHint | Qt.WindowCloseButtonHint
    modality: Qt.WindowModal
    width: Kirigami.Units.gridUnit * 20
    height: Kirigami.Units.gridUnit * 26
    minimumWidth: Kirigami.Units.gridUnit * 18
    minimumHeight: Kirigami.Units.gridUnit * 16
    title: qsTr("Icono del tema")

    function openWith(iconName: string, titleText: string, kindText: string, kindIcon: string) {
        initialIcon = iconName;
        selectedIcon = iconName;
        entryTitle = titleText;
        kindName = kindText;
        fallbackIcon = kindIcon;
        if (!visible) {
            x = screen.virtualX + Math.round((screen.width - width) / 2);
            y = screen.virtualY + Math.round((screen.height - height) / 2);
        }
        show();
        raise();
        requestActivate();
    }

    function accept() {
        if (!canAccept)
            return;
        iconChosen(selectedIcon);
        close();
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        context: Qt.WindowShortcut
        enabled: iconWindow.opened
        onActivated: iconWindow.close()
    }

    header: PanelHeader {
        headerHeight: Kirigami.Units.gridUnit * 3
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing
            EntryIcon {
                Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                Layout.preferredHeight: Kirigami.Units.iconSizes.medium
                iconSize: Kirigami.Units.iconSizes.medium
                entryIcon: iconWindow.selectedIcon ? ({
                        kind: "theme",
                        name: iconWindow.selectedIcon
                    }) : null
                fallbackIcon: iconWindow.fallbackIcon
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                spacing: 0
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 4
                    text: iconWindow.entryTitle || qsTr("Elemento sin nombre")
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: text.length > 0
                    text: iconWindow.kindName
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    opacity: 0.7
                }
            }
        }
    }

    Controls.ScrollView {
        id: iconScroll
        anchors.fill: parent
        contentWidth: availableWidth
        contentHeight: iconPane.implicitHeight
        clip: true
        Controls.ScrollBar.horizontal.policy: Controls.ScrollBar.AlwaysOff

        Controls.Pane {
            id: iconPane
            width: iconScroll.availableWidth
            leftPadding: 20
            rightPadding: 20
            topPadding: 16
            bottomPadding: 16
            background: null

            contentItem: FolderAppearancePicker {
                showColors: false
                themeNames: true
                iconOptions: iconWindow.iconOptions
                selectedIcon: iconWindow.selectedIcon
                onIconSelected: iconName => iconWindow.selectedIcon = iconName
                onRevealRequested: function (top, height) {
                    const flickable = iconScroll.contentItem as Flickable;
                    if (!flickable)
                        return;
                    const offset = top + iconPane.topPadding;
                    if (offset < flickable.contentY)
                        flickable.contentY = offset;
                    else if (offset + height > flickable.contentY + flickable.height)
                        flickable.contentY = Math.min(offset + height - flickable.height, Math.max(0, flickable.contentHeight - flickable.height));
                }
            }
        }
    }

    footer: PanelFooter {
        headerHeight: Kirigami.Units.gridUnit * 2.5
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing
            Item {
                Layout.fillWidth: true
            }
            Controls.Button {
                text: qsTr("Cancelar")
                icon.name: "dialog-cancel"
                onClicked: iconWindow.close()
            }
            Controls.Button {
                text: qsTr("Aceptar")
                icon.name: "dialog-ok-apply"
                enabled: iconWindow.canAccept
                onClicked: iconWindow.accept()
            }
        }
    }
}
