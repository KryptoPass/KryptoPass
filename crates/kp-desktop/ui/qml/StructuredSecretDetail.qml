pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

SecretContentView {
    id: root

    property bool showTotp: false
    property bool showQr: false
    readonly property var totp: root.fields?.totp || null

    implicitHeight: content.implicitHeight

    function formatTotpCode(value) {
        return String(value || "").replace(/(\d{3})(?=\d)/g, "$1 ");
    }

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
                label: KI18n.i18nc("@label", "Verification code")
                value: root.formatTotpCode(root.totp?.currentCode || "")
                copyLabel: KI18n.i18nc("@action:button", "Copy Verification Code")
                copyEnabled: !!root.totp?.currentCode
                prominent: true
                hint: root.totp?.remainingSeconds > 0 ? KI18n.i18n("Renews in %1 s", root.totp?.remainingSeconds || 0) : ""
                progress: root.totp?.period > 0 ? Math.max(0, Math.min(1, Number(root.totp?.remainingSeconds || 0) / Number(root.totp?.period || 1))) : -1
                progressColor: Number(root.totp?.remainingSeconds) < 5 ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.highlightColor
                accessibleDescription: KI18n.i18n("Temporary code that renews automatically.")
                onCopyRequested: root.copyTotpRequested(false)

                Controls.ToolButton {
                    text: KI18n.i18nc("@action:inmenu", "Copy Next Code")
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
                        text: KI18n.i18n("Issuer: %1", root.fields?.totp?.issuer || "")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !!root.fields?.totp?.account
                        text: KI18n.i18n("Account: %1", root.fields?.totp?.account || "")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !!root.fields?.totp?.algorithm
                        text: KI18n.i18n("Algorithm: %1", root.fields?.totp?.algorithm || "")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: Number(root.fields?.totp?.digits) > 0
                        text: KI18n.i18n("Digits: %1", root.fields?.totp?.digits || 0)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: Number(root.fields?.totp?.period) > 0
                        text: KI18n.i18n("Period: %1 s", root.fields?.totp?.period || 0)
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
                text: root.fields?.qrSource ? KI18n.i18nc("@action:button", "Hide QR Code") : KI18n.i18nc("@action:button", "Show QR Code")
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
                Accessible.name: KI18n.i18n("QR code")
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
                    label: String(modelData.label || KI18n.i18nc("@label fallback field name", "Field"))
                    value: !modelData.hasValue ? KI18n.i18n("No value") : modelData.concealed && !modelData.revealed ? "••••••••" : String(modelData.value || "")
                    hint: String(modelData.hint || "")
                    multiline: !!modelData.multiline || String(modelData.value || "").indexOf("\n") >= 0 || String(modelData.value || "").indexOf("\r") >= 0
                    copyLabel: KI18n.i18nc("@action:button %1 is a field name", "Copy %1", label)
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
            heading: KI18n.i18nc("@title:group", "Notes")
            value: root.fields?.notes || ""
            documentKey: root.entryId
            accessibleLabel: KI18n.i18n("Item notes")
        }
    }
}
