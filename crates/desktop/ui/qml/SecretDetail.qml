pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

GridLayout {
    id: root

    property var detail: null
    property var recipients: []
    property bool revealed: false
    property bool historyOpen: false
    property bool historyExpanded: false
    property string historyEntryId: ""
    readonly property var websiteUrls: {
        const urls = Array.isArray(detail?.urls) ? detail.urls.slice().filter(url => !!String(url || "").trim()) : [];
        if (!urls.length && detail?.url)
            urls.push(detail.url);
        return urls;
    }
    readonly property var historyItems: Array.isArray(detail?.history?.events) ? detail.history.events : []
    readonly property var visibleHistoryItems: historyItems.slice(0, historyExpanded ? 20 : 5)
    readonly property var versions: Array.isArray(detail?.history?.versions) ? detail.history.versions : []
    property var versionDetail: null
    signal versionOpenRequested(int index)
    signal versionCloseRequested()
    signal versionCopyRequested(string field)
    signal activityExportRequested()
    onVersionDetailChanged: {
        if (versionDetail)
            versionDialog.open();
        else
            versionDialog.close();
    }
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
        case "loginUse": return qsTr("Credencial utilizada");
        case "passwordRotated": return qsTr("Contraseña actualizada");
        case "edited": return qsTr("Datos editados");
        case "copyUsername": return qsTr("Usuario copiado");
        case "copyPassword": return qsTr("Contraseña copiada");
        case "copyTotp": return qsTr("Código de verificación copiado");
        case "copyNextTotp": return qsTr("Próximo código de verificación copiado");
        default: return qsTr("Actividad registrada");
        }
    }

    function formatEventTime(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? "" : date.toLocaleDateString(Qt.locale(), Locale.ShortFormat) + " " + date.toLocaleTimeString(Qt.locale(), Locale.LongFormat);
    }

    function eventIcon(kind) {
        switch (kind) {
        case "create": return "document-new";
        case "passwordRotated": return "view-refresh";
        case "edited": return "document-edit";
        case "loginUse": return "dialog-password";
        case "copyUsername":
        case "copyPassword":
        case "copyTotp":
        case "copyNextTotp": return "edit-copy";
        default: return "documentinfo";
        }
    }

    function eventDate(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? qsTr("Fecha no disponible") : date.toLocaleDateString(Qt.locale(), Locale.LongFormat);
    }

    function eventTime(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? "" : date.toLocaleTimeString(Qt.locale(), Locale.LongFormat);
    }

    onDetailChanged: {
        const nextId = detail?.id || "";
        if (nextId !== historyEntryId) {
            historyEntryId = nextId;
            historyOpen = false;
            historyExpanded = false;
        }
    }

    Layout.fillWidth: true
    readonly property bool wideLayout: detail?.kind === "login" && width >= Kirigami.Units.gridUnit * 56
    columns: wideLayout ? 2 : 1
    columnSpacing: Kirigami.Units.largeSpacing * 2
    rowSpacing: Kirigami.Units.largeSpacing

    ColumnLayout {
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        Layout.preferredWidth: root.wideLayout ? root.width * 0.65 : root.width
        Layout.alignment: Qt.AlignTop
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            Layout.fillWidth: true
            visible: root.detail && root.detail.kind === "login"
            spacing: Kirigami.Units.smallSpacing

            GridLayout {
                id: identityFields
                Layout.fillWidth: true
                columns: width >= Kirigami.Units.gridUnit * 36 ? 2 : 1
                columnSpacing: Kirigami.Units.smallSpacing
                rowSpacing: Kirigami.Units.smallSpacing

                DetailField {
                    label: qsTr("Correo o usuario")
                    value: root.detail?.username || qsTr("Sin usuario")
                    copyLabel: qsTr("Copiar usuario")
                    copyEnabled: !!root.detail?.username
                    onCopyRequested: root.copyUsernameRequested()
                }
                DetailField {
                    label: qsTr("Contraseña")
                    value: root.revealed ? root.detail?.password || qsTr("Sin contraseña") : "••••••••••••"
                    copyLabel: qsTr("Copiar contraseña")
                    copyEnabled: !!root.detail
                    revealable: true
                    revealed: root.revealed
                    onRevealRequested: root.revealToggled()
                    onCopyRequested: root.copyPasswordRequested()
                }
            }
            DetailField {
                visible: !!root.totp
                label: qsTr("Código de verificación")
                value: root.totp?.currentCode || ""
                copyLabel: qsTr("Copiar código de verificación")
                prominent: true
                hint: qsTr("Se renueva en %1 s").arg(root.totpRemaining)
                progress: root.totpProgress
                onCopyRequested: root.copyTotpRequested(false)

                Controls.ToolButton {
                    id: totpOptions
                    text: qsTr("Opciones del código de verificación")
                    icon.name: "view-more-symbolic"
                    display: Controls.AbstractButton.IconOnly
                    onClicked: totpMenu.popup()
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text

                    Controls.Menu {
                        id: totpMenu
                        Controls.MenuItem {
                            text: qsTr("Copiar próximo código")
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
            visible: root.detail && root.detail.kind === "login"
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Sitios web")
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
                        readonly property string domain: webUrl ? webUrl.replace(/^https?:\/\//i, "").split(/[/?#]/)[0] : qsTr("Enlace no válido")
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
                                text: qsTr("Abrir sitio web")
                                icon.name: "window-new"
                                display: Controls.AbstractButton.IconOnly
                                enabled: !!websiteDelegate.webUrl
                                Accessible.name: qsTr("Abrir %1 en el navegador").arg(websiteDelegate.domain)
                                Accessible.description: websiteDelegate.modelData
                                onClicked: Qt.openUrlExternally(websiteDelegate.webUrl)
                                Controls.ToolTip.visible: hovered || visualFocus
                                Controls.ToolTip.text: websiteDelegate.modelData
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
                    text: qsTr("No hay sitios web guardados.")
                    wrapMode: Text.Wrap
                }
            }
        }

        Controls.Frame {
            Layout.fillWidth: true
            visible: root.detail && root.detail.kind === "note"
            padding: Kirigami.Units.largeSpacing

            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Contenido")
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.TextArea {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 6)
                    leftPadding: 0
                    rightPadding: 0
                    background: Rectangle {
                        color: "transparent"
                        radius: Kirigami.Units.smallSpacing
                        border.width: parent.activeFocus ? 1 : 0
                        border.color: Kirigami.Theme.highlightColor
                    }
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
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Notas")
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.TextArea {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 3)
                    leftPadding: 0
                    rightPadding: 0
                    background: Rectangle {
                        color: "transparent"
                        radius: Kirigami.Units.smallSpacing
                        border.width: parent.activeFocus ? 1 : 0
                        border.color: Kirigami.Theme.highlightColor
                    }
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.Wrap
                    textFormat: TextEdit.PlainText
                    text: root.detail ? root.detail.notes || "" : ""
                    Accessible.name: qsTr("Notas del elemento")
                }
            }
        }

    }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        Layout.preferredWidth: root.wideLayout ? root.width * 0.35 : root.width
        Layout.alignment: Qt.AlignTop
        visible: root.historyItems.length > 0 || !!root.detail?.history || root.recipients.length > 0
        spacing: Kirigami.Units.largeSpacing

        Controls.Frame {
            Layout.fillWidth: true
            visible: root.historyItems.length > 0
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                Controls.ToolButton {
                    id: activityToggle
                    Layout.fillWidth: true
                    leftPadding: 0
                    rightPadding: 0
                    readonly property string sectionTitle: qsTr("Actividad")
                    text: ""
                    display: Controls.AbstractButton.IconOnly
                    focusPolicy: Qt.StrongFocus
                    checkable: true
                    checked: root.historyOpen
                    Accessible.name: qsTr("Actividad")
                    Accessible.description: root.historyOpen ? qsTr("Ocultar los %1 eventos").arg(root.historyItems.length) : qsTr("Mostrar los %1 eventos").arg(root.historyItems.length)
                    onToggled: root.historyOpen = checked
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Controls.Label {
                            Layout.fillWidth: true
                            text: activityToggle.sectionTitle
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            text: root.historyItems.length === 1 ? qsTr("1 evento") : qsTr("%1 eventos").arg(root.historyItems.length)
                            color: Kirigami.Theme.disabledTextColor
                        }
                        Kirigami.Icon {
                            source: root.historyOpen ? "arrow-up" : "arrow-down"
                            Layout.preferredWidth: Kirigami.Units.iconSizes.small
                            Layout.preferredHeight: Kirigami.Units.iconSizes.small
                        }
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !root.historyOpen
                    text: root.historyItems.length
                          ? qsTr("%1 · %2").arg(root.eventText(root.historyItems[0].kind)).arg(root.formatEventTime(root.historyItems[0].at))
                          : ""
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    visible: root.historyOpen
                    spacing: Kirigami.Units.smallSpacing

                    Repeater {
                        model: root.historyOpen ? root.visibleHistoryItems : []
                        delegate: ColumnLayout {
                            id: historyItemRow
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            readonly property bool startsDay: index === 0
                                || root.eventDate(modelData.at) !== root.eventDate(root.visibleHistoryItems[index - 1].at)

                            Controls.Label {
                                Layout.fillWidth: true
                                Layout.topMargin: historyItemRow.index > 0 ? Kirigami.Units.largeSpacing : Kirigami.Units.smallSpacing
                                visible: historyItemRow.startsDay
                                text: root.eventDate(historyItemRow.modelData.at)
                                wrapMode: Text.Wrap
                                textFormat: Text.PlainText
                                font.weight: Font.Medium
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Kirigami.Units.smallSpacing
                                Kirigami.Icon {
                                    source: root.eventIcon(historyItemRow.modelData.kind)
                                    Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                    Layout.preferredHeight: Kirigami.Units.iconSizes.small
                                    color: Kirigami.Theme.disabledTextColor
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    text: root.eventText(historyItemRow.modelData.kind)
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                }
                                Controls.Label {
                                    text: root.eventTime(historyItemRow.modelData.at)
                                    textFormat: Text.PlainText
                                    color: Kirigami.Theme.disabledTextColor
                                    Accessible.name: root.formatEventTime(historyItemRow.modelData.at)
                                }
                            }
                            Kirigami.Separator {
                                Layout.fillWidth: true
                                visible: historyItemRow.index < root.visibleHistoryItems.length - 1
                            }
                        }
                    }
                    Controls.Button {
                        visible: root.historyItems.length > 5
                        text: root.historyExpanded ? qsTr("Ver solo los últimos 5") : qsTr("Ver los últimos %1").arg(Math.min(20, root.historyItems.length))
                        flat: true
                        onClicked: root.historyExpanded = !root.historyExpanded
                    }
                    Controls.Button {
                        text: qsTr("Mostrar todos los eventos…")
                        icon.name: "document-open"
                        flat: true
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: qsTr("Generar y abrir un archivo de texto con los eventos conservados")
                        onClicked: root.activityExportRequested()
                    }
                }
            }
        }

        Controls.Frame {
            Layout.fillWidth: true
            visible: !!root.detail?.history
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Versiones anteriores")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: root.versions.length === 0
                    text: qsTr("Aún no hay versiones anteriores. Se guardan cuando actualizas los datos de esta credencial.")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
                Repeater {
                    model: root.versions
                    delegate: RowLayout {
                        id: versionRow
                        required property var modelData
                        Layout.fillWidth: true
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            spacing: 0
                            Controls.Label {
                                Layout.fillWidth: true
                                text: versionRow.modelData.passwordOnly ? qsTr("Contraseña anterior") : qsTr("Versión anterior")
                                wrapMode: Text.Wrap
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: root.formatEventTime(versionRow.modelData.at)
                                color: Kirigami.Theme.disabledTextColor
                                wrapMode: Text.Wrap
                            }
                        }
                        Controls.Button {
                            text: qsTr("Consultar")
                            Accessible.name: qsTr("Consultar versión anterior del %1").arg(root.formatEventTime(versionRow.modelData.at))
                            onClicked: root.versionOpenRequested(versionRow.modelData.index)
                        }
                    }
                }
            }
        }

        Controls.Frame {
            Layout.fillWidth: true
            visible: root.recipients.length > 0
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Destinatarios")
                    color: Kirigami.Theme.disabledTextColor
                }
                Repeater {
                    model: root.recipients
                    delegate: Controls.Label {
                        required property string modelData
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        text: modelData
                        textFormat: Text.PlainText
                        font.family: Kirigami.Theme.fixedWidthFont.family
                        wrapMode: Text.WrapAnywhere
                    }
                }
            }
        }

    }

    QtObject {
        readonly property Controls.Dialog popup: Controls.Dialog {
            id: versionDialog
            parent: Controls.Overlay.overlay
            anchors.centerIn: parent
            width: Math.min(Kirigami.Units.gridUnit * 30, parent ? parent.width - Kirigami.Units.largeSpacing * 2 : 540)
            height: Math.min(implicitHeight, parent ? parent.height - Kirigami.Units.largeSpacing * 2 : 600)
            modal: true
            title: root.versionDetail?.passwordOnly ? qsTr("Contraseña anterior") : qsTr("Versión anterior")
            standardButtons: Controls.Dialog.Close
            onClosed: root.versionCloseRequested()
            contentItem: Controls.ScrollView {
                id: versionScroll
                contentWidth: availableWidth
                clip: true
                leftPadding: Kirigami.Units.largeSpacing
                rightPadding: Kirigami.Units.largeSpacing
                topPadding: Kirigami.Units.smallSpacing
                bottomPadding: Kirigami.Units.smallSpacing
                implicitHeight: Math.min(versionFields.implicitHeight, Kirigami.Units.gridUnit * 24)
                ColumnLayout {
                    id: versionFields
                    width: versionScroll.availableWidth
                    spacing: Kirigami.Units.largeSpacing
                    Controls.Label {
                        Layout.fillWidth: true
                        text: root.formatEventTime(root.versionDetail?.at)
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !!root.versionDetail?.passwordOnly
                        text: qsTr("Este registro antiguo conserva únicamente la contraseña anterior.")
                        wrapMode: Text.Wrap
                    }
                    Kirigami.FormLayout {
                        Layout.fillWidth: true
                        Controls.Label {
                            Kirigami.FormData.label: qsTr("Nombre:")
                            Layout.fillWidth: true
                            visible: !root.versionDetail?.passwordOnly
                            text: root.versionDetail?.title || qsTr("Sin nombre")
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Kirigami.FormData.label: qsTr("Correo o usuario:")
                            Layout.fillWidth: true
                            visible: !root.versionDetail?.passwordOnly
                            text: root.versionDetail?.username || qsTr("Sin usuario")
                            textFormat: Text.PlainText
                            wrapMode: Text.WrapAnywhere
                        }
                        Controls.Label {
                            Kirigami.FormData.label: qsTr("Contraseña:")
                            text: root.versionDetail?.hasPassword ? qsTr("Contraseña oculta") : qsTr("Sin contraseña")
                        }
                        Controls.Label {
                            Kirigami.FormData.label: qsTr("Sitios web:")
                            Layout.fillWidth: true
                            visible: !root.versionDetail?.passwordOnly
                            text: root.versionDetail?.urls?.join("\n") || qsTr("Sin sitios web")
                            textFormat: Text.PlainText
                            wrapMode: Text.WrapAnywhere
                        }
                        Controls.TextArea {
                            Kirigami.FormData.label: qsTr("Notas:")
                            Layout.fillWidth: true
                            visible: !root.versionDetail?.passwordOnly && !!root.versionDetail?.notes
                            text: root.versionDetail?.notes || ""
                            textFormat: TextEdit.PlainText
                            wrapMode: TextEdit.Wrap
                            readOnly: true
                            selectByMouse: true
                            Accessible.name: qsTr("Notas de la versión anterior")
                        }
                    }
                    Controls.Button {
                        text: qsTr("Copiar contraseña anterior")
                        icon.name: "edit-copy"
                        enabled: !!root.versionDetail?.hasPassword
                        onClicked: root.versionCopyRequested("password")
                    }
                    Controls.Button {
                        visible: !root.versionDetail?.passwordOnly && !!root.versionDetail?.username
                        text: qsTr("Copiar usuario anterior")
                        icon.name: "edit-copy"
                        onClicked: root.versionCopyRequested("username")
                    }
                    Controls.Button {
                        visible: !root.versionDetail?.passwordOnly && !!root.versionDetail?.title
                        text: qsTr("Copiar nombre anterior")
                        icon.name: "edit-copy"
                        onClicked: root.versionCopyRequested("title")
                    }
                    Controls.Button {
                        visible: !root.versionDetail?.passwordOnly && !!root.versionDetail?.urls?.length
                        text: qsTr("Copiar sitios web anteriores")
                        icon.name: "edit-copy"
                        onClicked: root.versionCopyRequested("urls")
                    }
                    Controls.Button {
                        visible: !root.versionDetail?.passwordOnly && !!root.versionDetail?.notes
                        text: qsTr("Copiar notas anteriores")
                        icon.name: "edit-copy"
                        onClicked: root.versionCopyRequested("notes")
                    }
                    Controls.Button {
                        visible: !!root.versionDetail?.hasTotp
                        text: qsTr("Copiar configuración de verificación anterior")
                        icon.name: "edit-copy"
                        onClicked: root.versionCopyRequested("totp")
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: qsTr("Consultar o copiar estos datos no cambia la versión actual.")
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
    }

}
