pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Frame {
    id: root

    required property string label
    property string value: ""
    property string hint: ""
    property string copyLabel: qsTr("Copiar %1").arg(label)
    property bool copyEnabled: value.length > 0
    property bool multiline: false
    property bool revealable: false
    property bool revealed: false
    property string revealLabel: qsTr("Mostrar %1").arg(label)
    property string hideLabel: qsTr("Ocultar %1").arg(label)
    property bool prominent: false
    property real progress: -1
    default property alias extraActions: extraActions.data

    signal copyRequested()
    signal revealRequested()

    Layout.fillWidth: true
    padding: Kirigami.Units.largeSpacing

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: root.label
            textFormat: Text.PlainText
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Controls.TextArea {
                visible: root.multiline
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 5)
                text: root.value
                textFormat: TextEdit.PlainText
                wrapMode: TextEdit.Wrap
                readOnly: true
                selectByMouse: true
                Accessible.name: root.label
                Accessible.description: root.hint
            }

            Controls.TextField {
                visible: !root.multiline
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: root.value
                readOnly: true
                selectByMouse: true
                background: Rectangle {
                    color: "transparent"
                    radius: Kirigami.Units.smallSpacing
                    border.width: parent.activeFocus ? 1 : 0
                    border.color: Kirigami.Theme.highlightColor
                }
                leftPadding: 0
                rightPadding: 0
                font.family: root.revealable || root.prominent ? Kirigami.Theme.fixedWidthFont.family : Kirigami.Theme.defaultFont.family
                font.pointSize: Kirigami.Theme.defaultFont.pointSize * (root.prominent ? 1.5 : 1)
                font.weight: root.prominent ? Font.Medium : Font.Normal
                Accessible.name: root.label
                Accessible.description: root.hint
            }
            Controls.ToolButton {
                visible: root.revealable
                text: root.revealed ? root.hideLabel : root.revealLabel
                icon.name: root.revealed ? "view-hidden" : "view-visible"
                display: Controls.AbstractButton.IconOnly
                onClicked: root.revealRequested()
                Controls.ToolTip.visible: hovered || visualFocus
                Controls.ToolTip.text: text
            }
            RowLayout {
                id: extraActions
                spacing: 0
                visible: children.length > 0
            }
            Controls.ToolButton {
                text: root.copyLabel
                icon.name: "edit-copy"
                display: Controls.AbstractButton.IconOnly
                enabled: root.copyEnabled
                onClicked: root.copyRequested()
                Controls.ToolTip.visible: hovered || visualFocus
                Controls.ToolTip.text: text
            }
        }
        Controls.Label {
            Layout.fillWidth: true
            visible: root.hint.length > 0
            text: root.hint
            textFormat: Text.PlainText
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
        Controls.ProgressBar {
            Layout.fillWidth: true
            visible: root.progress >= 0
            from: 0
            to: 1
            value: Math.max(0, Math.min(1, root.progress))
            Accessible.name: qsTr("Tiempo restante del código de verificación")
        }
    }
}
