pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.ApplicationWindow {
    id: dialog
    required property var controller
    readonly property bool opened: visible
    readonly property bool available: controller.available

    signal copyRequested(string code)

    property bool networkOptionsVisible: false
    property bool showInvitationCode: false
    property bool reconnectFlow: false
    property string expandedDeviceId: ""
    property string aliasTargetId: ""
    property string aliasDraft: ""
    property string forgetTargetId: ""
    property string forgetTargetName: ""
    property bool resetRequested: false
    readonly property var snapshot: parseSnapshot(controller.devicesSnapshot)
    readonly property var devices: snapshot.devices
    readonly property var sasRows: parseSasRows(controller.sasEmojis)
    readonly property bool sasReady: sasRows.length === 7
    readonly property bool waitingForPeer: controller.bootstrap.length > 0 && controller.statusCode === "waiting_peer"
    readonly property bool comparing: controller.awaiting || controller.statusCode === "confirming"
    readonly property string statusHint: hintForStatus(controller.statusCode)
    readonly property bool statusIsProblem: ["expired", "rejected", "profile_error", "keystore_error", "authentication_error", "transport_error", "network_policy_blocked", "network_policy_changed"].includes(controller.statusCode)
    readonly property bool canPair: controller.available && !controller.busy && !controller.sessionActive && !controller.awaiting

    flags: Qt.Dialog | Qt.WindowTitleHint | Qt.WindowSystemMenuHint | Qt.WindowCloseButtonHint | Qt.WindowMaximizeButtonHint
    modality: Qt.WindowModal
    width: Kirigami.Units.gridUnit * 34
    height: Kirigami.Units.gridUnit * 36
    minimumWidth: Kirigami.Units.gridUnit * 24
    minimumHeight: Kirigami.Units.gridUnit * 22
    title: qsTr("Dispositivos")

    function open() {
        if (!visible) {
            x = screen.virtualX + Math.round((screen.width - width) / 2);
            y = screen.virtualY + Math.round((screen.height - height) / 2);
        }
        show();
        raise();
        requestActivate();
    }

    function parseSnapshot(value) {
        try {
            const parsed = JSON.parse(value || "{}");
            return {
                revision: Number.isSafeInteger(parsed.revision) ? parsed.revision : 0,
                devices: Array.isArray(parsed.devices) ? parsed.devices.filter(device => device && typeof device === "object") : []
            };
        } catch (error) {
            return {
                revision: 0,
                devices: []
            };
        }
    }

    function parseSasRows(value) {
        try {
            const parsed = JSON.parse(value || "[]");
            return Array.isArray(parsed) ? parsed.filter(row => row && typeof row.emoji === "string" && typeof row.name === "string") : [];
        } catch (error) {
            return [];
        }
    }

    function shortId(value) {
        const id = String(value || "");
        return id.length > 12 ? id.slice(0, 12) + "…" : id;
    }

    function deviceName(device) {
        return String(device.alias || "").length > 0 ? String(device.alias) : String(device.displayName || qsTr("Dispositivo"));
    }

    function connectionLabel(state) {
        switch (state) {
        case "connected":
            return qsTr("Conectado");
        case "connecting":
            return qsTr("Conectando");
        case "reconnecting":
            return qsTr("Reconectando");
        case "failed":
            return qsTr("Conexión interrumpida");
        default:
            return qsTr("Desconectado");
        }
    }

    function connectionColor(state) {
        if (state === "connected")
            return Kirigami.Theme.positiveTextColor;
        if (state === "failed")
            return Kirigami.Theme.negativeTextColor;
        if (state === "connecting" || state === "reconnecting")
            return Kirigami.Theme.linkColor;
        return Kirigami.Theme.disabledTextColor;
    }

    function routeLine(device) {
        const family = ({
                ipv4: "IPv4",
                ipv6: "IPv6",
                mixed: "IPv4 / IPv6"
            })[device.addressFamily] || "";
        const route = String(device.routeLabel || qsTr("Comprobando ruta…"));
        return family ? route + " · " + family : route;
    }

    function hintForStatus(code) {
        switch (code) {
        case "waiting_peer":
            return dialog.reconnectFlow ? qsTr("Escanea el QR y elige Reconectar en Android. El código de conexión caduca en 5 minutos.") : qsTr("Escanea el QR y elige Vincular dispositivo en Android. El código de conexión caduca en 5 minutos.");
        case "awaiting_sas":
            return qsTr("Compara los siete emojis y sus nombres con Android. Confirma solo si todos coinciden en ambas pantallas.");
        case "confirming":
            return qsTr("La otra pantalla está confirmando la comparación.");
        case "reconnecting":
            return qsTr("Intentando recuperar la sesión autenticada con este dispositivo.");
        case "closing":
            return qsTr("Cerrando el enlace. El dispositivo seguirá emparejado.");
        case "expired":
            return qsTr("La invitación caducó. Genera una nueva para volver a intentarlo.");
        case "trust_error":
            return qsTr("No se cambió la confianza guardada. Comprueba el perfil y vuelve a cargar los dispositivos.");
        case "profile_error":
            return qsTr("El perfil local de pruebas no está disponible. No se creó una identidad sustituta.");
        case "keystore_error":
            return qsTr("El llavero del sistema no pudo abrir la identidad de pruebas. No se creó una identidad sustituta.");
        case "authentication_error":
            return qsTr("No se autenticó el dispositivo. La confianza no se amplió; genera un código nuevo para reintentar.");
        case "rejected":
            return qsTr("La vinculación fue rechazada y no se concedió confianza.");
        case "cancelled":
            return qsTr("La operación se canceló. La confianza que ya estaba guardada se conserva.");
        case "transport_error":
            return dialog.controller.internetEnabled ? qsTr("Comprueba el acceso a Internet y vuelve a intentarlo.") : qsTr("Comprueba que ambos dispositivos estén en la misma red y vuelve a intentarlo.");
        case "disconnected":
            return qsTr("La sesión está desconectada. La confianza guardada se conserva; puedes reconectar el dispositivo.");
        case "connected":
            return qsTr("La sesión autenticada está activa. Cerrar esta ventana no la desconecta.");
        default:
            return "";
        }
    }

    function startPairing() {
        if (!canPair)
            return;
        reconnectFlow = false;
        controller.startPairing();
    }

    function openAliasEditor(device) {
        aliasTargetId = String(device.deviceId || "");
        aliasDraft = String(device.alias || "");
        aliasDialog.open();
        aliasField.forceActiveFocus();
    }

    function saveAlias() {
        if (!aliasTargetId || controller.aliasBusy)
            return;
        controller.saveDeviceAlias(aliasTargetId, aliasDraft);
    }

    onVisibleChanged: {
        if (visible) {
            controller.loadNetworkAddresses();
            controller.loadDevices();
            if (controller.awaiting)
                Qt.callLater(function () {
                    noMatchesButton.forceActiveFocus();
                });
        } else {
            aliasDialog.close();
            deleteDialog.close();
            // Cancels an unfinished pairing and clears QR/SAS; an active session survives.
            controller.releaseUi();
        }
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        enabled: dialog.visible && !aliasDialog.opened && !deleteDialog.opened && !dialog.controller.maintenanceBusy
        onActivated: dialog.close()
    }

    Connections {
        target: dialog.controller
        function onAliasSaved(deviceId) {
            if (String(deviceId) === dialog.aliasTargetId)
                aliasDialog.close();
        }
        function onAwaitingChanged() {
            if (dialog.controller.awaiting)
                Qt.callLater(function () {
                    noMatchesButton.forceActiveFocus();
                });
        }
        function onBootstrapChanged() {
            if (dialog.controller.bootstrap.length === 0)
                dialog.showInvitationCode = false;
        }
    }

    header: PanelHeader {
        headerHeight: Kirigami.Units.gridUnit * 3
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon {
                source: "network-connect"
                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                implicitHeight: Kirigami.Units.iconSizes.smallMedium
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 4
                    text: qsTr("Tus dispositivos")
                    elide: Text.ElideRight
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Conexión de pruebas · Sin sincronización de bóveda")
                    textFormat: Text.PlainText
                    color: Kirigami.Theme.disabledTextColor
                    elide: Text.ElideRight
                }
            }
            Controls.ToolButton {
                id: moreButton
                icon.name: "overflow-menu"
                text: qsTr("Más opciones")
                display: Controls.AbstractButton.IconOnly
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: text
                onClicked: moreMenu.popup(moreButton, 0, moreButton.height)
                Controls.Menu {
                    id: moreMenu
                    Controls.MenuItem {
                        text: qsTr("Opciones de conexión")
                        icon.name: "network-wired"
                        checkable: true
                        checked: dialog.networkOptionsVisible
                        onTriggered: dialog.networkOptionsVisible = checked
                    }
                    Controls.MenuItem {
                        text: qsTr("Actualizar dispositivos y redes")
                        icon.name: "view-refresh"
                        enabled: !dialog.controller.busy && !dialog.controller.awaiting
                        onTriggered: {
                            dialog.controller.loadDevices();
                            dialog.controller.loadNetworkAddresses();
                        }
                    }
                    Controls.MenuSeparator {}
                    Controls.MenuItem {
                        text: qsTr("Reiniciar vinculación…")
                        icon.name: "edit-clear-all"
                        enabled: dialog.controller.available && !dialog.controller.busy && !dialog.controller.aliasBusy
                        onTriggered: {
                            dialog.resetRequested = true;
                            deleteDialog.open();
                        }
                    }
                }
            }
        }
    }

    Controls.ScrollView {
        id: pageScroll
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: pageScroll.availableWidth
            spacing: Kirigami.Units.smallSpacing

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: dialog.statusIsProblem
                type: ["expired", "rejected"].includes(dialog.controller.statusCode) ? Kirigami.MessageType.Warning : Kirigami.MessageType.Error
                text: dialog.controller.status + (dialog.statusHint.length > 0 ? "\n" + dialog.statusHint : "")
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: dialog.controller.aliasError.length > 0 && !aliasDialog.opened
                type: Kirigami.MessageType.Warning
                text: dialog.controller.aliasError
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: dialog.controller.sessionActive && !dialog.comparing
                type: Kirigami.MessageType.Information
                text: qsTr("Hay una sesión activa con %1. Desconéctalo antes de vincular o reconectar otro dispositivo.").arg(dialog.shortId(dialog.controller.activeDeviceId))
            }

            // Invitation: QR first, the long code only on request.
            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.waitingForPeer
                text: dialog.reconnectFlow ? qsTr("Reconectar dispositivo") : qsTr("Vincular dispositivo")
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.waitingForPeer
                spacing: Kirigami.Units.gridUnit
                Rectangle {
                    Layout.alignment: Qt.AlignTop
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 11
                    Layout.preferredHeight: Layout.preferredWidth
                    color: "white"
                    radius: Kirigami.Units.cornerRadius
                    visible: dialog.controller.bootstrapQr.length > 0
                    Image {
                        anchors.fill: parent
                        anchors.margins: Kirigami.Units.smallSpacing
                        source: dialog.controller.bootstrapQr
                        fillMode: Image.PreserveAspectFit
                        cache: false
                        asynchronous: true
                        smooth: false
                        Accessible.name: qsTr("Código QR de vinculación local")
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    spacing: Kirigami.Units.largeSpacing
                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.statusHint
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.controller.internetEnabled ? qsTr("Ambos dispositivos necesitan acceso a Internet.") : (dialog.controller.lanEnabled ? qsTr("Ambos dispositivos deben estar en la misma red local.") : qsTr("Modo emulador: solo para Android en esta computadora."))
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.ToolButton {
                        text: dialog.showInvitationCode ? qsTr("Ocultar código") : qsTr("Usar código en lugar de QR")
                        icon.name: "edit-paste"
                        onClicked: dialog.showInvitationCode = !dialog.showInvitationCode
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.waitingForPeer && dialog.showInvitationCode
                Controls.TextField {
                    Layout.fillWidth: true
                    text: dialog.controller.bootstrap
                    readOnly: true
                    selectByMouse: true
                    font.family: "monospace"
                    Accessible.name: qsTr("Código de invitación de un solo uso")
                }
                Controls.Button {
                    text: qsTr("Copiar")
                    icon.name: "edit-copy"
                    onClicked: dialog.copyRequested(dialog.controller.bootstrap)
                }
            }

            // SAS comparison: the human decision, never a default action.
            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.comparing
                text: qsTr("Compara los siete elementos")
            }
            Controls.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.comparing
                text: dialog.statusHint
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.comparing
                columns: width >= Kirigami.Units.gridUnit * 30 ? 7 : width >= Kirigami.Units.gridUnit * 16 ? 4 : 2
                columnSpacing: Kirigami.Units.smallSpacing
                rowSpacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: dialog.sasRows
                    delegate: Rectangle {
                        id: sasCell
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.minimumWidth: Kirigami.Units.gridUnit * 4
                        implicitHeight: sasColumn.implicitHeight + Kirigami.Units.largeSpacing * 2
                        radius: Kirigami.Units.cornerRadius
                        color: Kirigami.Theme.alternateBackgroundColor
                        ColumnLayout {
                            id: sasColumn
                            anchors.centerIn: parent
                            width: parent.width - Kirigami.Units.smallSpacing * 2
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Label {
                                Layout.fillWidth: true
                                text: String(sasCell.modelData.emoji || "")
                                font.pixelSize: Kirigami.Units.gridUnit * 1.6
                                horizontalAlignment: Text.AlignHCenter
                                textFormat: Text.PlainText
                                Accessible.name: qsTr("Elemento %1: %2").arg(sasCell.index + 1).arg(String(sasCell.modelData.emoji || ""))
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: String(sasCell.modelData.name || "")
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                                horizontalAlignment: Text.AlignHCenter
                            }
                        }
                    }
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.comparing
                text: dialog.sasReady ? qsTr("Si un solo emoji o nombre no coincide, rechaza la vinculación.") : qsTr("No se pudo mostrar la comparación completa. Rechaza la vinculación y vuelve a intentarlo.")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.controller.awaiting
                Item {
                    Layout.fillWidth: true
                }
                Controls.Button {
                    id: noMatchesButton
                    text: qsTr("No coinciden")
                    icon.name: "dialog-cancel"
                    onClicked: dialog.controller.decide(false)
                }
                Controls.Button {
                    text: qsTr("Los 7 coinciden")
                    icon.name: "dialog-ok-apply"
                    enabled: dialog.sasReady
                    onClicked: dialog.controller.decide(true)
                }
            }

            // Connection options, folded by default.
            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.networkOptionsVisible
                text: qsTr("Opciones de conexión")
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.networkOptionsVisible
                spacing: Kirigami.Units.smallSpacing

                Controls.CheckBox {
                    Layout.fillWidth: true
                    text: qsTr("Conectar por Internet")
                    checked: dialog.controller.internetEnabled
                    enabled: !dialog.controller.busy
                    onToggled: dialog.controller.configureInternet(checked, relayField.text)
                }
                Controls.TextField {
                    id: relayField
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.gridUnit
                    visible: dialog.controller.internetEnabled
                    enabled: !dialog.controller.busy
                    text: dialog.controller.relayOrigin
                    placeholderText: qsTr("Servidor de conexión (wss://…)")
                    Accessible.name: qsTr("Servidor de conexión por Internet")
                    onEditingFinished: dialog.controller.configureInternet(true, text)
                }
                Controls.CheckBox {
                    Layout.fillWidth: true
                    text: qsTr("Exigir VPN para conexiones por Internet")
                    checked: dialog.controller.requireVpn
                    enabled: !dialog.controller.maintenanceBusy
                    onToggled: dialog.controller.configurePrivacy(checked, dialog.controller.allowLanException, dialog.controller.relayOnly)
                }
                Controls.CheckBox {
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.gridUnit
                    visible: dialog.controller.requireVpn
                    text: qsTr("Permitir una excepción explícita para recuperación LAN")
                    checked: dialog.controller.allowLanException
                    enabled: !dialog.controller.maintenanceBusy
                    onToggled: dialog.controller.configurePrivacy(dialog.controller.requireVpn, checked, dialog.controller.relayOnly)
                }
                Controls.CheckBox {
                    Layout.fillWidth: true
                    text: qsTr("Usar solo retransmisión TURN")
                    checked: dialog.controller.relayOnly
                    enabled: !dialog.controller.maintenanceBusy
                    onToggled: dialog.controller.configurePrivacy(dialog.controller.requireVpn, dialog.controller.allowLanException, checked)
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: dialog.controller.vpnStatus == 1
                    type: Kirigami.MessageType.Warning
                    text: qsTr("Desktop no puede verificar que DNS, señalización, STUN, TURN y UDP estén cubiertos por la VPN. Exigir VPN bloquea las conexiones por Internet; la recuperación LAN también queda bloqueada hasta activar su excepción.")
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: dialog.controller.relayOnly
                    type: Kirigami.MessageType.Information
                    text: qsTr("Solo retransmisión elimina STUN y candidatos directos, exige un servidor TURN válido y bloquea las conexiones LAN directas.")
                }
                Controls.CheckBox {
                    Layout.fillWidth: true
                    visible: !dialog.controller.internetEnabled
                    text: qsTr("Usar conexión por red local")
                    checked: dialog.controller.lanEnabled
                    enabled: !dialog.controller.busy
                    onToggled: dialog.controller.configureNetwork(checked, dialog.controller.networkIndex)
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.gridUnit
                    visible: dialog.controller.lanEnabled && !dialog.controller.internetEnabled
                    Controls.ComboBox {
                        Layout.fillWidth: true
                        enabled: !dialog.controller.busy
                        model: dialog.controller.networkAddresses
                        currentIndex: dialog.controller.networkIndex
                        Accessible.name: qsTr("Interfaz de red local de Desktop")
                        onActivated: index => dialog.controller.configureNetwork(true, index)
                    }
                    Controls.ToolButton {
                        icon.name: "view-refresh"
                        text: qsTr("Actualizar redes")
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.visible: hovered
                        Controls.ToolTip.text: text
                        enabled: !dialog.controller.busy && !dialog.waitingForPeer && !dialog.controller.awaiting
                        onClicked: dialog.controller.loadNetworkAddresses()
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: dialog.controller.lanEnabled && !dialog.controller.internetEnabled && dialog.controller.networkIndex < 0
                    text: qsTr("No hay una dirección local disponible. Actualiza las redes o usa el modo emulador.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !dialog.controller.lanEnabled && !dialog.controller.internetEnabled
                    text: qsTr("Modo emulador avanzado: solo funciona cuando Android se ejecuta en esta computadora.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            // Authorized devices as list rows, like the main window's panes.
            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.devices.length > 0 || dialog.controller.busy
                text: qsTr("Dispositivos autorizados")
            }
            Controls.Label {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: dialog.devices.length === 0 && dialog.controller.busy
                text: qsTr("Cargando dispositivos autorizados…")
                textFormat: Text.PlainText
                color: Kirigami.Theme.disabledTextColor
            }
            Repeater {
                model: dialog.devices
                delegate: ColumnLayout {
                    id: deviceRow
                    required property int index
                    required property var modelData
                    readonly property string deviceId: String(modelData.deviceId || "")
                    readonly property string connection: String(modelData.connection || "")
                    readonly property bool expanded: dialog.expandedDeviceId === deviceId
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Kirigami.Units.largeSpacing
                        Layout.rightMargin: Kirigami.Units.smallSpacing
                        Layout.topMargin: Kirigami.Units.smallSpacing
                        spacing: Kirigami.Units.largeSpacing
                        Kirigami.Icon {
                            source: "smartphone"
                            fallback: "computer"
                            implicitWidth: Kirigami.Units.iconSizes.medium
                            implicitHeight: Kirigami.Units.iconSizes.medium
                            Accessible.ignored: true
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            Controls.Label {
                                Layout.fillWidth: true
                                text: dialog.deviceName(deviceRow.modelData)
                                textFormat: Text.PlainText
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            RowLayout {
                                spacing: Kirigami.Units.smallSpacing
                                Controls.Label {
                                    text: deviceRow.modelData.trust === "trusted" ? qsTr("Emparejado") : qsTr("Confianza no disponible")
                                    textFormat: Text.PlainText
                                    color: Kirigami.Theme.disabledTextColor
                                    Accessible.name: qsTr("Estado de confianza: %1").arg(text)
                                }
                                Controls.Label {
                                    text: "·"
                                    color: Kirigami.Theme.disabledTextColor
                                    Accessible.ignored: true
                                }
                                Controls.Label {
                                    text: dialog.connectionLabel(deviceRow.connection)
                                    color: dialog.connectionColor(deviceRow.connection)
                                    textFormat: Text.PlainText
                                    Accessible.name: qsTr("Estado de conexión: %1").arg(text)
                                }
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                visible: deviceRow.connection === "connected"
                                text: dialog.routeLine(deviceRow.modelData)
                                textFormat: Text.PlainText
                                color: Kirigami.Theme.disabledTextColor
                                elide: Text.ElideRight
                                Accessible.name: qsTr("Ruta de conexión: %1").arg(text)
                            }
                        }
                        Controls.Button {
                            visible: !deviceRow.modelData.isActive
                            text: qsTr("Reconectar")
                            icon.name: "network-connect"
                            enabled: !dialog.controller.busy && !dialog.controller.sessionActive && deviceRow.modelData.trust === "trusted"
                            onClicked: {
                                dialog.reconnectFlow = true;
                                dialog.controller.selectDevice(deviceRow.index);
                                dialog.controller.startReconnect();
                            }
                        }
                        Controls.Button {
                            visible: !!deviceRow.modelData.isActive
                            text: qsTr("Desconectar")
                            icon.name: "network-disconnect"
                            enabled: dialog.controller.statusCode !== "closing"
                            onClicked: dialog.controller.disconnectPeer(deviceRow.deviceId)
                        }
                        Controls.ToolButton {
                            id: deviceMenuButton
                            icon.name: "overflow-menu"
                            text: qsTr("Más acciones para %1").arg(dialog.deviceName(deviceRow.modelData))
                            display: Controls.AbstractButton.IconOnly
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.text: qsTr("Más acciones")
                            onClicked: deviceMenu.popup(deviceMenuButton, 0, deviceMenuButton.height)
                            Controls.Menu {
                                id: deviceMenu
                                Controls.MenuItem {
                                    text: qsTr("Cambiar nombre…")
                                    icon.name: "edit-rename"
                                    enabled: !dialog.controller.aliasBusy && !dialog.controller.busy
                                    onTriggered: dialog.openAliasEditor(deviceRow.modelData)
                                }
                                Controls.MenuItem {
                                    text: deviceRow.expanded ? qsTr("Ocultar identificador") : qsTr("Mostrar identificador")
                                    icon.name: "dialog-information"
                                    onTriggered: {
                                        dialog.expandedDeviceId = deviceRow.expanded ? "" : deviceRow.deviceId;
                                        dialog.controller.selectDevice(deviceRow.index);
                                    }
                                }
                                Controls.MenuSeparator {}
                                Controls.MenuItem {
                                    text: qsTr("Olvidar dispositivo…")
                                    icon.name: "edit-delete"
                                    enabled: !dialog.controller.busy && !dialog.controller.aliasBusy
                                    onTriggered: {
                                        dialog.forgetTargetId = deviceRow.deviceId;
                                        dialog.forgetTargetName = dialog.deviceName(deviceRow.modelData);
                                        dialog.resetRequested = false;
                                        deleteDialog.open();
                                    }
                                }
                            }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Kirigami.Units.largeSpacing + Kirigami.Units.iconSizes.medium + Kirigami.Units.largeSpacing
                        Layout.rightMargin: Kirigami.Units.smallSpacing
                        visible: deviceRow.expanded
                        Controls.Label {
                            Layout.fillWidth: true
                            text: deviceRow.deviceId
                            textFormat: Text.PlainText
                            font.family: "monospace"
                            wrapMode: Text.WrapAnywhere
                            color: Kirigami.Theme.disabledTextColor
                            Accessible.name: qsTr("Identificador completo del dispositivo")
                        }
                        Controls.ToolButton {
                            text: qsTr("Copiar")
                            icon.name: "edit-copy"
                            onClicked: dialog.copyRequested(deviceRow.deviceId)
                        }
                    }
                    Kirigami.Separator {
                        Layout.fillWidth: true
                        Layout.topMargin: Kirigami.Units.smallSpacing
                    }
                }
            }

            Kirigami.PlaceholderMessage {
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.gridUnit * 3
                Layout.leftMargin: Kirigami.Units.gridUnit * 2
                Layout.rightMargin: Kirigami.Units.gridUnit * 2
                visible: dialog.devices.length === 0 && !dialog.controller.busy && !dialog.waitingForPeer && !dialog.comparing
                icon.name: "network-connect"
                text: qsTr("Sin dispositivos vinculados")
                explanation: qsTr("Vincula KryptoPass para Android para mantener una conexión segura con este equipo.")
                helpfulAction: Kirigami.Action {
                    text: qsTr("Vincular dispositivo")
                    icon.name: "list-add"
                    enabled: dialog.canPair
                    onTriggered: dialog.startPairing()
                }
            }

            Item {
                Layout.preferredHeight: Kirigami.Units.largeSpacing
            }
        }
    }

    footer: PanelFooter {
        headerHeight: Kirigami.Units.gridUnit * 2.5
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing
            Controls.BusyIndicator {
                running: dialog.controller.busy
                visible: running
                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                implicitHeight: Kirigami.Units.iconSizes.smallMedium
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !dialog.statusIsProblem
                text: dialog.controller.status
                textFormat: Text.PlainText
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
            }
            Item {
                Layout.fillWidth: true
                visible: dialog.statusIsProblem
            }
            Controls.Button {
                text: qsTr("Vincular dispositivo")
                icon.name: "list-add"
                enabled: dialog.canPair
                onClicked: dialog.startPairing()
            }
            Controls.Button {
                enabled: !dialog.controller.maintenanceBusy
                text: dialog.controller.busy ? qsTr("Cancelar") : qsTr("Cerrar")
                icon.name: dialog.controller.busy ? "dialog-cancel" : "dialog-close"
                onClicked: {
                    if (dialog.controller.busy)
                        dialog.controller.cancel();
                    else
                        dialog.close();
                }
            }
        }
    }

    Controls.Dialog {
        id: deleteDialog
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: dialog.resetRequested ? qsTr("¿Reiniciar vinculación?") : qsTr("¿Olvidar dispositivo?")
        width: Math.min(Kirigami.Units.gridUnit * 24, dialog.width - Kirigami.Units.gridUnit * 2)
        standardButtons: Controls.Dialog.NoButton
        contentItem: Controls.Label {
            text: dialog.resetRequested ? qsTr("Se cerrará la conexión y se borrarán la identidad privada de este perfil de vinculación, sus autorizaciones y nombres locales. La bóveda y las claves OpenPGP se conservan. Para volver a vincular, compara un SAS nuevo.") : qsTr("Se cerrará la conexión y se eliminarán la autorización y el nombre local de «%1» en este equipo. También olvida este equipo en Android antes de volver a emparejarlos. La bóveda y OpenPGP se conservan.").arg(dialog.forgetTargetName)
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
        }
        footer: Controls.DialogButtonBox {
            Controls.Button {
                text: qsTr("Cancelar")
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
            }
            Controls.Button {
                text: dialog.resetRequested ? qsTr("Reiniciar vinculación") : qsTr("Olvidar dispositivo")
                icon.name: "edit-delete"
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.DestructiveRole
                onClicked: {
                    if (dialog.resetRequested)
                        dialog.controller.resetPairingProfile();
                    else
                        dialog.controller.forgetDevice(dialog.forgetTargetId);
                    deleteDialog.close();
                }
            }
            onRejected: deleteDialog.close()
        }
    }

    Controls.Dialog {
        id: aliasDialog
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: qsTr("Nombre del dispositivo")
        width: Math.min(Kirigami.Units.gridUnit * 22, dialog.width - Kirigami.Units.gridUnit * 2)
        standardButtons: Controls.Dialog.NoButton

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.mediumSpacing
            Controls.TextField {
                id: aliasField
                Layout.fillWidth: true
                text: dialog.aliasDraft
                maximumLength: 64
                enabled: !dialog.controller.aliasBusy
                selectByMouse: true
                placeholderText: qsTr("Nombre del dispositivo")
                Accessible.name: qsTr("Nombre local del dispositivo")
                onTextEdited: dialog.aliasDraft = text
                onAccepted: {
                    dialog.aliasDraft = text;
                    dialog.saveAlias();
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Solo se guarda en este equipo.")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Kirigami.Theme.disabledTextColor
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.controller.aliasError.length > 0
                text: dialog.controller.aliasError
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Kirigami.Theme.negativeTextColor
            }
        }
        footer: Controls.DialogButtonBox {
            Controls.Button {
                text: qsTr("Quitar nombre")
                enabled: !dialog.controller.aliasBusy
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ResetRole
                onClicked: {
                    dialog.aliasDraft = "";
                    aliasField.text = "";
                    dialog.saveAlias();
                }
            }
            Controls.Button {
                text: qsTr("Cancelar")
                enabled: !dialog.controller.aliasBusy
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
                onClicked: aliasDialog.close()
            }
            Controls.Button {
                text: dialog.controller.aliasBusy ? qsTr("Guardando…") : qsTr("Guardar")
                enabled: !dialog.controller.aliasBusy
                // Not AcceptRole: the dialog closes only after aliasSaved confirms the write.
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
                onClicked: {
                    dialog.aliasDraft = aliasField.text;
                    dialog.saveAlias();
                }
            }
        }
    }
}
