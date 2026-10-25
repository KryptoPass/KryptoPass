pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.ApplicationWindow {
    id: dialog
    objectName: "devicesWindow"
    required property var controller
    required property var keyStore
    // Desktop only exchanges changes with phones while its vault is unlocked.
    property bool vaultLocked: false
    readonly property bool opened: visible
    readonly property bool available: controller.available

    signal copyRequested(string code)

    property bool networkOptionsVisible: false
    property bool showInvitationCode: false
    property bool reconnectFlow: false
    property string selectedDeviceId: ""
    readonly property var selectedDevice: devices.find(device => String(device.deviceId) === selectedDeviceId) || null
    readonly property bool showingDetail: selectedDevice !== null
    readonly property var activeDevice: devices.find(device => String(device.deviceId) === controller.activeDeviceId) || null
    readonly property string activeDeviceName: activeDevice ? deviceName(activeDevice) : qsTr("otro dispositivo")
    // Which offer the last key transfer of this window carried, to label its status.
    property bool keyTransferVault: false
    property string keyTransferDeviceId: ""
    property string syncMessageDeviceId: ""
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
    readonly property bool statusIsProblem: ["expired", "rejected", "profile_error", "keystore_error", "authentication_error", "transport_error", "network_policy_blocked", "network_policy_changed", "sync_history_error"].includes(controller.statusCode)
    readonly property bool canPair: controller.available && !controller.busy && !controller.sessionActive && !controller.awaiting

    flags: Qt.Dialog | Qt.WindowTitleHint | Qt.WindowSystemMenuHint | Qt.WindowCloseButtonHint | Qt.WindowMaximizeButtonHint
    modality: Qt.WindowModal
    width: Kirigami.Units.gridUnit * 34
    height: Kirigami.Units.gridUnit * 36
    minimumWidth: Kirigami.Units.gridUnit * 24
    minimumHeight: Kirigami.Units.gridUnit * 22
    title: qsTr("Dispositivos")

    function open() {
        // Devices expose keys and Sync, so they stay closed while the vault is locked.
        if (vaultLocked)
            return;
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
            return parsed && typeof parsed === "object" ? parsed : ({
                    visible: false
                });
        } catch (error) {
            return ({
                    visible: false
                });
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
        case "ready":
            return qsTr("Lista para enviar la clave elegida");
        case "unsupported":
            return qsTr("Este dispositivo no admite recibir la clave");
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
        if (controller.sessionActive) {
            connectionLimit.open();
            return;
        }
        if (!canPair)
            return;
        reconnectFlow = false;
        controller.startPairing();
    }

    function openDevice(device) {
        if (comparing)
            return;
        selectedDeviceId = String(device.deviceId || "");
        networkOptionsVisible = false;
    }

    function reconnectDevice(device) {
        if (controller.sessionActive) {
            connectionLimit.open();
            return;
        }
        if (controller.busy || controller.syncBusy || controller.maintenanceBusy || device.trust !== "trusted")
            return;
        const index = devices.findIndex(candidate => candidate.deviceId === device.deviceId);
        if (index < 0)
            return;
        reconnectFlow = true;
        selectedDeviceId = "";
        controller.selectDevice(index);
        controller.startReconnect();
    }

    function forgetDevice(device) {
        forgetTargetId = String(device.deviceId || "");
        forgetTargetName = deviceName(device);
        resetRequested = false;
        deleteDialog.open();
    }

    function setVaultSync(deviceId, enabled) {
        syncMessageDeviceId = deviceId;
        if (enabled) {
            const device = devices.find(candidate => String(candidate.deviceId) === deviceId);
            syncKeysPicker.deviceId = deviceId;
            syncKeysPicker.recipient = device ? deviceName(device) : "";
            syncKeysPicker.phoneKeys = device && device.vaultKeys ? device.vaultKeys : null;
            syncKeysPicker.open();
        } else {
            controller.disableVaultSync(deviceId);
        }
    }

    function keyTitle(fingerprint) {
        const key = keyStore.entries.find(entry => String(entry.id).toUpperCase() === String(fingerprint).toUpperCase());
        return key ? key.title : String(fingerprint).slice(-16);
    }

    function vaultSyncKeyIds(deviceId) {
        try {
            return JSON.parse(controller.vaultSyncKeys)[deviceId] || [];
        } catch (error) {
            return [];
        }
    }

    function canTransferKey(key) {
        return !!key && key.hasSecret && key.statusCode === "valid" && key.canEncrypt && /^[A-Fa-f0-9]{40}$/.test(String(key.id));
    }

    function chooseKey(device) {
        keyPicker.deviceId = String(device.deviceId);
        keyPicker.connectionGeneration = String(device.connectionGeneration);
        keyPicker.recipient = deviceName(device);
        keyPicker.selectedFingerprint = "";
        keyStore.controller.refreshKeys();
        keyPicker.open();
    }

    function approveTransfer(device, vaultIdentity, fingerprint, keyTitle) {
        keyTransferVault = vaultIdentity;
        keyTransferDeviceId = String(device.deviceId);
        syntheticTransferApproval.vaultIdentity = vaultIdentity;
        syntheticTransferApproval.deviceId = String(device.deviceId);
        syntheticTransferApproval.connectionGeneration = String(device.connectionGeneration);
        syntheticTransferApproval.recipient = deviceName(device);
        syntheticTransferApproval.fingerprint = fingerprint;
        syntheticTransferApproval.keyTitle = keyTitle || "";
        syntheticTransferApproval.approved = false;
        syntheticTransferApproval.open();
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
            keyPicker.close();
            syntheticTransferApproval.close();
            connectionLimit.close();
            selectedDeviceId = "";
            // Cancels an unfinished pairing and clears QR/SAS; an active session survives.
            controller.releaseUi();
        }
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        enabled: dialog.visible && !aliasDialog.opened && !deleteDialog.opened && !keyPicker.opened && !syntheticTransferApproval.opened && !inboundKeyDialog.opened && !connectionLimit.opened && !dialog.controller.maintenanceBusy
        onActivated: {
            if (dialog.showingDetail)
                dialog.selectedDeviceId = "";
            else
                dialog.close();
        }
    }

    Connections {
        target: dialog.controller
        function onAliasSaved(deviceId) {
            if (String(deviceId) === dialog.aliasTargetId)
                aliasDialog.close();
        }
        function onAwaitingChanged() {
            if (dialog.controller.awaiting) {
                dialog.selectedDeviceId = "";
                Qt.callLater(function () {
                    noMatchesButton.forceActiveFocus();
                });
            }
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
            Controls.ToolButton {
                visible: dialog.showingDetail
                icon.name: "go-previous"
                text: qsTr("Volver a dispositivos")
                display: Controls.AbstractButton.IconOnly
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: text
                onClicked: dialog.selectedDeviceId = ""
            }
            Kirigami.Icon {
                visible: !dialog.showingDetail
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
                    text: dialog.showingDetail ? dialog.deviceName(dialog.selectedDevice) : qsTr("Tus dispositivos")
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: dialog.showingDetail ? qsTr("Conexión y opciones del dispositivo") : qsTr("Vincula y conecta tus dispositivos")
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
                        onTriggered: {
                            dialog.selectedDeviceId = "";
                            dialog.networkOptionsVisible = checked;
                        }
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
                        text: qsTr("Borrar todas las vinculaciones…")
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

    DeviceDetailsPage {
        anchors.fill: parent
        visible: dialog.showingDetail
        host: dialog
        modelData: dialog.selectedDevice || ({})
    }

    Controls.ScrollView {
        id: pageScroll
        anchors.fill: parent
        visible: !dialog.showingDetail
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
                delegate: Controls.ItemDelegate {
                    id: deviceRow
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing
                    Layout.rightMargin: Kirigami.Units.largeSpacing
                    leftPadding: Kirigami.Units.largeSpacing
                    rightPadding: Kirigami.Units.smallSpacing
                    topPadding: Kirigami.Units.largeSpacing
                    bottomPadding: Kirigami.Units.largeSpacing
                    text: dialog.deviceName(modelData)
                    enabled: !dialog.comparing
                    Accessible.description: dialog.connectionLabel(String(modelData.connection || ""))
                    onClicked: dialog.openDevice(modelData)
                    contentItem: RowLayout {
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
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Label {
                                Layout.fillWidth: true
                                text: dialog.deviceName(deviceRow.modelData)
                                textFormat: Text.PlainText
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            RowLayout {
                                spacing: Kirigami.Units.smallSpacing
                                Rectangle {
                                    implicitWidth: 7
                                    implicitHeight: 7
                                    radius: width / 2
                                    color: dialog.connectionColor(String(deviceRow.modelData.connection || ""))
                                    Accessible.ignored: true
                                }
                                Controls.Label {
                                    text: dialog.connectionLabel(String(deviceRow.modelData.connection || ""))
                                    color: dialog.connectionColor(String(deviceRow.modelData.connection || ""))
                                    textFormat: Text.PlainText
                                }
                            }
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
                                    text: qsTr("Ver detalles y opciones")
                                    icon.name: "document-properties"
                                    onTriggered: dialog.openDevice(deviceRow.modelData)
                                }
                                Controls.MenuItem {
                                    text: deviceRow.modelData.isActive ? qsTr("Desconectar") : qsTr("Reconectar")
                                    icon.name: deviceRow.modelData.isActive ? "network-disconnect" : "network-connect"
                                    enabled: !dialog.controller.busy && !dialog.controller.syncBusy && !dialog.controller.maintenanceBusy && dialog.controller.statusCode !== "closing"
                                    onTriggered: {
                                        if (deviceRow.modelData.isActive)
                                            dialog.controller.disconnectPeer(String(deviceRow.modelData.deviceId));
                                        else
                                            dialog.reconnectDevice(deviceRow.modelData);
                                    }
                                }
                                Controls.MenuItem {
                                    text: qsTr("Cambiar nombre…")
                                    icon.name: "edit-rename"
                                    enabled: !dialog.controller.aliasBusy && !dialog.controller.busy
                                    onTriggered: dialog.openAliasEditor(deviceRow.modelData)
                                }
                                Controls.MenuSeparator {}
                                Controls.MenuItem {
                                    text: qsTr("Olvidar dispositivo…")
                                    icon.name: "edit-delete"
                                    enabled: !dialog.controller.busy && !dialog.controller.aliasBusy
                                    onTriggered: dialog.forgetDevice(deviceRow.modelData)
                                }
                            }
                        }
                        Kirigami.Icon {
                            source: "go-next-symbolic"
                            implicitWidth: Kirigami.Units.iconSizes.small
                            implicitHeight: Kirigami.Units.iconSizes.small
                            Accessible.ignored: true
                        }
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
                    icon.name: "network-connect"
                    enabled: dialog.canPair || dialog.controller.sessionActive
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
                visible: !dialog.showingDetail
                icon.name: "network-connect"
                enabled: dialog.canPair || dialog.controller.sessionActive
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
        title: dialog.resetRequested ? qsTr("¿Borrar todas las vinculaciones?") : qsTr("¿Olvidar dispositivo?")
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
                text: dialog.resetRequested ? qsTr("Borrar todas las vinculaciones") : qsTr("Olvidar dispositivo")
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
        id: connectionLimit
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: qsTr("Una conexión a la vez")
        width: Math.min(Kirigami.Units.gridUnit * 24, dialog.width - Kirigami.Units.gridUnit * 2)
        contentItem: Controls.Label {
            text: qsTr("Ahora estás conectado a %1. Para conectar otro dispositivo, abre esta conexión y pulsa Desconectar. Tus dispositivos seguirán vinculados.").arg(dialog.activeDeviceName)
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        footer: Controls.DialogButtonBox {
            Controls.Button {
                text: qsTr("Volver")
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
            }
            Controls.Button {
                text: qsTr("Ver conexión actual")
                enabled: dialog.activeDevice !== null
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.AcceptRole
            }
            onAccepted: {
                if (dialog.activeDevice)
                    dialog.openDevice(dialog.activeDevice);
                connectionLimit.close();
            }
            onRejected: connectionLimit.close()
        }
    }

    SyncKeysPicker {
        id: syncKeysPicker
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        host: dialog
        onKeysChosen: function (deviceId, fingerprints) {
            dialog.controller.enableVaultSync(deviceId, fingerprints);
        }
    }

    DeviceKeyPicker {
        id: keyPicker
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        host: dialog
        onKeyChosen: function (fingerprint, title) {
            const device = dialog.devices.find(candidate => String(candidate.deviceId) === keyPicker.deviceId);
            if (device && String(device.connectionGeneration) === keyPicker.connectionGeneration)
                dialog.approveTransfer(device, true, fingerprint, title);
        }
    }

    Controls.Dialog {
        id: syntheticTransferApproval
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        padding: Kirigami.Units.largeSpacing
        spacing: 0
        title: vaultIdentity ? qsTr("Enviar tu clave OpenPGP al teléfono") : qsTr("Confirmar envío de clave de prueba")
        width: Math.min(Kirigami.Units.gridUnit * 26, dialog.width - Kirigami.Units.gridUnit * 2)
        standardButtons: Controls.Dialog.NoButton
        property string deviceId: ""
        property string connectionGeneration: ""
        property string recipient: ""
        property string fingerprint: ""
        property string keyTitle: ""
        property bool approved: false
        property bool vaultIdentity: false
        readonly property var transferDevice: dialog.devices.find(device => String(device.deviceId || "") === deviceId) || ({})
        readonly property string transferPhase: String(transferDevice.syntheticTransferPhase || "ready")
        readonly property var chosenKey: dialog.keyStore.entries.find(key => String(key.id) === fingerprint) || null
        readonly property bool chosenKeyAvailable: !vaultIdentity || dialog.canTransferKey(chosenKey)
        readonly property bool canConfirm: transferDevice.connection === "connected" && String(transferDevice.connectionGeneration) === connectionGeneration && transferPhase === "ready" && chosenKeyAvailable && !dialog.controller.busy && !dialog.controller.syncBusy && !dialog.controller.maintenanceBusy && !dialog.controller.syntheticTransferPreparing
        readonly property bool transferPending: dialog.controller.syntheticTransferPreparing || ["waiting_acceptance", "sending", "waiting_receipt"].includes(transferPhase)

        onClosed: {
            if (approved && transferPending) {
                approved = false;
                console.info("Key transfer cancel from approval dialog closed");
                dialog.controller.cancelSyntheticKeyTransfer();
            }
        }
        onRejected: close()

        contentItem: Controls.ScrollView {
            id: approvalScroll
            contentWidth: availableWidth
            implicitHeight: Math.min(approvalContent.implicitHeight, Math.max(Kirigami.Units.gridUnit * 6, dialog.height - Kirigami.Units.gridUnit * 9))
            clip: true
            ColumnLayout {
                id: approvalContent
                width: approvalScroll.availableWidth
                spacing: Kirigami.Units.largeSpacing
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing
                    Controls.Label {
                        Layout.fillWidth: true
                        text: qsTr("Destinatario: %1").arg(syntheticTransferApproval.recipient)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: syntheticTransferApproval.vaultIdentity ? qsTr("Clave elegida: %1\nHuella completa; compárala con el teléfono:").arg(syntheticTransferApproval.keyTitle) : qsTr("Fingerprint completo de la identidad OpenPGP sintética:")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: syntheticTransferApproval.fingerprint
                        textFormat: Text.PlainText
                        font.family: "monospace"
                        wrapMode: Text.WrapAnywhere
                        Accessible.name: syntheticTransferApproval.vaultIdentity ? qsTr("Fingerprint completo de tu clave") : qsTr("Fingerprint completo de la clave de prueba")
                    }
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !syntheticTransferApproval.approved
                    type: Kirigami.MessageType.Warning
                    text: syntheticTransferApproval.vaultIdentity ? qsTr("Se enviará únicamente la clave privada elegida a este teléfono, después de que la aceptes allí. GnuPG pedirá su contraseña para exportarla y seguirá protegida con ella. No cambia la clave principal de la bóveda ni reemplaza una identidad existente en el teléfono. Si la sincronización de la bóveda está activada con este teléfono, también se sincronizarán con él los elementos que incluyan esta clave. Esta confirmación autoriza un único envío en esta sesión.") : qsTr("Solo se enviará la clave sintética de prueba, después de que Android la acepte. Esta confirmación autoriza una única transferencia para esta sesión.")
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !syntheticTransferApproval.approved && (syntheticTransferApproval.transferDevice.connection !== "connected" || String(syntheticTransferApproval.transferDevice.connectionGeneration) !== syntheticTransferApproval.connectionGeneration || (syntheticTransferApproval.vaultIdentity && !syntheticTransferApproval.chosenKeyAvailable))
                    type: Kirigami.MessageType.Warning
                    text: qsTr("La conexión o la clave cambió. Cierra esta confirmación y vuelve a elegir la clave.")
                }
                Controls.BusyIndicator {
                    Layout.alignment: Qt.AlignHCenter
                    visible: dialog.controller.syntheticTransferPreparing || syntheticTransferApproval.transferPending
                    running: visible
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: syntheticTransferApproval.approved
                    text: dialog.controller.syntheticTransferPreparing ? (syntheticTransferApproval.vaultIdentity ? qsTr("Exportando tu clave con GnuPG; escribe su contraseña si te la pide…") : qsTr("Preparando la clave de prueba en GnuPG…")) : (syntheticTransferApproval.vaultIdentity ? dialog.keyTransferLabel(syntheticTransferApproval.transferPhase) : dialog.syntheticTransferLabel(syntheticTransferApproval.transferPhase))
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
        }
        footer: Controls.DialogButtonBox {
            padding: syntheticTransferApproval.padding
            spacing: Kirigami.Units.smallSpacing * 2
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
                enabled: syntheticTransferApproval.approved ? !syntheticTransferApproval.transferPending : syntheticTransferApproval.canConfirm
                text: !syntheticTransferApproval.approved ? qsTr("Confirmar y enviar") : dialog.controller.syntheticTransferPreparing ? qsTr("Preparando…") : syntheticTransferApproval.transferPending ? qsTr("Enviando…") : syntheticTransferApproval.canConfirm ? qsTr("Reintentar") : qsTr("Volver al dispositivo")
                icon.name: syntheticTransferApproval.approved && !syntheticTransferApproval.transferPending ? (syntheticTransferApproval.canConfirm ? "view-refresh" : "go-previous") : "mail-send"
                // ActionRole keeps the dialog open: AcceptRole would close it, and closing
                // an approved, pending transfer cancels it in onClosed.
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
                onClicked: {
                    if (syntheticTransferApproval.approved && !syntheticTransferApproval.transferPending && !syntheticTransferApproval.canConfirm) {
                        dialog.selectedDeviceId = syntheticTransferApproval.deviceId;
                        syntheticTransferApproval.approved = false;
                        syntheticTransferApproval.close();
                        return;
                    }
                    if (!syntheticTransferApproval.canConfirm)
                        return;
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
