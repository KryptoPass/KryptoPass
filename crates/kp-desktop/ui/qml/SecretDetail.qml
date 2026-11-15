pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property var detail: null
    property var recipients: []
    property var recipientKeys: []
    property bool revealed: false
    property var versionDetail: null
    property bool historyOpen: false
    property bool historyExpanded: false
    property string historyEntryId: ""
    property Item versionReturnItem: null
    readonly property SecretContentView contentView: contentLoader.item as SecretContentView
    readonly property var historyItems: Array.isArray(root.detail?.content?.fields?.history?.events) ? root.detail.content.fields.history.events : []
    readonly property var visibleHistoryItems: root.historyItems.slice(0, root.historyExpanded ? 20 : 5)
    readonly property var versions: Array.isArray(root.detail?.content?.fields?.history?.versions) ? root.detail.content.fields.history.versions : []
    readonly property bool hasHistory: !!root.detail?.content?.fields?.history
    readonly property bool wideLayout: root.width >= Kirigami.Units.gridUnit * 56
    readonly property bool bottomTwoColumns: root.wideLayout && root.hasHistory && root.recipients.length > 0

    signal revealToggled
    signal copyUsernameRequested
    signal copyPasswordRequested
    signal copyTotpRequested(bool next)
    signal copyFieldRequested(string field)
    signal copyUrlRequested(int index)
    signal revealFieldRequested(string field)
    signal qrRequested
    signal versionOpenRequested(int index)
    signal versionCloseRequested
    signal versionCopyRequested(string field)
    signal versionCopyUrlRequested(int index)
    signal versionRevealRequested
    signal activityExportRequested

    spacing: Kirigami.Units.largeSpacing

    function eventText(kind) {
        switch (kind) {
        case "create":
            return qsTranslate("SecretDetail", "Credencial creada");
        case "loginUse":
            return qsTranslate("SecretDetail", "Credencial utilizada");
        case "passwordRotated":
            return qsTranslate("SecretDetail", "Contraseña actualizada");
        case "edited":
            return qsTranslate("SecretDetail", "Datos editados");
        case "copyUsername":
            return qsTranslate("SecretDetail", "Usuario copiado");
        case "copyPassword":
            return qsTranslate("SecretDetail", "Contraseña copiada");
        case "copyTotp":
            return qsTranslate("SecretDetail", "Código de verificación copiado");
        case "copyNextTotp":
            return qsTranslate("SecretDetail", "Próximo código de verificación copiado");
        default:
            return qsTranslate("SecretDetail", "Actividad registrada");
        }
    }

    function formatEventTime(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? "" : date.toLocaleDateString(Qt.locale(), Locale.ShortFormat) + " " + date.toLocaleTimeString(Qt.locale(), Locale.LongFormat);
    }

    function eventIcon(kind) {
        switch (kind) {
        case "create":
            return "document-new";
        case "passwordRotated":
            return "view-refresh";
        case "edited":
            return "document-edit";
        case "loginUse":
            return "dialog-password";
        case "copyUsername":
        case "copyPassword":
        case "copyTotp":
        case "copyNextTotp":
            return "edit-copy";
        default:
            return "documentinfo";
        }
    }

    function eventDate(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? qsTranslate("SecretDetail", "Fecha no disponible") : date.toLocaleDateString(Qt.locale(), Locale.LongFormat);
    }

    function eventTime(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? "" : date.toLocaleTimeString(Qt.locale(), Locale.LongFormat);
    }

    onDetailChanged: {
        const nextId = root.detail?.id || "";
        if (nextId !== root.historyEntryId) {
            root.historyEntryId = nextId;
            root.historyOpen = false;
            root.historyExpanded = false;
        }
    }

    Loader {
        id: contentLoader
        Layout.fillWidth: true
        sourceComponent: EntryTypes.find(root.detail?.content?.kind)?.detail || null
    }

    Binding {
        target: root.contentView
        property: "fields"
        value: root.detail?.content?.fields || null
        when: !!root.contentView
    }
    Binding {
        target: root.contentView
        property: "entryId"
        value: root.detail?.id || ""
        when: !!root.contentView
    }
    Binding {
        target: root.contentView
        property: "revealed"
        value: root.revealed
        when: !!root.contentView
    }
    Binding {
        target: root.contentView
        property: "versionDetail"
        value: root.versionDetail
        when: !!root.contentView
    }

    Connections {
        target: root.contentView
        function onRevealToggled() {
            root.revealToggled();
        }
        function onCopyUsernameRequested() {
            root.copyUsernameRequested();
        }
        function onCopyPasswordRequested() {
            root.copyPasswordRequested();
        }
        function onCopyTotpRequested(next) {
            root.copyTotpRequested(next);
        }
        function onCopyFieldRequested(field) {
            root.copyFieldRequested(field);
        }
        function onCopyUrlRequested(index) {
            root.copyUrlRequested(index);
        }
        function onRevealFieldRequested(field) {
            root.revealFieldRequested(field);
        }
        function onQrRequested() {
            root.qrRequested();
        }
        function onVersionOpenRequested(index) {
            root.versionOpenRequested(index);
        }
        function onVersionCloseRequested() {
            root.versionCloseRequested();
        }
        function onVersionCopyRequested(field) {
            root.versionCopyRequested(field);
        }
        function onActivityExportRequested() {
            root.activityExportRequested();
        }
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: !!root.detail && !EntryTypes.find(root.detail.content?.kind)
        type: Kirigami.MessageType.Error
        text: qsTr("No se puede mostrar este tipo de elemento.")
    }

    GridLayout {
        Layout.fillWidth: true
        columns: root.bottomTwoColumns ? 2 : 1
        columnSpacing: Kirigami.Units.largeSpacing
        rowSpacing: Kirigami.Units.largeSpacing

        Controls.Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.recipients.length > 0
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Quién tiene acceso")
                    color: Kirigami.Theme.disabledTextColor
                }
                Repeater {
                    model: root.recipients
                    delegate: ColumnLayout {
                        id: recipientRow
                        required property string modelData
                        readonly property var recipientKey: root.recipientKeys.find(key => key.id.toUpperCase() === recipientRow.modelData.toUpperCase()) || null
                        readonly property string recipientName: recipientKey?.contactName || recipientKey?.title || recipientKey?.email || qsTr("Clave no disponible")
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        spacing: Kirigami.Units.smallSpacing

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: 2
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: recipientRow.recipientName
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    visible: !!recipientRow.recipientKey?.email
                                    text: recipientRow.recipientKey?.email || ""
                                    textFormat: Text.PlainText
                                    color: Kirigami.Theme.disabledTextColor
                                    wrapMode: Text.WrapAnywhere
                                }
                            }
                            Controls.ToolButton {
                                id: recipientDetails
                                text: checked ? qsTr("Ocultar detalles de la clave") : qsTr("Ver detalles de la clave")
                                icon.name: checked ? "arrow-up" : "arrow-down"
                                display: Controls.AbstractButton.IconOnly
                                checkable: true
                                Accessible.name: qsTr("%1: %2").arg(text).arg(recipientRow.recipientName)
                                Controls.ToolTip.visible: hovered || visualFocus
                                Controls.ToolTip.text: text
                            }
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: recipientDetails.checked
                            text: qsTr("Huella OpenPGP: %1").arg(recipientRow.modelData)
                            textFormat: Text.PlainText
                            font.family: Kirigami.Theme.fixedWidthFont.family
                            wrapMode: Text.WrapAnywhere
                            color: Kirigami.Theme.disabledTextColor
                        }
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.hasHistory
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
                        readonly property string sectionTitle: qsTranslate("SecretDetail", "Actividad")
                        text: ""
                        display: Controls.AbstractButton.IconOnly
                        focusPolicy: Qt.StrongFocus
                        checkable: true
                        checked: root.historyOpen
                        Accessible.name: qsTranslate("SecretDetail", "Actividad")
                        Accessible.description: root.historyOpen ? qsTranslate("SecretDetail", "Ocultar los %1 eventos").arg(root.historyItems.length) : qsTranslate("SecretDetail", "Mostrar los %1 eventos").arg(root.historyItems.length)
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
                                text: root.historyItems.length === 1 ? qsTranslate("SecretDetail", "1 evento") : qsTranslate("SecretDetail", "%1 eventos").arg(root.historyItems.length)
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
                        text: root.historyItems.length ? qsTranslate("SecretDetail", "%1 · %2").arg(root.eventText(root.historyItems[0].kind)).arg(root.formatEventTime(root.historyItems[0].at)) : ""
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
                                readonly property bool startsDay: index === 0 || root.eventDate(modelData.at) !== root.eventDate(root.visibleHistoryItems[index - 1].at)

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
                            text: root.historyExpanded ? qsTranslate("SecretDetail", "Ver solo los últimos 5") : qsTranslate("SecretDetail", "Ver los últimos %1").arg(Math.min(20, root.historyItems.length))
                            flat: true
                            onClicked: root.historyExpanded = !root.historyExpanded
                        }
                        Controls.Button {
                            text: qsTranslate("SecretDetail", "Mostrar todos los eventos…")
                            icon.name: "document-open"
                            flat: true
                            Controls.ToolTip.visible: hovered || visualFocus
                            Controls.ToolTip.text: qsTranslate("SecretDetail", "Generar y abrir un archivo de texto con los eventos conservados")
                            onClicked: root.activityExportRequested()
                        }
                    }
                }
            }

            Controls.Frame {
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: root.hasHistory
                padding: Kirigami.Units.largeSpacing
                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.smallSpacing
                    Controls.Label {
                        Layout.fillWidth: true
                        text: qsTranslate("SecretDetail", "Versiones anteriores")
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: root.versions.length === 0
                        text: qsTranslate("SecretDetail", "Aún no hay versiones anteriores. Se guardan cuando actualizas los datos de esta credencial.")
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
                                    text: qsTranslate("SecretDetail", "Versión anterior")
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
                                id: versionOpenButton
                                text: qsTranslate("SecretDetail", "Consultar")
                                Accessible.name: qsTranslate("SecretDetail", "Consultar versión anterior del %1").arg(root.formatEventTime(versionRow.modelData.at))
                                onClicked: {
                                    root.versionReturnItem = versionOpenButton;
                                    root.versionOpenRequested(versionRow.modelData.index);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    VersionDialog {
        id: versionDialog
        versionDetail: root.versionDetail
        versions: root.versions
        currentTitle: root.detail?.title || ""
        onRevealRequested: root.versionRevealRequested()
        onCopyRequested: function (field) {
            root.versionCopyRequested(field);
        }
        onCopyUrlRequested: function (index) {
            root.versionCopyUrlRequested(index);
        }
        onOpenVersionRequested: function (index) {
            root.versionOpenRequested(index);
        }
        onVersionClosed: {
            root.versionCloseRequested();
            Qt.callLater(function () {
                if (root.versionReturnItem)
                    root.versionReturnItem.forceActiveFocus();
            });
        }
    }
}
