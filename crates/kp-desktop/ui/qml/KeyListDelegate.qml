pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.ItemDelegate {
    id: root
    required property var entry
    property bool primary: false
    readonly property IdentityWording wording: IdentityWording {}
    readonly property string statusText: entry?.status || ""
    /// Key state; an expiry within the warning window replaces "Vigente".
    readonly property string summary: entry.isPerson ? "" : wording.health(wording.item(entry, ["expiresSoon", "partExpiresSoon"])) || statusText
    text: entry.title
    icon.name: entry.isPerson ? "view-pim-contacts" : "user-identity"
    icon.width: Kirigami.Units.iconSizes.large
    icon.height: Kirigami.Units.iconSizes.large
    padding: Kirigami.Units.largeSpacing
    Accessible.description: entry.isPerson ? entry.email : qsTr("%1. %2. Huella: %3").arg(entry.email || qsTr("Sin correo electrónico")).arg(root.summary).arg(entry.id)
    Keys.onReturnPressed: clicked()
    Keys.onEnterPressed: clicked()

    Controls.ToolTip.visible: (hovered || visualFocus) && (nameLabel.truncated || emailLabel.truncated)
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
    Controls.ToolTip.text: text + "\n" + (entry.email || qsTr("Sin correo electrónico"))

    contentItem: RowLayout {
        spacing: Kirigami.Units.largeSpacing
        KeyAvatar {
            source: root.entry.photoSource || ""
            fallbackIcon: root.icon.name
            selected: root.highlighted || root.down
            Layout.preferredWidth: root.icon.width
            Layout.preferredHeight: root.icon.height
            Layout.alignment: Qt.AlignTop
            Accessible.ignored: true
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            spacing: Kirigami.Units.smallSpacing / 2
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    id: nameLabel
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: root.text
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: root.highlighted || root.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                    Accessible.ignored: true
                }
                Controls.Label {
                    visible: root.primary
                    text: qsTr("Bóveda")
                    font: Kirigami.Theme.smallFont
                    color: root.highlighted || root.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.positiveTextColor
                }
            }
            Controls.Label {
                id: emailLabel
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: root.entry.email || qsTr("Sin correo electrónico")
                textFormat: Text.PlainText
                elide: Text.ElideMiddle
                color: root.highlighted || root.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                Accessible.ignored: true
            }
            RowLayout {
                Layout.fillWidth: true
                visible: !root.entry.isPerson
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: root.wording.shortId(root.entry.id)
                    textFormat: Text.PlainText
                    font: Kirigami.Theme.fixedWidthFont
                    color: root.highlighted || root.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                    Accessible.ignored: true
                }
                Kirigami.Icon {
                    visible: root.wording.needsAttention(root.entry)
                    source: "emblem-warning"
                    Layout.preferredWidth: Kirigami.Units.iconSizes.small
                    Layout.preferredHeight: Kirigami.Units.iconSizes.small
                    Accessible.ignored: true
                }
                Controls.Label {
                    text: root.summary
                    textFormat: Text.PlainText
                    color: root.highlighted || root.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                    Accessible.ignored: true
                }
            }
        }
    }
}
