pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.ApplicationWindow {
    id: dialog
    objectName: "devicesWindow"
    required property var controller
    readonly property bool opened: visible
    readonly property bool available: controller.available

    signal copyRequested(string code)

    property bool networkOptionsVisible: false
    property bool showInvitationCode: false
    property bool reconnectFlow: false
    property string expandedDeviceId: ""
    property string syncExpandedDeviceId: ""
    // Which offer the last key transfer of this window carried, to label its status.
    property bool keyTransferVault: false
    property string aliasTargetId: ""
    property string aliasDraft: ""
    property string forgetTargetId: ""
    property string forgetTargetName: ""
    property bool resetRequested: false
    readonly property var snapshot: parseSnapshot(controller.devicesSnapshot)
    readonly property var devices: snapshot.devices
    readonly property var sasRows: parseSasRows(controller.sasEmojis)
    // Phone → Desktop vault-identity offer; opens its dialog while visible.
    readonly property var inboundTransfer: parseInboundTransfer(controller.inboundKeyTransfer)
    onInboundTransferChanged: {
        if (inboundTransfer.visible) {
            if (!inboundKeyDialog.opened)
                inboundKeyDialog.open();
        } else if (inboundKeyDialog.opened) {
            inboundKeyDialog.close();
        }
    }
    readonly property bool sasReady: sasRows.length === 7
    readonly property bool waitingForPeer: controller.bootstrap.length > 0 && controller.statusCode === "waiting_peer"
    // Any LAN path (pairing, recovery, emulator) is allowed by the privacy choices.
    readonly property bool lanAllowed: !controller.relayOnly && (!controller.requireVpn || controller.allowLanException)
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

    function parseInboundTransfer(value) {
        try {
            const parsed = JSON.parse(value || "{}");
            return parsed && typeof parsed === "object" ? parsed : ({ visible: false });
        } catch (error) {
            return ({ visible: false });
        }
    }

    function inboundPhaseLabel(transfer) {
        switch (transfer.phase) {
        case "assessing":
            return qsTr("Revisando la clave ofrecida…");
        case "waiting_decision":
            return qsTr("Esperando tu decisión");
        case "receiving":
            return qsTr("Recibiendo (%1 de %2 bytes)").arg(transfer.received || 0).arg(transfer.total || 0);
        case "importing":
            return qsTr("Importando en GnuPG");
        case "verifying":
            return qsTr("Comprobando el desafío");
        case "complete":
            return qsTr("Completado");
        case "rejected":
            return qsTr("Rechazada");
        case "cancelled":
            return qsTr("Cancelada por el teléfono");
        case "expired":
            return qsTr("Caducada");
        case "failed":
            return qsTr("Error");
        default:
            return "";
        }
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

    function keyTransferLabel(phase) {
        switch (phase) {
        case "waiting_acceptance":
            return qsTr("Esperando que aceptes en el teléfono");
        case "sending":
            return qsTr("Enviando tu clave");
        case "waiting_receipt":
            return qsTr("Esperando que el teléfono la guarde");
        case "complete":
            return qsTr("El teléfono guardó tu clave y comprobó que puede descifrar");
        default:
            return syntheticTransferLabel(phase);
        }
    }

    function syntheticTransferLabel(phase) {
        switch (phase) {
        case "ready":
            return qsTr("Prueba de transferencia disponible");
        case "waiting_acceptance":
            return qsTr("Esperando que Android acepte");
        case "sending":
            return qsTr("Enviando clave de prueba");
        case "waiting_receipt":
            return qsTr("Esperando confirmación de guardado");
        case "complete":
            return qsTr("Android confirmó el guardado y el desafío");
        case "rejected":
            return qsTr("Android rechazó la oferta");
        case "cancelled":
            return qsTr("Transferencia cancelada");
        case "expired":
            return qsTr("La transferencia caducó");
        case "uncertain":
            return qsTr("Conexión perdida; el resultado es incierto");
        case "failed":
            return qsTr("La transferencia falló");
        default:
            return qsTr("Esta sesión no admite la prueba");
        }
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
            return qsTr("Vuelve a cargar los dispositivos. Si el problema continúa, consulta el registro de la aplicación.");
        case "keystore_error":
            return qsTr("Comprueba que el llavero del sistema esté disponible y vuelve a intentarlo.");
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
                    text: qsTr("Vincula y conecta tus dispositivos")
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
                        text: dialog.controller.internetEnabled && dialog.controller.lanEnabled ? qsTr("El teléfono prueba primero la red local y, si no llega, usa Internet.") : dialog.controller.internetEnabled ? qsTr("Ambos dispositivos necesitan acceso a Internet.") : (dialog.controller.lanEnabled ? qsTr("Ambos dispositivos deben estar en la misma red local.") : qsTr("Modo emulador: solo para Android en esta computadora."))
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

            // Connection options, folded by default. Choices that cannot work
            // together are disabled here; the controller refuses them as well.
            // Each control restores its binding after a click so later backend
            // updates (normalization, VPN state) stay visible.
            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.networkOptionsVisible
                text: qsTr("Privacidad de la conexión")
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.networkOptionsVisible
                spacing: Kirigami.Units.smallSpacing

                Controls.RadioButton {
                    id: privacyAutomatic
                    Layout.fillWidth: true
                    text: qsTr("Automática (recomendada)")
                    checked: !dialog.controller.requireVpn
                    enabled: !dialog.controller.maintenanceBusy
                    onToggled: {
                        if (checked)
                            dialog.controller.configurePrivacy(false, false, dialog.controller.relayOnly);
                        checked = Qt.binding(() => !dialog.controller.requireVpn);
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: privacyAutomatic.indicator.width + privacyAutomatic.spacing
                    text: qsTr("Usa la ruta que elija el sistema, incluida tu VPN si está conectada.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.RadioButton {
                    id: privacyVpn
                    Layout.fillWidth: true
                    text: qsTr("Solo a través de VPN")
                    checked: dialog.controller.requireVpn
                    enabled: !dialog.controller.maintenanceBusy
                    onToggled: {
                        if (checked)
                            dialog.controller.configurePrivacy(true, dialog.controller.allowLanException, dialog.controller.relayOnly);
                        checked = Qt.binding(() => dialog.controller.requireVpn);
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: privacyVpn.indicator.width + privacyVpn.spacing
                    text: qsTr("Internet solo se usa mientras el sistema lo envía por una VPN. Si la VPN se desconecta, la conexión se pausa.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: dialog.controller.requireVpn
                    type: dialog.controller.vpnStatus === 2 ? Kirigami.MessageType.Positive : Kirigami.MessageType.Warning
                    text: {
                        switch (dialog.controller.vpnStatus) {
                        case 2:
                            return qsTr("VPN conectada: la conexión a Internet pasa por ella.");
                        case 3:
                            return qsTr("No hay una VPN conectada. Las conexiones por Internet están en pausa hasta que la conectes.");
                        default:
                            return qsTr("Este sistema no permite comprobar la VPN, así que las conexiones por Internet están en pausa.");
                        }
                    }
                }
                Controls.CheckBox {
                    Layout.fillWidth: true
                    Layout.leftMargin: privacyVpn.indicator.width + privacyVpn.spacing
                    visible: dialog.controller.requireVpn
                    text: qsTr("Permitir también la red local")
                    checked: dialog.controller.allowLanException && !dialog.controller.relayOnly
                    enabled: !dialog.controller.relayOnly && !dialog.controller.maintenanceBusy
                    onToggled: {
                        dialog.controller.configurePrivacy(true, checked, dialog.controller.relayOnly);
                        checked = Qt.binding(() => dialog.controller.allowLanException && !dialog.controller.relayOnly);
                    }
                }
                Controls.CheckBox {
                    id: hideAddress
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    text: qsTr("Ocultar mi dirección IP al otro dispositivo")
                    checked: dialog.controller.relayOnly
                    enabled: !dialog.controller.maintenanceBusy
                    onToggled: {
                        dialog.controller.configurePrivacy(dialog.controller.requireVpn, dialog.controller.allowLanException, checked);
                        checked = Qt.binding(() => dialog.controller.relayOnly);
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: hideAddress.indicator.width + hideAddress.spacing
                    text: qsTr("Todo pasa por el servidor de retransmisión. Es algo más lento y desactiva la red local.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.networkOptionsVisible
                text: qsTr("Cómo emparejar un dispositivo nuevo")
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.networkOptionsVisible
                spacing: Kirigami.Units.smallSpacing

                Controls.RadioButton {
                    id: pairByAuto
                    Layout.fillWidth: true
                    text: qsTr("Automático: red local y, si no llega, Internet")
                    checked: dialog.controller.internetEnabled && dialog.controller.lanEnabled
                    enabled: !dialog.controller.busy && dialog.lanAllowed
                    onToggled: {
                        if (checked) {
                            dialog.controller.configureNetwork(true, dialog.controller.networkIndex);
                            dialog.controller.configureInternet(true, relayField.text);
                        }
                        checked = Qt.binding(() => dialog.controller.internetEnabled && dialog.controller.lanEnabled);
                    }
                }
                Controls.RadioButton {
                    id: pairByInternet
                    Layout.fillWidth: true
                    text: qsTr("Solo por Internet")
                    checked: dialog.controller.internetEnabled && !dialog.controller.lanEnabled
                    enabled: !dialog.controller.busy
                    onToggled: {
                        if (checked) {
                            dialog.controller.configureNetwork(false, -1);
                            dialog.controller.configureInternet(true, relayField.text);
                        }
                        checked = Qt.binding(() => dialog.controller.internetEnabled && !dialog.controller.lanEnabled);
                    }
                }
                Controls.TextField {
                    id: relayField
                    Layout.fillWidth: true
                    Layout.leftMargin: pairByInternet.indicator.width + pairByInternet.spacing
                    visible: dialog.controller.internetEnabled
                    enabled: !dialog.controller.busy
                    text: dialog.controller.relayOrigin
                    placeholderText: qsTr("Servidor de conexión (wss://…)")
                    Accessible.name: qsTr("Servidor de conexión por Internet")
                    onEditingFinished: dialog.controller.configureInternet(true, text)
                }
                Controls.RadioButton {
                    id: pairByLan
                    Layout.fillWidth: true
                    text: qsTr("Solo red local")
                    checked: dialog.controller.lanEnabled && !dialog.controller.internetEnabled
                    enabled: !dialog.controller.busy && dialog.lanAllowed
                    onToggled: {
                        if (checked) {
                            dialog.controller.configureInternet(false, relayField.text);
                            dialog.controller.configureNetwork(true, dialog.controller.networkIndex);
                        }
                        checked = Qt.binding(() => dialog.controller.lanEnabled && !dialog.controller.internetEnabled);
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: pairByLan.indicator.width + pairByLan.spacing
                    visible: !dialog.lanAllowed
                    text: dialog.controller.relayOnly ? qsTr("No disponible mientras ocultas tu dirección IP.") : qsTr("No disponible: activa «Permitir también la red local» en Privacidad.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: pairByLan.indicator.width + pairByLan.spacing
                    visible: dialog.lanAllowed && dialog.controller.lanEnabled
                    Controls.ComboBox {
                        Layout.fillWidth: true
                        enabled: !dialog.controller.busy
                        model: dialog.controller.networkAddresses
                        currentIndex: dialog.controller.networkIndex
                        Accessible.name: qsTr("Red local de este equipo")
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
                    Layout.leftMargin: pairByLan.indicator.width + pairByLan.spacing
                    visible: dialog.lanAllowed && dialog.controller.lanEnabled && dialog.controller.networkIndex < 0
                    text: qsTr("No se encontró una red local. Conéctate a Wi-Fi o Ethernet y actualiza las redes.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.RadioButton {
                    id: pairByEmulator
                    Layout.fillWidth: true
                    text: qsTr("Emulador Android en este equipo (avanzado)")
                    checked: !dialog.controller.lanEnabled && !dialog.controller.internetEnabled
                    enabled: !dialog.controller.busy && dialog.lanAllowed
                    onToggled: {
                        if (checked) {
                            dialog.controller.configureInternet(false, relayField.text);
                            dialog.controller.configureNetwork(false, -1);
                        }
                        checked = Qt.binding(() => !dialog.controller.lanEnabled && !dialog.controller.internetEnabled);
                    }
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
                    readonly property string connectionGeneration: String(modelData.connectionGeneration || "")
                    readonly property string syntheticTransferPhase: String(modelData.syntheticTransferPhase || "unsupported")
                    readonly property int keyTransferVersion: Number(modelData.keyTransferVersion || 0)
                    readonly property bool expanded: dialog.expandedDeviceId === deviceId
                    readonly property bool syncExpanded: dialog.syncExpandedDeviceId === deviceId
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
                            enabled: !dialog.controller.busy && !dialog.controller.syncBusy && !dialog.controller.sessionActive && deviceRow.modelData.trust === "trusted"
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
                        visible: dialog.available && deviceRow.modelData.trust === "trusted"
                        Controls.ToolButton {
                            text: deviceRow.syncExpanded ? qsTr("Ocultar prueba de Sync") : qsTr("Prueba sintética de Sync")
                            icon.name: "network-connect"
                            enabled: !dialog.controller.syncBusy && !dialog.controller.busy
                            onClicked: {
                                dialog.syncExpandedDeviceId = deviceRow.syncExpanded ? "" : deviceRow.deviceId;
                                dialog.controller.selectDevice(deviceRow.index);
                            }
                        }
                    }
                    ColumnLayout {
                        readonly property bool vaultSyncOn: dialog.controller.vaultSyncPeers.indexOf(deviceRow.deviceId) >= 0
                        Layout.fillWidth: true
                        Layout.leftMargin: Kirigami.Units.largeSpacing + Kirigami.Units.iconSizes.medium + Kirigami.Units.largeSpacing
                        Layout.rightMargin: Kirigami.Units.smallSpacing
                        visible: dialog.available && dialog.controller.vaultKeyFingerprint.length > 0 && deviceRow.modelData.trust === "trusted"
                        spacing: Kirigami.Units.smallSpacing

                        Controls.Label {
                            Layout.fillWidth: true
                            text: parent.vaultSyncOn
                                ? qsTr("La bóveda se sincroniza con este teléfono mientras Desktop está desbloqueado y conectado. Solo los elementos cifrados con tu clave principal; carpetas e iconos quedan en Desktop.")
                                : qsTr("Sincroniza con este teléfono los elementos cifrados con tu clave principal. El teléfono necesita antes tu clave.")
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            color: Kirigami.Theme.disabledTextColor
                        }
                        Controls.Button {
                            text: parent.vaultSyncOn ? qsTr("Dejar de sincronizar la bóveda") : qsTr("Sincronizar bóveda")
                            icon.name: parent.vaultSyncOn ? "dialog-cancel" : "folder-sync"
                            enabled: !dialog.controller.busy && !dialog.controller.maintenanceBusy
                            onClicked: parent.vaultSyncOn ? dialog.controller.disableVaultSync(deviceRow.deviceId) : dialog.controller.enableVaultSync(deviceRow.deviceId)
                        }
                        Kirigami.InlineMessage {
                            Layout.fillWidth: true
                            visible: dialog.controller.vaultSyncMessage.length > 0
                            type: Kirigami.MessageType.Information
                            text: dialog.controller.vaultSyncMessage
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Kirigami.Units.largeSpacing + Kirigami.Units.iconSizes.medium + Kirigami.Units.largeSpacing
                        Layout.rightMargin: Kirigami.Units.smallSpacing
                        visible: dialog.available && dialog.controller.vaultKeyFingerprint.length > 0 && deviceRow.modelData.trust === "trusted"
                        spacing: Kirigami.Units.smallSpacing

                        Controls.Label {
                            Layout.fillWidth: true
                            text: deviceRow.connection === "connected" && deviceRow.keyTransferVersion < 2
                                ? qsTr("Actualiza KryptoPass en el teléfono para recibir tu clave OpenPGP.")
                                : qsTr("El teléfono necesita tu clave OpenPGP para abrir la bóveda. Se envía cifrada por esta conexión y sigue protegida con tu contraseña.")
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            color: Kirigami.Theme.disabledTextColor
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Button {
                                visible: deviceRow.connection === "connected" && deviceRow.syntheticTransferPhase === "ready" && deviceRow.keyTransferVersion >= 2
                                text: qsTr("Enviar mi clave al teléfono")
                                icon.name: "document-send"
                                enabled: !dialog.controller.syntheticTransferPreparing && !dialog.controller.syncBusy && !dialog.controller.busy && !dialog.controller.maintenanceBusy
                                onClicked: {
                                    dialog.keyTransferVault = true;
                                    syntheticTransferApproval.vaultIdentity = true;
                                    syntheticTransferApproval.deviceId = deviceRow.deviceId;
                                    syntheticTransferApproval.connectionGeneration = deviceRow.connectionGeneration;
                                    syntheticTransferApproval.recipient = dialog.deviceName(deviceRow.modelData);
                                    syntheticTransferApproval.fingerprint = dialog.controller.vaultKeyFingerprint;
                                    syntheticTransferApproval.approved = false;
                                    syntheticTransferApproval.open();
                                }
                            }
                            Controls.Button {
                                visible: dialog.keyTransferVault && ["waiting_acceptance", "sending", "waiting_receipt"].includes(deviceRow.syntheticTransferPhase)
                                text: qsTr("Cancelar transferencia")
                                icon.name: "dialog-cancel"
                                onClicked: { console.info("Key transfer cancel from device row cancel button"); dialog.controller.cancelSyntheticKeyTransfer(); }
                            }
                            Controls.BusyIndicator {
                                running: dialog.keyTransferVault && dialog.controller.syntheticTransferPreparing
                                visible: running
                                implicitWidth: Kirigami.Units.iconSizes.small
                                implicitHeight: implicitWidth
                            }
                            Controls.Label {
                                visible: dialog.keyTransferVault && deviceRow.connection === "connected" && deviceRow.syntheticTransferPhase !== "unsupported" && deviceRow.syntheticTransferPhase !== "ready"
                                text: dialog.keyTransferLabel(deviceRow.syntheticTransferPhase)
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                            }
                        }
                        Kirigami.InlineMessage {
                            Layout.fillWidth: true
                            visible: dialog.keyTransferVault && dialog.controller.syntheticTransferError.length > 0
                            type: Kirigami.MessageType.Error
                            text: dialog.controller.syntheticTransferError
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Kirigami.Units.largeSpacing + Kirigami.Units.iconSizes.medium + Kirigami.Units.largeSpacing
                        Layout.rightMargin: Kirigami.Units.smallSpacing
                        visible: dialog.available && dialog.controller.syntheticTransferFingerprint.length > 0 && deviceRow.modelData.trust === "trusted"
                        spacing: Kirigami.Units.smallSpacing

                        Controls.Label {
                            Layout.fillWidth: true
                            text: qsTr("Prueba debug con una clave sintética protegida. Nunca usa ni envía claves OpenPGP reales.")
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            color: Kirigami.Theme.disabledTextColor
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Button {
                                visible: deviceRow.connection === "connected" && deviceRow.syntheticTransferPhase === "ready"
                                text: qsTr("Enviar clave de prueba")
                                icon.name: "mail-send"
                                enabled: !dialog.controller.syntheticTransferPreparing && !dialog.controller.syncBusy && !dialog.controller.busy && !dialog.controller.maintenanceBusy
                                onClicked: {
                                    dialog.keyTransferVault = false;
                                    syntheticTransferApproval.vaultIdentity = false;
                                    syntheticTransferApproval.deviceId = deviceRow.deviceId;
                                    syntheticTransferApproval.connectionGeneration = deviceRow.connectionGeneration;
                                    syntheticTransferApproval.recipient = dialog.deviceName(deviceRow.modelData);
                                    syntheticTransferApproval.fingerprint = dialog.controller.syntheticTransferFingerprint;
                                    syntheticTransferApproval.approved = false;
                                    syntheticTransferApproval.open();
                                }
                            }
                            Controls.Button {
                                visible: !dialog.keyTransferVault && ["waiting_acceptance", "sending", "waiting_receipt"].includes(deviceRow.syntheticTransferPhase)
                                text: qsTr("Cancelar transferencia")
                                icon.name: "dialog-cancel"
                                onClicked: { console.info("Key transfer cancel from device details cancel button"); dialog.controller.cancelSyntheticKeyTransfer(); }
                            }
                            Controls.BusyIndicator {
                                running: !dialog.keyTransferVault && dialog.controller.syntheticTransferPreparing
                                visible: running
                                implicitWidth: Kirigami.Units.iconSizes.small
                                implicitHeight: implicitWidth
                            }
                            Controls.Label {
                                visible: !dialog.keyTransferVault && deviceRow.connection === "connected" && deviceRow.syntheticTransferPhase !== "unsupported" && deviceRow.syntheticTransferPhase !== "ready"
                                text: dialog.syntheticTransferLabel(deviceRow.syntheticTransferPhase)
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                            }
                        }
                        Kirigami.InlineMessage {
                            Layout.fillWidth: true
                            visible: !dialog.keyTransferVault && dialog.controller.syntheticTransferError.length > 0
                            type: Kirigami.MessageType.Error
                            text: dialog.controller.syntheticTransferError
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Kirigami.Units.largeSpacing + Kirigami.Units.iconSizes.medium + Kirigami.Units.largeSpacing
                        Layout.rightMargin: Kirigami.Units.smallSpacing
                        visible: deviceRow.syncExpanded
                        spacing: Kirigami.Units.smallSpacing

                        Controls.Label {
                            Layout.fillWidth: true
                            text: qsTr("Bóveda aislada con una clave de prueba conocida. Solo datos sintéticos.")
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Button {
                                text: qsTr("Activar prueba de Sync")
                                icon.name: "media-playback-start"
                                enabled: !dialog.controller.syncBusy && !dialog.controller.busy && !dialog.controller.maintenanceBusy
                                onClicked: {
                                    dialog.controller.selectDevice(deviceRow.index);
                                    dialog.controller.activateSyntheticSync();
                                }
                            }
                            Controls.Button {
                                text: qsTr("Actualizar prueba")
                                icon.name: "view-refresh"
                                enabled: dialog.controller.syncConfigured && !dialog.controller.syncBusy && !dialog.controller.busy && !dialog.controller.maintenanceBusy
                                onClicked: {
                                    dialog.controller.selectDevice(deviceRow.index);
                                    dialog.controller.refreshSyntheticSync();
                                }
                            }
                            Controls.Button {
                                text: qsTr("Crear credencial de prueba")
                                icon.name: "list-add"
                                enabled: dialog.controller.syncConfigured && !dialog.controller.syncBusy && !dialog.controller.busy && !dialog.controller.maintenanceBusy
                                onClicked: {
                                    dialog.controller.selectDevice(deviceRow.index);
                                    dialog.controller.createSyntheticCredential();
                                }
                            }
                        }
                        Controls.BusyIndicator {
                            Layout.alignment: Qt.AlignLeft
                            running: dialog.controller.syncBusy && dialog.controller.syncPeerId === deviceRow.deviceId
                            visible: running
                            implicitWidth: Kirigami.Units.iconSizes.small
                            implicitHeight: implicitWidth
                        }
                        Kirigami.InlineMessage {
                            Layout.fillWidth: true
                            visible: dialog.controller.syncPeerId === deviceRow.deviceId && dialog.controller.syncError.length > 0
                            type: Kirigami.MessageType.Error
                            text: dialog.controller.syncError
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: dialog.controller.syncPeerId === deviceRow.deviceId && dialog.controller.syncStatus.length > 0
                            text: dialog.controller.syncStatus
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: dialog.controller.syncPeerId === deviceRow.deviceId && dialog.controller.syncConfigured
                            text: qsTr("Revisiones guardadas en este equipo: %1").arg(dialog.controller.syncLocalRevisionCount)
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            color: Kirigami.Theme.disabledTextColor
                        }
                        Repeater {
                            model: dialog.controller.syncPeerId === deviceRow.deviceId ? dialog.controller.syncEntries : []
                            Controls.Label {
                                required property string modelData
                                Layout.fillWidth: true
                                text: "• " + modelData
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
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
        id: syntheticTransferApproval
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: vaultIdentity ? qsTr("Enviar tu clave OpenPGP al teléfono") : qsTr("Confirmar envío de clave de prueba")
        width: Math.min(Kirigami.Units.gridUnit * 26, dialog.width - Kirigami.Units.gridUnit * 2)
        standardButtons: Controls.Dialog.NoButton
        property string deviceId: ""
        property string connectionGeneration: ""
        property string recipient: ""
        property string fingerprint: ""
        property bool approved: false
        property bool vaultIdentity: false
        readonly property var transferDevice: dialog.devices.find(device => String(device.deviceId || "") === deviceId) || ({})
        readonly property string transferPhase: String(transferDevice.syntheticTransferPhase || "ready")
        readonly property bool transferPending: dialog.controller.syntheticTransferPreparing || ["waiting_acceptance", "sending", "waiting_receipt"].includes(transferPhase)

        onClosed: {
            if (approved && transferPending) {
                approved = false;
                console.info("Key transfer cancel from approval dialog closed");
                dialog.controller.cancelSyntheticKeyTransfer();
            }
        }
        onRejected: close()

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.mediumSpacing
            Controls.Label {
                Layout.fillWidth: true
                visible: !syntheticTransferApproval.approved
                text: qsTr("Destinatario: %1").arg(syntheticTransferApproval.recipient)
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !syntheticTransferApproval.approved
                text: syntheticTransferApproval.vaultIdentity ? qsTr("Fingerprint completo de tu clave. Compáralo con el que muestra el teléfono:") : qsTr("Fingerprint completo de la identidad OpenPGP sintética:")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !syntheticTransferApproval.approved
                text: syntheticTransferApproval.fingerprint
                textFormat: Text.PlainText
                font.family: "monospace"
                wrapMode: Text.WrapAnywhere
                Accessible.name: syntheticTransferApproval.vaultIdentity ? qsTr("Fingerprint completo de tu clave") : qsTr("Fingerprint completo de la clave de prueba")
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !syntheticTransferApproval.approved
                type: Kirigami.MessageType.Warning
                text: syntheticTransferApproval.vaultIdentity
                    ? qsTr("Se enviará la clave privada que abre tu bóveda, solo a este teléfono y solo después de que la aceptes allí. GnuPG te pedirá su contraseña para exportarla; en el teléfono seguirá protegida con esa misma contraseña. Esta confirmación autoriza un único envío en esta sesión.")
                    : qsTr("Solo se enviará la clave sintética de prueba, después de que Android la acepte. Esta confirmación autoriza una única transferencia para esta sesión.")
            }
            Controls.BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                visible: dialog.controller.syntheticTransferPreparing || syntheticTransferApproval.transferPending
                running: visible
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: syntheticTransferApproval.approved
                text: dialog.controller.syntheticTransferPreparing
                    ? (syntheticTransferApproval.vaultIdentity ? qsTr("Exportando tu clave con GnuPG; escribe su contraseña si te la pide…") : qsTr("Preparando la clave de prueba en GnuPG…"))
                    : (syntheticTransferApproval.vaultIdentity ? dialog.keyTransferLabel(syntheticTransferApproval.transferPhase) : dialog.syntheticTransferLabel(syntheticTransferApproval.transferPhase))
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: syntheticTransferApproval.approved && dialog.controller.syntheticTransferError.length > 0
                type: Kirigami.MessageType.Error
                text: dialog.controller.syntheticTransferError
            }
        }
        footer: Controls.DialogButtonBox {
            Controls.Button {
                text: syntheticTransferApproval.approved ? (syntheticTransferApproval.transferPending ? qsTr("Cancelar transferencia") : qsTr("Cerrar")) : qsTr("Cancelar")
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
                onClicked: {
                    if (syntheticTransferApproval.approved && syntheticTransferApproval.transferPending) {
                        console.info("Key transfer cancel from approval dialog cancel button");
                        dialog.controller.cancelSyntheticKeyTransfer();
                    }
                    syntheticTransferApproval.approved = false;
                    syntheticTransferApproval.close();
                }
            }
            Controls.Button {
                visible: !syntheticTransferApproval.approved
                text: qsTr("Confirmar y enviar")
                icon.name: "mail-send"
                // ActionRole keeps the dialog open: AcceptRole would close it, and closing
                // an approved, pending transfer cancels it in onClosed.
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
                onClicked: {
                    syntheticTransferApproval.approved = true;
                    if (syntheticTransferApproval.vaultIdentity)
                        dialog.controller.startVaultKeyTransfer(syntheticTransferApproval.deviceId, syntheticTransferApproval.connectionGeneration, syntheticTransferApproval.fingerprint);
                    else
                        dialog.controller.startSyntheticKeyTransfer(syntheticTransferApproval.deviceId, syntheticTransferApproval.connectionGeneration, syntheticTransferApproval.fingerprint);
                }
            }
            onRejected: syntheticTransferApproval.close()
        }
    }

    Controls.Dialog {
        id: inboundKeyDialog
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        closePolicy: Controls.Popup.NoAutoClose
        title: qsTr("Recibir tu clave OpenPGP desde el teléfono")
        width: Math.min(Kirigami.Units.gridUnit * 28, dialog.width - Kirigami.Units.gridUnit * 2)
        standardButtons: Controls.Dialog.NoButton
        readonly property var transfer: dialog.inboundTransfer
        readonly property string phase: String(transfer.phase || "")
        readonly property bool working: ["assessing", "importing", "verifying"].includes(phase)

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.mediumSpacing
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Origen: %1").arg(String(inboundKeyDialog.transfer.deviceName || ""))
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Fingerprint completo de la clave ofrecida. Compáralo con el que muestra el teléfono:")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: String(inboundKeyDialog.transfer.fingerprint || "")
                textFormat: Text.PlainText
                font.family: "monospace"
                wrapMode: Text.WrapAnywhere
                Accessible.name: qsTr("Fingerprint completo de la clave ofrecida")
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: String(inboundKeyDialog.transfer.assessmentMessage || "").length > 0
                type: ["new", "add_secret", "merge"].includes(String(inboundKeyDialog.transfer.assessment || "")) ? Kirigami.MessageType.Information : Kirigami.MessageType.Warning
                text: String(inboundKeyDialog.transfer.assessmentMessage || "")
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: inboundKeyDialog.transfer.canDecide === true
                text: qsTr("Solo se aceptará la clave privada protegida con contraseña de esta huella. Después de importarla, GnuPG puede pedirte su contraseña para comprobar que funciona.")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Controls.BusyIndicator {
                    running: inboundKeyDialog.working
                    visible: running
                    implicitWidth: Kirigami.Units.iconSizes.small
                    implicitHeight: implicitWidth
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: dialog.inboundPhaseLabel(inboundKeyDialog.transfer)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    font.bold: true
                }
            }
            Controls.ProgressBar {
                Layout.fillWidth: true
                visible: inboundKeyDialog.phase === "receiving"
                from: 0
                to: Math.max(1, Number(inboundKeyDialog.transfer.total || 0))
                value: Number(inboundKeyDialog.transfer.received || 0)
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: String(inboundKeyDialog.transfer.message || "").length > 0
                type: inboundKeyDialog.transfer.error === true ? Kirigami.MessageType.Error : (inboundKeyDialog.phase === "complete" ? Kirigami.MessageType.Positive : Kirigami.MessageType.Information)
                text: String(inboundKeyDialog.transfer.message || "")
            }
        }
        footer: Controls.DialogButtonBox {
            Controls.Button {
                visible: inboundKeyDialog.transfer.canDecide === true || inboundKeyDialog.phase === "assessing"
                text: qsTr("Rechazar")
                icon.name: "dialog-cancel"
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
                onClicked: dialog.controller.rejectInboundKeyTransfer()
            }
            Controls.Button {
                visible: inboundKeyDialog.transfer.canDecide === true
                text: qsTr("Aceptar e importar")
                icon.name: "document-import"
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
                onClicked: dialog.controller.acceptInboundKeyTransfer()
            }
            Controls.Button {
                visible: inboundKeyDialog.transfer.finished === true
                text: qsTr("Cerrar")
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
                onClicked: dialog.controller.dismissInboundKeyTransfer()
            }
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
