pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

SecretContentView {
    id: root
    implicitHeight: contentLayout.implicitHeight

    readonly property var detail: fields
    readonly property var totp: detail?.totp || null
    readonly property int totpPeriod: Math.max(1, Number(totp?.period) || 30)
    readonly property int totpRemaining: Math.max(0, Number(totp?.remainingSeconds) || 0)
    readonly property real totpProgress: Math.max(0, Math.min(1, totpRemaining / totpPeriod))
    readonly property bool totpInRow: !!totp && width >= Kirigami.Units.gridUnit * 56

    function formatTotpCode(value) {
        return String(value || "").replace(/(\d{3})(?=\d)/g, "$1 ");
    }

    ColumnLayout {
        id: contentLayout
        width: root.width
        spacing: Kirigami.Units.largeSpacing

        GridLayout {
            id: credentialFields
            Layout.fillWidth: true
            visible: !!root.detail
            columns: root.totpInRow ? 3 : width >= Kirigami.Units.gridUnit * 36 ? 2 : 1
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.smallSpacing

            DetailField {
                Layout.fillHeight: true
                label: KI18n.i18nc("@label:textbox", "Email or username")
                value: root.detail?.username || KI18n.i18n("No username")
                copyLabel: KI18n.i18nc("@action:button", "Copy Username")
                copyEnabled: !!root.detail?.username
                onCopyRequested: root.copyUsernameRequested()
            }
            DetailField {
                Layout.fillHeight: true
                label: KI18n.i18nc("@label", "Password")
                value: root.revealed ? root.detail?.password || KI18n.i18n("No password") : "••••••••••••"
                copyLabel: KI18n.i18nc("@action:button", "Copy Password")
                copyEnabled: !!root.detail
                revealable: true
                revealed: root.revealed
                onRevealRequested: root.revealToggled()
                onCopyRequested: root.copyPasswordRequested()
            }
            DetailField {
                Layout.fillHeight: true
                Layout.columnSpan: root.totpInRow ? 1 : Math.min(2, credentialFields.columns)
                visible: !!root.totp
                label: KI18n.i18nc("@label", "Verification code")
                value: root.formatTotpCode(root.totp?.currentCode || "")
                copyLabel: KI18n.i18nc("@action:button", "Copy Verification Code")
                copyEnabled: !!root.totp?.currentCode
                prominent: true
                hint: KI18n.i18n("Renews in %1 s", root.totpRemaining)
                progress: root.totpProgress
                progressColor: root.totpRemaining < 5 ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.highlightColor
                accessibleDescription: KI18n.i18n("Temporary code that renews automatically.")
                onCopyRequested: root.copyTotpRequested(false)

                Controls.ToolButton {
                    id: totpOptions
                    text: KI18n.i18n("More verification code options")
                    icon.name: "view-more-symbolic"
                    display: Controls.AbstractButton.IconOnly
                    onClicked: totpMenu.popup()
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text

                    Controls.Menu {
                        id: totpMenu
                        Controls.MenuItem {
                            text: KI18n.i18nc("@action:inmenu", "Copy Next Code")
                            icon.name: "edit-copy"
                            enabled: !!root.totp?.nextCode
                            onTriggered: root.copyTotpRequested(true)
                        }
                    }
                }
            }
        }

        WebsiteListCard {
            Layout.fillWidth: true
            visible: !!root.detail
            urls: root.detail?.urls || []
            onCopyUrlRequested: function (index) {
                root.copyUrlRequested(index);
            }
        }

        SecretTextSection {
            objectName: "markdownNotesSection"
            Layout.fillWidth: true
            visible: !!value
            heading: KI18n.i18nc("@title:group", "Notes")
            value: root.detail?.notes || ""
            documentKey: root.entryId
            accessibleLabel: KI18n.i18n("Item notes")
        }
    }
}
