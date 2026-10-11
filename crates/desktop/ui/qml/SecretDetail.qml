pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property var detail: null
    property bool revealed: false
    property bool historyExpanded: false
    property string historyEntryId: ""
    readonly property var websiteUrls: {
        const urls = Array.isArray(detail?.urls) ? detail.urls.slice().filter(url => !!String(url || "").trim()) : [];
        if (!urls.length && detail?.url)
            urls.push(detail.url);
        return urls;
    }
    readonly property var historyItems: {
        const events = Array.isArray(detail?.history?.events) ? detail.history.events.slice() : [];
        const items = events.map(event => ({ kind: event.kind, at: event.at }));
        const rotations = Array.isArray(detail?.history?.passwordRotations) ? detail.history.passwordRotations : [];
        rotations.forEach(event => {
            const timestamp = Number(event.at);
            if (!items.some(item => item.kind === "passwordRotated" && Number(item.at) === timestamp))
                items.push({ kind: "passwordRotated", at: event.at });
        });
        return items.sort((left, right) => Number(right.at) - Number(left.at));
    }
    readonly property var visibleHistoryItems: historyExpanded ? historyItems : historyItems.slice(0, 10)
    readonly property var totp: detail?.totp || null
    readonly property int totpPeriod: Math.max(1, Number(totp?.period) || 30)
    readonly property int totpRemaining: Math.max(0, Number(totp?.remainingSeconds) || 0)
    readonly property real totpProgress: Math.max(0, Math.min(1, totpRemaining / totpPeriod))

    signal revealToggled()
    signal copyUsernameRequested()
    signal copyPasswordRequested()
    signal copyTotpRequested(bool next)

    function safeWebUrl(value) {
        const candidate = String(value || "").trim();
        if (!candidate || /[\u0000-\u0020]/.test(candidate))
            return "";
        const match = /^https?:\/\/([^/?#\s]+)(?:[/?#][^\s]*)?$/i.exec(candidate);
        return match && match[1] ? candidate : "";
    }

    function eventText(kind) {
        switch (kind) {
        case "create": return qsTr("Credencial creada");
        case "loginUse": return qsTr("Uso de esta credencial en KryptoPass; no confirma el inicio de sesión en el sitio");
        case "passwordRotated": return qsTr("Contraseña rotada");
        case "edited": return qsTr("Credencial editada");
        case "copyUsername": return qsTr("Copiaste el correo o usuario");
        case "copyPassword": return qsTr("Copiaste la contraseña");
        case "copyTotp": return qsTr("Copiaste el código TOTP actual");
        case "copyNextTotp": return qsTr("Copiaste el próximo código TOTP");
        default: return qsTr("Actividad registrada");
        }
    }

    function formatEventTime(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? "" : date.toLocaleString(Qt.locale(), Locale.ShortFormat);
    }

    onDetailChanged: {
        const nextId = detail?.id || "";
        if (nextId !== historyEntryId) {
            historyEntryId = nextId;
            historyExpanded = false;
        }
    }

    Layout.fillWidth: true
    spacing: Kirigami.Units.largeSpacing

    Controls.Frame {
        Layout.fillWidth: true
        visible: root.detail && root.detail.kind === "login"
        padding: Kirigami.Units.largeSpacing

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Kirigami.FormLayout {
                Layout.fillWidth: true

                Controls.Label {
                    Kirigami.FormData.label: qsTr("Correo o usuario:")
                    Layout.fillWidth: true
                    text: root.detail ? root.detail.username || qsTr("Sin usuario") : ""
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Controls.Button {
                    text: qsTr("Copiar usuario")
                    icon.name: "edit-copy"
                    enabled: !!root.detail && !!root.detail.username
                    onClicked: root.copyUsernameRequested()
                }
                Item { Layout.fillWidth: true }
            }

            Kirigami.FormLayout {
                Layout.fillWidth: true
                Controls.Label {
                    Kirigami.FormData.label: qsTr("Contraseña:")
                    Layout.fillWidth: true
                    text: root.detail && root.detail.password
                            ? root.detail.password
                            : root.revealed ? qsTr("Sin contraseña") : qsTr("Contraseña oculta")
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Controls.Button {
                    text: root.revealed ? qsTr("Ocultar contraseña") : qsTr("Mostrar contraseña")
                    icon.name: root.revealed ? "view-hidden" : "view-visible"
                    onClicked: root.revealToggled()
                }
                Controls.Button {
                    text: qsTr("Copiar contraseña")
                    icon.name: "edit-copy"
                    enabled: !!root.detail && root.detail.kind === "login"
                    onClicked: root.copyPasswordRequested()
                }
                Item { Layout.fillWidth: true }
            }
        }
    }

    Controls.Frame {
        Layout.fillWidth: true
        visible: !!root.detail && root.detail.kind === "login" && !!root.totp
        padding: Kirigami.Units.largeSpacing
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            Kirigami.Heading {
                level: 3
                text: qsTr("Autenticación de dos pasos")
            }
            Controls.Label {
                text: qsTr("Código actual")
                color: Kirigami.Theme.disabledTextColor
            }
            RowLayout {
                Layout.fillWidth: true
                Controls.Button {
                    text: root.totp?.currentCode || ""
                    font.family: Kirigami.Theme.fixedWidthFont.family
                    font.pointSize: Kirigami.Theme.defaultFont.pointSize + 2
                    font.weight: Font.DemiBold
                    icon.name: "edit-copy"
                    enabled: !!root.totp?.currentCode
                    Accessible.name: qsTr("Copiar código TOTP actual")
                    onClicked: root.copyTotpRequested(false)
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Se renueva en %1 s").arg(root.totpRemaining)
                    Accessible.name: qsTr("El código actual se renueva en %1 segundos").arg(root.totpRemaining)
                }
            }
            Controls.ProgressBar {
                Layout.fillWidth: true
                from: 0
                to: 1
                value: root.totpProgress
                Accessible.name: qsTr("Tiempo restante del código TOTP actual")
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Próximo código · disponible en %1 s").arg(root.totpRemaining)
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.fillWidth: true
                Controls.Button {
                    text: root.totp?.nextCode || ""
                    font.family: Kirigami.Theme.fixedWidthFont.family
                    font.pointSize: Kirigami.Theme.defaultFont.pointSize + 2
                    font.weight: Font.DemiBold
                    icon.name: "edit-copy"
                    enabled: !!root.totp?.nextCode
                    Accessible.name: qsTr("Copiar próximo código TOTP, disponible en %1 segundos").arg(root.totpRemaining)
                    onClicked: root.copyTotpRequested(true)
                }
            }
        }
    }

    Controls.Frame {
        Layout.fillWidth: true
        visible: root.detail && root.detail.kind === "login"
        padding: Kirigami.Units.largeSpacing
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            Controls.Label {
                text: qsTr("Sitios web")
                font.weight: Font.DemiBold
            }
            Repeater {
                model: root.websiteUrls
                delegate: ColumnLayout {
                    id: websiteDelegate
                    required property string modelData
                    Layout.fillWidth: true
                    Controls.Button {
                        Layout.fillWidth: true
                        visible: !!root.safeWebUrl(websiteDelegate.modelData)
                        text: websiteDelegate.modelData
                        flat: true
                        icon.name: "internet-web-browser"
                        contentItem: Controls.Label {
                            text: websiteDelegate.modelData
                            textFormat: Text.PlainText
                            wrapMode: Text.WrapAnywhere
                            horizontalAlignment: Text.AlignLeft
                        }
                        onClicked: Qt.openUrlExternally(root.safeWebUrl(websiteDelegate.modelData))
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !root.safeWebUrl(websiteDelegate.modelData)
                        text: websiteDelegate.modelData
                        textFormat: Text.PlainText
                        wrapMode: Text.WrapAnywhere
                    }
                }
            }
            Controls.Label {
                visible: root.websiteUrls.length === 0
                text: qsTr("No hay sitios web guardados.")
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }

    Controls.Frame {
        Layout.fillWidth: true
        visible: root.detail && root.detail.kind === "note"
        padding: Kirigami.Units.largeSpacing

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                text: qsTr("Contenido")
                color: Kirigami.Theme.disabledTextColor
            }
            Controls.TextArea {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 6)
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                textFormat: TextEdit.PlainText
                text: root.detail ? root.detail.value || qsTr("Sin contenido") : ""
                Accessible.name: qsTr("Contenido de la nota segura")
            }
        }
    }

    Controls.Frame {
        Layout.fillWidth: true
        visible: !!root.detail && !!root.detail.notes
        padding: Kirigami.Units.largeSpacing
        contentItem: ColumnLayout {
            Controls.Label {
                text: qsTr("Notas")
                color: Kirigami.Theme.disabledTextColor
            }
            Controls.TextArea {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 3)
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                textFormat: TextEdit.PlainText
                text: root.detail ? root.detail.notes || "" : ""
                Accessible.name: qsTr("Notas del elemento")
            }
        }
    }

    Controls.Frame {
        Layout.fillWidth: true
        visible: root.historyItems.length > 0
        padding: Kirigami.Units.largeSpacing
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            Kirigami.Heading {
                level: 3
                text: qsTr("Historial de actividad")
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Registra acciones realizadas en KryptoPass; no confirma accesos al sitio web.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            Repeater {
                model: root.visibleHistoryItems
                delegate: GridLayout {
                    id: historyItemRow
                    required property var modelData
                    Layout.fillWidth: true
                    columns: width < Kirigami.Units.gridUnit * 32 ? 1 : 2
                    columnSpacing: Kirigami.Units.largeSpacing
                    rowSpacing: Kirigami.Units.smallSpacing
                    Controls.Label {
                        Layout.row: 0
                        Layout.column: 0
                        Layout.fillWidth: true
                        text: root.eventText(historyItemRow.modelData.kind)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.row: historyItemRow.columns === 1 ? 1 : 0
                        Layout.column: historyItemRow.columns === 1 ? 0 : 1
                        Layout.fillWidth: true
                        horizontalAlignment: historyItemRow.columns === 1 ? Text.AlignLeft : Text.AlignRight
                        text: root.formatEventTime(historyItemRow.modelData.at)
                        textFormat: Text.PlainText
                        color: Kirigami.Theme.disabledTextColor
                    }
                }
            }
            Controls.Button {
                visible: root.historyItems.length > 10
                text: root.historyExpanded ? qsTr("Mostrar menos") : qsTr("Ver historial completo (%1)").arg(root.historyItems.length)
                flat: true
                onClicked: root.historyExpanded = !root.historyExpanded
            }
        }
    }
}
