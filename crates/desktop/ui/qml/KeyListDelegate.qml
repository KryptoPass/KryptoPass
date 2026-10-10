pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.ItemDelegate {
    id: root
    required property var entry
    text: entry.title
    icon.name: "user-identity"
    icon.width: Kirigami.Units.iconSizes.large
    icon.height: Kirigami.Units.iconSizes.large
    padding: Kirigami.Units.largeSpacing
    Accessible.description: qsTr("%1. %2. %3. Huella: %4").arg(entry.email || qsTr("Sin correo electrónico")).arg(entry.status).arg(entry.validity || qsTr("Identidad sin verificar")).arg(entry.id)
    Keys.onReturnPressed: clicked()
    Keys.onEnterPressed: clicked()

    Controls.ToolTip.visible: (hovered || visualFocus) && (nameLabel.truncated || emailLabel.truncated)
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
    Controls.ToolTip.text: text + "\n" + (entry.email || qsTr("Sin correo electrónico")) + "\n" + entry.id

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
                spacing: Kirigami.Units.largeSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: root.entry.id.slice(-16).replace(/(.{4})(?=.)/g, "$1 ")
                    font: Kirigami.Theme.fixedWidthFont
                    elide: Text.ElideLeft
                    color: root.highlighted || root.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                    Accessible.ignored: true
                }
                KeyStatus {
                    Layout.maximumWidth: parent.width / 2
                    code: root.entry.statusCode || ""
                    text: root.entry.status
                    selected: root.highlighted || root.down
                }
            }
        }
    }
}
