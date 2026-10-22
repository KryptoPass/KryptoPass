pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: dialog
    required property var controller

    anchors.centerIn: parent
    parent: Controls.Overlay.overlay
    width: Math.min(880, parent ? parent.width - Kirigami.Units.gridUnit * 2 : 880)
    height: Math.min(720, parent ? parent.height - Kirigami.Units.gridUnit * 2 : 720)
    modal: true
    title: qsTr("Dispositivos")
    closePolicy: Controls.Popup.CloseOnEscape
    standardButtons: Controls.Dialog.NoButton
    padding: 20
    background: Rectangle {
        color: Kirigami.Theme.backgroundColor
        radius: 6
        border.width: 1
        border.color: Kirigami.Theme.disabledTextColor
    }
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
    readonly property string statusHint: hintForStatus(controller.statusCode)
    readonly property bool statusIsProblem: ["expired", "rejected", "profile_error", "keystore_error", "authentication_error", "transport_error", "network_policy_blocked", "network_policy_changed"].includes(controller.statusCode)

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

    onOpened: {
        controller.loadNetworkAddresses();
        controller.loadDevices();
        if (controller.awaiting)
            Qt.callLater(function () {
                noMatchesButton.forceActiveFocus();
            });
    }
    onClosed: {
        if (aliasDialog.opened)
            aliasDialog.close();
        controller.releaseUi();
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

    contentItem: Controls.ScrollView {
        id: pageScroll
        clip: true
        contentWidth: availableWidth

        ColumnLayout {
            width: pageScroll.availableWidth
            spacing: Kirigami.Units.largeSpacing

            RowLayout {
                Layout.fillWidth: true
                spacing: 16
                Kirigami.Icon {
                    source: "network-connect"
                    Layout.preferredWidth: 40
                    Layout.preferredHeight: 40
                    color: Kirigami.Theme.linkColor
                    Accessible.ignored: true
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Controls.Label {
                        Layout.fillWidth: true
                        text: qsTr("Conecta tus dispositivos")
                        font.pixelSize: 22
                        font.weight: Font.DemiBold
                        wrapMode: Text.WordWrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: qsTr("Vincula Android y conserva una conexión segura entre tus dispositivos.")
                        wrapMode: Text.WordWrap
                        color: Kirigami.Theme.disabledTextColor
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Conexión de pruebas · Sin sincronización de bóveda")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Kirigami.Theme.disabledTextColor
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: !dialog.statusIsProblem
                text: dialog.controller.status
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: dialog.statusIsProblem
                type: ["expired", "rejected"].includes(dialog.controller.statusCode) ? Kirigami.MessageType.Warning : Kirigami.MessageType.Error
                text: dialog.controller.status + (dialog.statusHint.length > 0 ? "\n" + dialog.statusHint : "")
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: dialog.controller.aliasError.length > 0 && !aliasDialog.opened
                type: Kirigami.MessageType.Warning
                text: dialog.controller.aliasError
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.statusHint.length > 0 && !dialog.statusIsProblem
                text: dialog.statusHint
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.controller.sessionActive
                text: qsTr("Hay una sesión activa con %1. Desconecta ese dispositivo antes de vincular o reconectar otro.").arg(dialog.shortId(dialog.controller.activeDeviceId))
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }

            Controls.Frame {
                Layout.fillWidth: true
                visible: dialog.waitingForPeer
                padding: 16

                GridLayout {
                    anchors.fill: parent
                    columns: width >= 620 ? 2 : 1
                    columnSpacing: Kirigami.Units.largeSpacing
                    rowSpacing: Kirigami.Units.largeSpacing

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
                        spacing: Kirigami.Units.smallSpacing

                        Kirigami.Heading {
                            Layout.fillWidth: true
                            level: 3
                            text: qsTr("Código de conexión")
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Rectangle {
                            Layout.alignment: Qt.AlignHCenter
                            Layout.preferredWidth: Math.min(250, pageScroll.availableWidth * 0.62)
                            Layout.preferredHeight: Layout.preferredWidth
                            color: "white"
                            border.color: Kirigami.Theme.disabledTextColor
                            border.width: 1
                            radius: Kirigami.Units.smallSpacing
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

                        Controls.Label {
                            Layout.fillWidth: true
                            text: qsTr("Escanea este código con KryptoPass Android.")
                            textFormat: Text.PlainText
                            wrapMode: Text.WordWrap
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        spacing: Kirigami.Units.mediumSpacing

                        Controls.Label {
                            Layout.fillWidth: true
                            text: dialog.controller.internetEnabled ? qsTr("Comprueba que ambos dispositivos tengan acceso a Internet.") : (dialog.controller.lanEnabled ? qsTr("Asegúrate de que ambos dispositivos estén en la misma red local.") : qsTr("Usa este modo solo para el emulador en esta computadora."))
                            textFormat: Text.PlainText
                            wrapMode: Text.WordWrap
                        }

                        Controls.Button {
                            Layout.fillWidth: true
                            text: dialog.showInvitationCode ? qsTr("Ocultar código manual") : qsTr("Mostrar código manual")
                            onClicked: dialog.showInvitationCode = !dialog.showInvitationCode
                        }

                        Controls.TextArea {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 112
                            visible: dialog.showInvitationCode
                            text: dialog.controller.bootstrap
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.WrapAnywhere
                            textFormat: TextEdit.PlainText
                            Accessible.name: qsTr("Código de invitación de un solo uso")
                        }

                        Controls.Button {
                            Layout.fillWidth: true
                            visible: dialog.showInvitationCode
                            text: qsTr("Copiar código")
                            onClicked: dialog.copyRequested(dialog.controller.bootstrap)
                        }
                    }
                }
            }

            Controls.Frame {
                Layout.fillWidth: true
                visible: dialog.controller.awaiting || dialog.controller.statusCode === "confirming"
                padding: 16

                ColumnLayout {
                    anchors.fill: parent
                    spacing: Kirigami.Units.mediumSpacing

                    Kirigami.Heading {
                        Layout.fillWidth: true
                        level: 3
                        text: qsTr("Compara los siete elementos")
                        wrapMode: Text.WordWrap
                    }

                    GridLayout {
                        Layout.fillWidth: true
                        columns: width >= 820 ? 7 : width >= 440 ? 4 : 2
                        columnSpacing: Kirigami.Units.smallSpacing
                        rowSpacing: Kirigami.Units.smallSpacing

                        Repeater {
                            model: dialog.sasRows
                            Controls.Frame {
                                id: sasCell
                                required property int index
                                required property var modelData
                                Layout.fillWidth: true
                                Layout.minimumWidth: 88
                                Layout.preferredWidth: 112

                                ColumnLayout {
                                    anchors.fill: parent
                                    spacing: Kirigami.Units.smallSpacing

                                    Controls.Label {
                                        Layout.fillWidth: true
                                        text: String(sasCell.modelData.emoji || "")
                                        font.pixelSize: 30
                                        horizontalAlignment: Text.AlignHCenter
                                        textFormat: Text.PlainText
                                        Accessible.name: qsTr("Elemento %1: %2").arg(sasCell.index + 1).arg(String(sasCell.modelData.emoji || ""))
                                    }

                                    Controls.Label {
                                        Layout.fillWidth: true
                                        text: String(sasCell.modelData.name || "")
                                        textFormat: Text.PlainText
                                        wrapMode: Text.WordWrap
                                        horizontalAlignment: Text.AlignHCenter
                                    }
                                }
                            }
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.sasReady ? qsTr("Si un solo emoji o nombre no coincide, rechaza la vinculación.") : qsTr("No se pudo mostrar la comparación completa. Rechaza la vinculación y vuelve a intentarlo.")
                        textFormat: Text.PlainText
                        wrapMode: Text.WordWrap
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        visible: dialog.controller.awaiting
                        Controls.Button {
                            id: noMatchesButton
                            Layout.fillWidth: true
                            text: qsTr("No coinciden")
                            onClicked: dialog.controller.decide(false)
                        }
                        Controls.Button {
                            Layout.fillWidth: true
                            text: qsTr("Los 7 coinciden")
                            highlighted: true
                            enabled: dialog.sasReady
                            onClicked: dialog.controller.decide(true)
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true

                Controls.ToolButton {
                    text: dialog.networkOptionsVisible ? qsTr("Ocultar opciones de conexión") : qsTr("Opciones de conexión")
                    icon.name: "network-wired"
                    checkable: true
                    checked: dialog.networkOptionsVisible
                    onToggled: dialog.networkOptionsVisible = checked
                }
                Item {
                    Layout.fillWidth: true
                }
                Controls.Button {
                    text: qsTr("Actualizar redes")
                    visible: dialog.networkOptionsVisible
                    icon.name: "view-refresh"
                    enabled: !dialog.controller.busy && !dialog.waitingForPeer && !dialog.controller.awaiting
                    onClicked: dialog.controller.loadNetworkAddresses()
                }
            }

            Controls.Frame {
                Layout.fillWidth: true
                visible: dialog.networkOptionsVisible
                padding: 16

                ColumnLayout {
                    anchors.fill: parent
                    spacing: Kirigami.Units.mediumSpacing

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

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: dialog.controller.vpnStatus == 1
                        text: qsTr("Desktop no puede verificar que DNS, señalización, STUN, TURN y UDP estén cubiertos por la VPN. Exigir VPN bloquea las conexiones por Internet; la recuperación LAN también queda bloqueada hasta activar su excepción.")
                        textFormat: Text.PlainText
                        wrapMode: Text.WordWrap
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: dialog.controller.relayOnly
                        text: qsTr("Solo retransmisión elimina STUN y candidatos directos, exige un servidor TURN válido y bloquea las conexiones LAN directas.")
                        textFormat: Text.PlainText
                        wrapMode: Text.WordWrap
                    }

                    Controls.CheckBox {
                        Layout.fillWidth: true
                        visible: !dialog.controller.internetEnabled
                        text: qsTr("Usar conexión por red local")
                        checked: dialog.controller.lanEnabled
                        enabled: !dialog.controller.busy
                        onToggled: dialog.controller.configureNetwork(checked, dialog.controller.networkIndex)
                    }

                    Controls.ComboBox {
                        Layout.fillWidth: true
                        visible: dialog.controller.lanEnabled && !dialog.controller.internetEnabled
                        enabled: !dialog.controller.busy
                        model: dialog.controller.networkAddresses
                        currentIndex: dialog.controller.networkIndex
                        Accessible.name: qsTr("Interfaz de red local de Desktop")
                        onActivated: index => dialog.controller.configureNetwork(true, index)
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: dialog.controller.lanEnabled && !dialog.controller.internetEnabled && dialog.controller.networkIndex < 0
                        text: qsTr("No hay una dirección local disponible. Actualiza las redes o usa el modo emulador.")
                        textFormat: Text.PlainText
                        wrapMode: Text.WordWrap
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !dialog.controller.lanEnabled && !dialog.controller.internetEnabled
                        text: qsTr("Modo emulador avanzado: solo funciona cuando Android se ejecuta en esta computadora.")
                        textFormat: Text.PlainText
                        wrapMode: Text.WordWrap
                    }
                }
            }

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 3
                font.pixelSize: 17
                text: qsTr("Dispositivos autorizados")
                wrapMode: Text.WordWrap
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.devices.length === 0 && dialog.controller.busy
                text: qsTr("Cargando dispositivos autorizados…")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.devices.length === 0 && !dialog.controller.busy
                text: qsTr("Todavía no hay dispositivos autorizados en este perfil. Vincula Android para añadir uno.")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }

            Repeater {
                model: dialog.devices

                Controls.Frame {
                    id: deviceCard
                    required property int index
                    required property var modelData
                    Layout.fillWidth: true
                    padding: 16

                    ColumnLayout {
                        anchors.fill: parent
                        spacing: Kirigami.Units.smallSpacing

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.mediumSpacing

                            Kirigami.Icon {
                                source: "computer"
                                Layout.preferredWidth: 36
                                Layout.preferredHeight: 36
                                Layout.alignment: Qt.AlignTop
                                color: Kirigami.Theme.disabledTextColor
                                Accessible.ignored: true
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: Kirigami.Units.smallSpacing

                                Kirigami.Heading {
                                    Layout.fillWidth: true
                                    level: 4
                                    font.pixelSize: 17
                                    font.weight: Font.DemiBold
                                    text: String(deviceCard.modelData.alias || "").length > 0 ? String(deviceCard.modelData.alias) : String(deviceCard.modelData.displayName || qsTr("Dispositivo"))
                                    textFormat: Text.PlainText
                                    wrapMode: Text.WordWrap
                                }

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Kirigami.Units.smallSpacing

                                    Controls.Label {
                                        text: deviceCard.modelData.trust === "trusted" ? qsTr("Emparejado") : qsTr("Confianza no disponible")
                                        textFormat: Text.PlainText
                                        color: Kirigami.Theme.disabledTextColor
                                        Accessible.name: qsTr("Estado de confianza: %1").arg(text)
                                    }

                                    Controls.Label {
                                        text: qsTr("·")
                                        visible: true
                                    }

                                    Controls.Label {
                                        text: dialog.connectionLabel(String(deviceCard.modelData.connection || ""))
                                        color: dialog.connectionColor(String(deviceCard.modelData.connection || ""))
                                        font.weight: Font.DemiBold
                                        textFormat: Text.PlainText
                                        Accessible.name: qsTr("Estado de conexión: %1").arg(text)
                                    }
                                }

                                Controls.Label {
                                    Layout.fillWidth: true
                                    visible: deviceCard.modelData.connection === "connected"
                                    text: qsTr("Transporte: %1").arg(String(deviceCard.modelData.routeLabel || qsTr("Comprobando…")))
                                    textFormat: Text.PlainText
                                    wrapMode: Text.WordWrap
                                    color: Kirigami.Theme.disabledTextColor
                                    Accessible.name: text
                                }

                                Controls.Label {
                                    Layout.fillWidth: true
                                    visible: deviceCard.modelData.connection === "connected"
                                    text: qsTr("Red: %1").arg(({
                                            ipv4: "IPv4",
                                            ipv6: "IPv6",
                                            mixed: "IPv4 / IPv6"
                                        })[deviceCard.modelData.addressFamily] || "—")
                                    textFormat: Text.PlainText
                                    color: Kirigami.Theme.disabledTextColor
                                    Accessible.name: text
                                }

                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: qsTr("ID %1").arg(dialog.shortId(deviceCard.modelData.deviceId))
                                    textFormat: Text.PlainText
                                    font.family: "monospace"
                                    wrapMode: Text.WrapAnywhere
                                }
                            }

                            Controls.Button {
                                text: qsTr("Cambiar nombre")
                                icon.name: "edit-rename"
                                display: deviceCard.width < 600 ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                                Controls.ToolTip.visible: hovered
                                Controls.ToolTip.text: text
                                enabled: !dialog.controller.aliasBusy && !dialog.controller.busy
                                Accessible.name: qsTr("Editar nombre local del dispositivo")
                                onClicked: dialog.openAliasEditor(deviceCard.modelData)
                            }

                            Controls.Button {
                                text: dialog.expandedDeviceId === String(deviceCard.modelData.deviceId) ? qsTr("Ocultar ID") : qsTr("Detalles")
                                Accessible.name: qsTr("Ver identificador completo del dispositivo")
                                icon.name: "dialog-information"
                                display: deviceCard.width < 600 ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                                Controls.ToolTip.visible: hovered
                                Controls.ToolTip.text: text
                                onClicked: {
                                    dialog.expandedDeviceId = dialog.expandedDeviceId === String(deviceCard.modelData.deviceId) ? "" : String(deviceCard.modelData.deviceId);
                                    dialog.controller.selectDevice(deviceCard.index);
                                }
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true

                            Controls.Button {
                                text: qsTr("Reconectar")
                                icon.name: "network-connect"
                                enabled: !dialog.controller.busy && !dialog.controller.sessionActive && deviceCard.modelData.trust === "trusted"
                                onClicked: {
                                    dialog.reconnectFlow = true;
                                    dialog.controller.selectDevice(deviceCard.index);
                                    dialog.controller.startReconnect();
                                }
                            }

                            Controls.Button {
                                text: qsTr("Desconectar")
                                icon.name: "network-disconnect"
                                visible: !!deviceCard.modelData.isActive
                                enabled: !!deviceCard.modelData.isActive && dialog.controller.statusCode !== "closing"
                                onClicked: dialog.controller.disconnectPeer(String(deviceCard.modelData.deviceId))
                            }

                            Controls.ToolButton {
                                text: qsTr("Olvidar")
                                icon.name: "edit-delete"
                                enabled: !dialog.controller.busy && !dialog.controller.aliasBusy
                                onClicked: {
                                    dialog.forgetTargetId = String(deviceCard.modelData.deviceId);
                                    dialog.forgetTargetName = String(deviceCard.modelData.displayName);
                                    dialog.resetRequested = false;
                                    deleteDialog.open();
                                }
                            }

                            Item {
                                Layout.fillWidth: true
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            visible: dialog.expandedDeviceId === String(deviceCard.modelData.deviceId)

                            Controls.Label {
                                Layout.fillWidth: true
                                text: String(deviceCard.modelData.deviceId || "")
                                textFormat: Text.PlainText
                                font.family: "monospace"
                                wrapMode: Text.WrapAnywhere
                                Accessible.name: qsTr("Identificador completo del dispositivo")
                            }

                            Controls.ToolButton {
                                text: qsTr("Copiar ID")
                                onClicked: dialog.copyRequested(String(deviceCard.modelData.deviceId || ""))
                            }
                        }
                    }
                }
            }
        }
    }

    footer: RowLayout {
        Controls.Button {
            text: dialog.width < 500 ? qsTr("Vincular") : qsTr("Vincular dispositivo")
            highlighted: true
            icon.name: "list-add"
            enabled: dialog.controller.available && !dialog.controller.busy && !dialog.controller.sessionActive && !dialog.controller.awaiting
            onClicked: {
                dialog.reconnectFlow = false;
                dialog.controller.startPairing();
            }
        }

        Controls.Button {
            text: dialog.width < 500 ? qsTr("Actualizar") : qsTr("Actualizar dispositivos")
            icon.name: "view-refresh"
            enabled: !dialog.controller.busy && !dialog.controller.awaiting
            onClicked: {
                dialog.controller.loadDevices();
                dialog.controller.loadNetworkAddresses();
            }
        }

        Controls.ToolButton {
            text: qsTr("Reiniciar vinculación")
            icon.name: "edit-clear-all"
            enabled: dialog.controller.available && !dialog.controller.busy && !dialog.controller.aliasBusy
            onClicked: {
                dialog.resetRequested = true;
                deleteDialog.open();
            }
        }

        Item {
            Layout.fillWidth: true
        }

        Controls.Button {
            enabled: !dialog.controller.maintenanceBusy
            text: dialog.controller.busy ? qsTr("Cancelar") : qsTr("Cerrar")
            onClicked: {
                if (dialog.controller.busy)
                    dialog.controller.cancel();
                else
                    dialog.close();
            }
        }
    }

    Controls.Dialog {
        id: deleteDialog
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: dialog.resetRequested ? qsTr("¿Reiniciar vinculación?") : qsTr("¿Olvidar dispositivo?")
        width: Math.min(460, dialog.parent ? dialog.parent.width - 32 : 460)
        standardButtons: Controls.Dialog.NoButton
        contentItem: Controls.Label {
            text: dialog.resetRequested ? qsTr("Se cerrará la conexión y se borrarán la identidad privada de este perfil de vinculación, sus autorizaciones y nombres locales. La bóveda y las claves OpenPGP se conservan. Para volver a vincular, compara un SAS nuevo.") : qsTr("Se cerrará la conexión y se eliminarán la autorización y el nombre local de «%1» en este equipo. También olvida este equipo en Android antes de volver a emparejarlos. La bóveda y OpenPGP se conservan.").arg(dialog.forgetTargetName)
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
        }
        footer: RowLayout {
            Controls.Button {
                text: qsTr("Cancelar")
                onClicked: deleteDialog.close()
            }
            Item {
                Layout.fillWidth: true
            }
            Controls.Button {
                text: dialog.resetRequested ? qsTr("Reiniciar vinculación") : qsTr("Olvidar dispositivo")
                onClicked: {
                    if (dialog.resetRequested)
                        dialog.controller.resetPairingProfile();
                    else
                        dialog.controller.forgetDevice(dialog.forgetTargetId);
                    deleteDialog.close();
                }
            }
        }
    }

    Controls.Dialog {
        id: aliasDialog
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: qsTr("Nombre del dispositivo")
        width: Math.min(420, dialog.parent ? dialog.parent.width - Kirigami.Units.gridUnit * 2 : 420)
        standardButtons: Controls.Dialog.NoButton

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.mediumSpacing

            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Solo se guarda en este equipo.")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }

            Controls.TextField {
                id: aliasField
                Layout.fillWidth: true
                text: dialog.aliasDraft
                maximumLength: 64
                enabled: !dialog.controller.aliasBusy
                selectByMouse: true
                Accessible.name: qsTr("Nombre local del dispositivo")
                onTextEdited: dialog.aliasDraft = text
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.controller.aliasError.length > 0
                text: dialog.controller.aliasError
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }

            RowLayout {
                Layout.fillWidth: true
                Controls.Button {
                    text: qsTr("Quitar nombre")
                    enabled: !dialog.controller.aliasBusy
                    onClicked: {
                        dialog.aliasDraft = "";
                        aliasField.text = "";
                        dialog.saveAlias();
                    }
                }
                Item {
                    Layout.fillWidth: true
                }
                Controls.Button {
                    text: qsTr("Cancelar")
                    enabled: !dialog.controller.aliasBusy
                    onClicked: aliasDialog.close()
                }
                Controls.Button {
                    text: dialog.controller.aliasBusy ? qsTr("Guardando…") : qsTr("Guardar")
                    highlighted: true
                    enabled: !dialog.controller.aliasBusy
                    onClicked: {
                        dialog.aliasDraft = aliasField.text;
                        dialog.saveAlias();
                    }
                }
            }
        }
    }
}
