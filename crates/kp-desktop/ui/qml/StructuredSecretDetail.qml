pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

SecretContentView {
    id: root

    property bool showTotp: false
    property bool showQr: false

    implicitHeight: content.implicitHeight

    ColumnLayout {
        id: content
        width: root.width
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            Layout.fillWidth: true
            visible: root.showTotp && !!root.fields?.totp
            spacing: Kirigami.Units.smallSpacing

            DetailField {
                Layout.fillWidth: true
                label: qsTr("Código de verificación")
                value: root.fields?.totp?.currentCode || ""
                copyLabel: qsTr("Copiar código de verificación")
                copyEnabled: !!root.fields?.totp?.currentCode
                prominent: true
                hint: root.fields?.totp?.remainingSeconds > 0
                    ? qsTr("Se renueva en %1 s").arg(root.fields?.totp?.remainingSeconds || 0)
                    : ""
                progress: root.fields?.totp?.period > 0
                    ? Math.max(0, Math.min(1,
                        Number(root.fields?.totp?.remainingSeconds || 0)
                        / Number(root.fields?.totp?.period || 1)))
                    : -1
                onCopyRequested: root.copyTotpRequested(false)

                Controls.ToolButton {
                    text: qsTr("Copiar próximo código")
                    icon.name: "edit-copy"
                    display: Controls.AbstractButton.IconOnly
                    enabled: !!root.fields?.totp?.nextCode
                    onClicked: root.copyTotpRequested(true)
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text
                }
            }

            Controls.Frame {
                Layout.fillWidth: true
                padding: Kirigami.Units.largeSpacing
                contentItem: GridLayout {
                    columns: width >= Kirigami.Units.gridUnit * 36 ? 2 : 1
                    columnSpacing: Kirigami.Units.largeSpacing
                    rowSpacing: Kirigami.Units.smallSpacing

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !!root.fields?.totp?.issuer
                        text: qsTr("Emisor: %1").arg(root.fields?.totp?.issuer || "")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !!root.fields?.totp?.account
                        text: qsTr("Cuenta: %1").arg(root.fields?.totp?.account || "")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !!root.fields?.totp?.algorithm
                        text: qsTr("Algoritmo: %1").arg(root.fields?.totp?.algorithm || "")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: Number(root.fields?.totp?.digits) > 0
                        text: qsTr("Dígitos: %1").arg(root.fields?.totp?.digits || 0)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: Number(root.fields?.totp?.period) > 0
                        text: qsTr("Periodo: %1 s").arg(root.fields?.totp?.period || 0)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                }
            }

        }

        ColumnLayout {
            Layout.fillWidth: true
            visible: root.showQr && !!root.fields?.hasQr
            spacing: Kirigami.Units.smallSpacing

            Controls.Button {
                objectName: "qrToggle"
                text: root.fields?.qrSource ? qsTr("Ocultar código QR") : qsTr("Mostrar código QR")
                icon.name: "view-barcode-qr"
                onClicked: root.qrRequested()
                Accessible.name: text
            }
            Image {
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                Layout.preferredHeight: Kirigami.Units.gridUnit * 12
                visible: !!root.fields?.qrSource
                source: root.fields?.qrSource || ""
                sourceSize.width: Kirigami.Units.gridUnit * 12
                sourceSize.height: Kirigami.Units.gridUnit * 12
                fillMode: Image.PreserveAspectFit
                cache: false
                Accessible.name: qsTr("Código QR")
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: root.showQr && !root.fields?.hasQr && !!root.fields?.qrUnavailableReason
            text: root.fields?.qrUnavailableReason || ""
            textFormat: Text.PlainText
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        ColumnLayout {
            Layout.fillWidth: true
            visible: (root.fields?.rows || []).length > 0
            spacing: Kirigami.Units.smallSpacing

            Repeater {
                model: root.fields?.rows || []
                delegate: DetailField {
                    id: fieldRow
                    required property int index
                    required property var modelData
                    Layout.fillWidth: true
                    label: String(modelData.label || qsTr("Campo"))
                    value: !modelData.hasValue
                        ? qsTr("Sin valor")
                        : modelData.concealed && !modelData.revealed
                          ? "••••••••"
                          : String(modelData.value || "")
                    hint: String(modelData.hint || "")
                    multiline: !!modelData.multiline
                        || String(modelData.value || "").indexOf("\n") >= 0
                        || String(modelData.value || "").indexOf("\r") >= 0
                    copyLabel: qsTr("Copiar %1").arg(label)
                    copyEnabled: !!modelData.hasValue
                    revealable: !!modelData.concealed && !!modelData.hasValue
                    revealed: !!modelData.revealed
                    Accessible.name: label
                    onCopyRequested: root.copyFieldRequested(String(modelData.id || ""))
                    onRevealRequested: root.revealFieldRequested(String(modelData.id || ""))
                }
            }
        }

        SecretTextSection {
            objectName: "markdownNotesSection"
            Layout.fillWidth: true
            visible: !!root.fields?.notes
            heading: qsTr("Notas")
            value: root.fields?.notes || ""
            documentKey: root.entryId
            accessibleLabel: qsTr("Notas del elemento")
        }
    }
}
