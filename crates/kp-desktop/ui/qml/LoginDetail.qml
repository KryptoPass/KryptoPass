pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

SecretContentView {
    id: root
    implicitHeight: contentLayout.implicitHeight

    readonly property var detail: fields
    readonly property var websiteUrls: {
        return Array.isArray(detail?.urls) ? detail.urls.slice().filter(url => !!String(url || "").trim()) : [];
    }
    readonly property var totp: detail?.totp || null
    readonly property int totpPeriod: Math.max(1, Number(totp?.period) || 30)
    readonly property int totpRemaining: Math.max(0, Number(totp?.remainingSeconds) || 0)
    readonly property real totpProgress: Math.max(0, Math.min(1, totpRemaining / totpPeriod))
    readonly property bool totpInRow: !!totp && width >= Kirigami.Units.gridUnit * 56

    function safeWebUrl(value) {
        const candidate = String(value || "").trim();
        if (!candidate || /[\u0000-\u0020]/.test(candidate))
            return "";
        const match = /^https?:\/\/([^/?#\s]+)(?:[/?#][^\s]*)?$/i.exec(candidate);
        return match && match[1] ? candidate : "";
    }

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

        Controls.Frame {
            Layout.fillWidth: true
            visible: !!root.detail
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTranslate("SecretDetail", "Sitios web")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
                Repeater {
                    model: root.websiteUrls
                    delegate: ColumnLayout {
                        id: websiteDelegate
                        required property string modelData
                        required property int index
                        readonly property string webUrl: root.safeWebUrl(modelData)
                        readonly property string domain: webUrl ? webUrl.replace(/^https?:\/\//i, "").split(/[/?#]/)[0] : qsTranslate("SecretDetail", "Enlace no válido")
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: Kirigami.Units.smallSpacing
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: websiteDelegate.domain
                                    textFormat: Text.PlainText
                                    elide: Text.ElideMiddle
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    text: websiteDelegate.modelData
                                    textFormat: Text.PlainText
                                    color: Kirigami.Theme.disabledTextColor
                                    elide: Text.ElideMiddle
                                    Accessible.name: websiteDelegate.modelData
                                }
                            }
                            Controls.ToolButton {
                                text: qsTranslate("SecretDetail", "Abrir sitio web")
                                icon.name: "external-link-symbolic"
                                display: Controls.AbstractButton.IconOnly
                                enabled: !!websiteDelegate.webUrl
                                Accessible.name: qsTranslate("SecretDetail", "Abrir %1 en el navegador").arg(websiteDelegate.domain)
                                Accessible.description: websiteDelegate.modelData
                                onClicked: Qt.openUrlExternally(websiteDelegate.webUrl)
                                Controls.ToolTip.visible: hovered || visualFocus
                                Controls.ToolTip.text: websiteDelegate.modelData
                            }
                            Controls.ToolButton {
                                text: qsTranslate("SecretDetail", "Copiar sitio web")
                                icon.name: "edit-copy"
                                display: Controls.AbstractButton.IconOnly
                                enabled: !!websiteDelegate.modelData
                                Accessible.name: qsTranslate("SecretDetail", "Copiar %1").arg(websiteDelegate.domain)
                                Accessible.description: websiteDelegate.modelData
                                onClicked: root.copyUrlRequested(websiteDelegate.index)
                                Controls.ToolTip.visible: hovered || visualFocus
                                Controls.ToolTip.text: text
                            }
                        }
                        Kirigami.Separator {
                            Layout.fillWidth: true
                            visible: websiteDelegate.index < root.websiteUrls.length - 1
                        }
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: root.websiteUrls.length === 0
                    text: qsTranslate("SecretDetail", "No hay sitios web guardados.")
                    wrapMode: Text.Wrap
                }
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
