pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

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
                label: qsTranslate("SecretDetail", "Correo o usuario")
                value: root.detail?.username || qsTranslate("SecretDetail", "Sin usuario")
                copyLabel: qsTranslate("SecretDetail", "Copiar usuario")
                copyEnabled: !!root.detail?.username
                onCopyRequested: root.copyUsernameRequested()
            }
            DetailField {
                Layout.fillHeight: true
                label: qsTranslate("SecretDetail", "Contraseña")
                value: root.revealed ? root.detail?.password || qsTranslate("SecretDetail", "Sin contraseña") : "••••••••••••"
                copyLabel: qsTranslate("SecretDetail", "Copiar contraseña")
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
                label: qsTranslate("SecretDetail", "Código de verificación")
                value: root.formatTotpCode(root.totp?.currentCode || "")
                copyLabel: qsTranslate("SecretDetail", "Copiar código de verificación")
                copyEnabled: !!root.totp?.currentCode
                prominent: true
                hint: qsTranslate("SecretDetail", "Se renueva en %1 s").arg(root.totpRemaining)
                progress: root.totpProgress
                progressColor: root.totpRemaining < 5 ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.highlightColor
                accessibleDescription: qsTranslate("SecretDetail", "Código temporal que se renueva automáticamente.")
                onCopyRequested: root.copyTotpRequested(false)

                Controls.ToolButton {
                    id: totpOptions
                    text: qsTranslate("SecretDetail", "Más opciones del código de verificación")
                    icon.name: "view-more-symbolic"
                    display: Controls.AbstractButton.IconOnly
                    onClicked: totpMenu.popup()
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text

                    Controls.Menu {
                        id: totpMenu
                        Controls.MenuItem {
                            text: qsTranslate("SecretDetail", "Copiar próximo código")
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
            heading: qsTranslate("SecretDetail", "Notas")
            value: root.detail?.notes || ""
            documentKey: root.entryId
            accessibleLabel: qsTranslate("SecretDetail", "Notas del elemento")
        }
    }
}
