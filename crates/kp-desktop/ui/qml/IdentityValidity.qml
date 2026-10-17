pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.Control {
    id: root
    property string code: "unknown"
    property string text: ""
    readonly property bool rejected: ["never", "revoked", "invalid"].includes(code)
    readonly property int level: code === "ultimate" ? 3 : code === "full" ? 2 : code === "marginal" ? 1 : 0
    readonly property string label: {
        switch (code) {
        case "ultimate":
            return qsTr("Total");
        case "full":
            return qsTr("Plena");
        case "marginal":
            return qsTr("Marginal");
        case "never":
            return qsTr("No válida");
        case "revoked":
            return qsTr("Revocada");
        case "invalid":
            return qsTr("Inválida");
        default:
            return qsTr("Sin verificar");
        }
    }
    // Older records may still carry the former wording from the adapter.
    readonly property string displayText: code === "ultimate" ? qsTr("Validez total según GnuPG") : text || qsTr("Validez: %1").arg(label)
    readonly property string description: qsTr("Validez de la identidad calculada por GnuPG: %1. No indica la confianza en su propietario como certificador ni el estado de la clave.").arg(displayText)
    implicitWidth: implicitContentWidth
    implicitHeight: implicitContentHeight
    padding: 0
    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0
    hoverEnabled: true
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("Validez de la identidad: %1").arg(displayText)
    Accessible.description: description
    Controls.ToolTip.visible: hovered
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
    Controls.ToolTip.text: description

    contentItem: Item {
        implicitWidth: validityRow.implicitWidth
        implicitHeight: validityRow.implicitHeight

        RowLayout {
            id: validityRow
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            spacing: Kirigami.Units.smallSpacing
            Controls.Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.preferredWidth: Math.ceil(implicitWidth)
                Layout.alignment: Qt.AlignVCenter
                text: root.displayText
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: root.rejected ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
                Accessible.ignored: true
            }
            ColumnLayout {
                id: scale
                Layout.fillWidth: true
                Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                Layout.minimumWidth: Kirigami.Units.gridUnit * 8
                Layout.maximumWidth: Kirigami.Units.gridUnit * 12
                Layout.alignment: Qt.AlignVCenter
                spacing: Kirigami.Units.smallSpacing / 2
                Item {
                    id: meter
                    objectName: "identityValidityBar"
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.smallSpacing * 1.5
                    Layout.minimumHeight: Kirigami.Units.smallSpacing * 1.5

                    Rectangle {
                        width: meter.width * root.level / 3
                        height: meter.height
                        radius: height / 2
                        antialiasing: true
                        visible: root.level > 0
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop {
                                position: 0.0
                                color: Qt.rgba(Kirigami.Theme.highlightColor.r, Kirigami.Theme.highlightColor.g, Kirigami.Theme.highlightColor.b, 0)
                            }
                            GradientStop {
                                position: 1.0
                                color: Qt.rgba(Kirigami.Theme.highlightColor.r, Kirigami.Theme.highlightColor.g, Kirigami.Theme.highlightColor.b, root.level / 3)
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Repeater {
                        model: [qsTr("Marginal"), qsTr("Plena"), qsTr("Total")]
                        delegate: Controls.Label {
                            required property string modelData
                            required property int index
                            Layout.fillWidth: true
                            text: modelData
                            textFormat: Text.PlainText
                            horizontalAlignment: index === 0 ? Text.AlignLeft : index === 1 ? Text.AlignHCenter : Text.AlignRight
                            color: root.level === index + 1 ? Kirigami.Theme.textColor : Kirigami.Theme.disabledTextColor
                            Accessible.ignored: true
                        }
                    }
                }
            }
        }
    }
}
