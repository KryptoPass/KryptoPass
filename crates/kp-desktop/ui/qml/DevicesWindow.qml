pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.ApplicationWindow {
    id: dialog
    objectName: "devicesWindow"
    required property var controller
    required property var keyStore
    // Desktop only exchanges changes with phones while its vault is unlocked.
    property bool vaultLocked: false
    readonly property bool opened: visible

    signal copyRequested(string code)

    property bool networkOptionsVisible: false
    property bool showInvitationCode: false
    property bool reconnectFlow: false
    property string selectedDeviceId: ""
    readonly property var selectedDevice: devices.find(device => String(device.deviceId) === selectedDeviceId) || null
    readonly property bool showingDetail: selectedDevice !== null
    readonly property var activeDevice: devices.find(device => String(device.deviceId) === controller.activeDeviceId) || null
    readonly property string activeDeviceName: activeDevice ? deviceName(activeDevice) : KI18n.i18n("another device")
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
    readonly property bool canPair: !controller.busy && !controller.sessionActive && !controller.awaiting

    flags: Qt.Dialog | Qt.WindowTitleHint | Qt.WindowSystemMenuHint | Qt.WindowCloseButtonHint | Qt.WindowMaximizeButtonHint
    modality: Qt.WindowModal
    width: Kirigami.Units.gridUnit * 34
    height: Kirigami.Units.gridUnit * 36
    minimumWidth: Kirigami.Units.gridUnit * 24
    minimumHeight: Kirigami.Units.gridUnit * 22
    title: KI18n.i18nc("@title:window", "Devices")

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
            return KI18n.i18n("Reviewing the offered key…");
        case "waiting_decision":
            return KI18n.i18nc("@info key transfer status", "Waiting for your decision");
        case "receiving":
            return KI18n.i18nc("@info key transfer status", "Receiving (%1 of %2 bytes)", transfer.received || 0, transfer.total || 0);
        case "importing":
            return KI18n.i18nc("@info key transfer status", "Importing into GnuPG");
        case "verifying":
            return KI18n.i18nc("@info key transfer status", "Checking the challenge");
        case "complete":
            return KI18n.i18nc("@info key transfer status", "Complete");
        case "rejected":
            return KI18n.i18nc("@info key transfer status", "Rejected");
        case "cancelled":
            return KI18n.i18nc("@info key transfer status", "Cancelled by the phone");
        case "expired":
            return KI18n.i18nc("@info key status", "Expired");
        case "failed":
            return KI18n.i18nc("@info key transfer status", "Error");
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
        return String(device.alias || "").length > 0 ? String(device.alias) : String(device.displayName || KI18n.i18nc("@label fallback device name", "Device"));
    }

    function connectionLabel(state) {
        switch (state) {
        case "connected":
            return KI18n.i18nc("@info connection status", "Connected");
        case "connecting":
            return KI18n.i18nc("@info connection status", "Connecting");
        case "reconnecting":
            return KI18n.i18nc("@info connection status", "Reconnecting");
        case "failed":
            return KI18n.i18nc("@info connection status", "Connection interrupted");
        default:
            return KI18n.i18nc("@info connection status", "Disconnected");
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
            return KI18n.i18nc("@info key transfer status", "Ready to send the chosen key");
        case "unsupported":
            return KI18n.i18nc("@info key transfer status", "This device cannot receive the key");
        case "waiting_acceptance":
            return KI18n.i18nc("@info key transfer status", "Waiting for you to accept on the phone");
        case "sending":
            return KI18n.i18nc("@info key transfer status", "Sending your key");
        case "waiting_receipt":
            return KI18n.i18nc("@info key transfer status", "Waiting for the phone to store it");
        case "complete":
            return KI18n.i18nc("@info key transfer status", "The phone stored your key and checked that it can decrypt");
        default:
            return syntheticTransferLabel(phase);
        }
    }

    function syntheticTransferLabel(phase) {
        switch (phase) {
        case "ready":
            return KI18n.i18nc("@info key transfer status", "Transfer test available");
        case "waiting_acceptance":
            return KI18n.i18nc("@info key transfer status", "Waiting for Android to accept");
        case "sending":
            return KI18n.i18nc("@info key transfer status", "Sending test key");
        case "waiting_receipt":
            return KI18n.i18nc("@info key transfer status", "Waiting for storage confirmation");
        case "complete":
            return KI18n.i18nc("@info key transfer status", "Android confirmed storage and the challenge");
        case "rejected":
            return KI18n.i18nc("@info key transfer status", "Android rejected the offer");
        case "cancelled":
            return KI18n.i18nc("@info key transfer status", "Transfer cancelled");
        case "expired":
            return KI18n.i18nc("@info key transfer status", "The transfer expired");
        case "uncertain":
            return KI18n.i18nc("@info key transfer status", "Connection lost; the result is uncertain");
        case "failed":
            return KI18n.i18nc("@info key transfer status", "The transfer failed");
        default:
            return KI18n.i18nc("@info key transfer status", "This session does not support the test");
        }
    }

    function routeLine(device) {
        const family = ({
                ipv4: "IPv4",
                ipv6: "IPv6",
                mixed: "IPv4 / IPv6"
            })[device.addressFamily] || "";
        const route = String(device.routeLabel || KI18n.i18n("Checking route…"));
        return family ? route + " · " + family : route;
    }

    function hintForStatus(code) {
        switch (code) {
        case "waiting_peer":
            return dialog.reconnectFlow ? KI18n.i18n("Scan the QR code and choose Reconnect on Android. The connection code expires in 5 minutes.") : KI18n.i18n("Scan the QR code and choose Pair Device on Android. The connection code expires in 5 minutes.");
        case "awaiting_sas":
            return KI18n.i18n("Compare the seven emoji and their names with Android. Confirm only if all of them match on both screens.");
        case "confirming":
            return KI18n.i18n("The other screen is confirming the comparison.");
        case "reconnecting":
            return KI18n.i18n("Trying to recover the authenticated session with this device.");
        case "closing":
            return KI18n.i18n("Closing the link. The device stays paired.");
        case "expired":
            return KI18n.i18n("The invitation expired. Create a new one to try again.");
        case "trust_error":
            return KI18n.i18n("The saved trust was not changed. Check the profile and reload the devices.");
        case "profile_error":
            return KI18n.i18n("Reload the devices. If the problem continues, check the application log.");
        case "keystore_error":
            return KI18n.i18n("Check that the system keyring is available and try again.");
        case "authentication_error":
            return KI18n.i18n("The device was not authenticated. Trust was not extended; create a new code to retry.");
        case "rejected":
            return KI18n.i18n("The pairing was rejected and no trust was granted.");
        case "cancelled":
            return KI18n.i18n("The operation was cancelled. The trust already saved is kept.");
        case "transport_error":
            return dialog.controller.internetEnabled ? KI18n.i18n("Check the Internet access and try again.") : KI18n.i18n("Check that both devices are on the same network and try again.");
        case "disconnected":
            return KI18n.i18n("The session is disconnected. The saved trust is kept; you can reconnect the device.");
        case "connected":
            return KI18n.i18n("The authenticated session is active. Closing this window does not disconnect it.");
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
                text: KI18n.i18nc("@action:button", "Back to Devices")
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
                    text: dialog.showingDetail ? dialog.deviceName(dialog.selectedDevice) : KI18n.i18nc("@title", "Your devices")
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: dialog.showingDetail ? KI18n.i18n("Device connection and options") : KI18n.i18n("Pair and connect your devices")
                    textFormat: Text.PlainText
                    color: Kirigami.Theme.disabledTextColor
                    elide: Text.ElideRight
                }
            }
            Controls.ToolButton {
                id: moreButton
                icon.name: "overflow-menu"
                text: KI18n.i18nc("@action:button", "More Options")
                display: Controls.AbstractButton.IconOnly
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: text
                onClicked: moreMenu.popup(moreButton, 0, moreButton.height)
                Controls.Menu {
                    id: moreMenu
                    Controls.MenuItem {
                        text: KI18n.i18nc("@action:inmenu", "Connection Options")
                        icon.name: "network-wired"
                        checkable: true
                        checked: dialog.networkOptionsVisible
                        onTriggered: {
                            dialog.selectedDeviceId = "";
                            dialog.networkOptionsVisible = checked;
                        }
                    }
                    Controls.MenuItem {
                        text: KI18n.i18nc("@action:inmenu", "Refresh Devices and Networks")
                        icon.name: "view-refresh"
                        enabled: !dialog.controller.busy && !dialog.controller.awaiting
                        onTriggered: {
                            dialog.controller.loadDevices();
                            dialog.controller.loadNetworkAddresses();
                        }
                    }
                    Controls.MenuSeparator {}
                    Controls.MenuItem {
                        text: KI18n.i18nc("@action:inmenu", "Delete All Pairings…")
                        icon.name: "edit-clear-all"
                        enabled: !dialog.controller.busy && !dialog.controller.aliasBusy
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
                text: dialog.reconnectFlow ? KI18n.i18nc("@action:button", "Reconnect Device") : KI18n.i18nc("@action:button", "Pair Device")
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
                        Accessible.name: KI18n.i18n("Local pairing QR code")
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
                        text: dialog.controller.internetEnabled && dialog.controller.lanEnabled ? KI18n.i18n("The phone tries the local network first and, if it cannot reach it, uses the Internet.") : dialog.controller.internetEnabled ? KI18n.i18n("Both devices need Internet access.") : (dialog.controller.lanEnabled ? KI18n.i18n("Both devices must be on the same local network.") : KI18n.i18n("Emulator mode: only for Android on this computer."))
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.ToolButton {
                        text: dialog.showInvitationCode ? KI18n.i18nc("@action:button", "Hide Code") : KI18n.i18nc("@action:button", "Use a Code Instead of QR")
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
                    Accessible.name: KI18n.i18n("One-time invitation code")
                }
                Controls.Button {
                    text: KI18n.i18nc("@action:button", "Copy")
                    icon.name: "edit-copy"
                    onClicked: dialog.copyRequested(dialog.controller.bootstrap)
                }
            }

            // SAS comparison: the human decision, never a default action.
            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.comparing
                text: KI18n.i18nc("@title", "Compare the seven items")
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
                                Accessible.name: KI18n.i18nc("@info SAS emoji position and emoji", "Item %1: %2", sasCell.index + 1, String(sasCell.modelData.emoji || ""))
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
                text: dialog.sasReady ? KI18n.i18n("If a single emoji or name does not match, reject the pairing.") : KI18n.i18n("The full comparison could not be shown. Reject the pairing and try again.")
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
                    text: KI18n.i18nc("@action:button", "They Do Not Match")
                    icon.name: "dialog-cancel"
                    onClicked: dialog.controller.decide(false)
                }
                Controls.Button {
                    text: KI18n.i18nc("@action:button", "All 7 Match")
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
                text: KI18n.i18nc("@title:group", "Connection privacy")
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
                    text: KI18n.i18nc("@option:radio", "Automatic (recommended)")
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
                    text: KI18n.i18n("Uses the route the system chooses, including your VPN if it is connected.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.RadioButton {
                    id: privacyVpn
                    Layout.fillWidth: true
                    text: KI18n.i18nc("@option:radio", "Only through a VPN")
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
                    text: KI18n.i18n("The Internet is only used while the system sends it through a VPN. If the VPN disconnects, the connection pauses.")
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
                            return KI18n.i18n("VPN connected: the Internet connection goes through it.");
                        case 3:
                            return KI18n.i18n("No VPN is connected. Internet connections are paused until you connect it.");
                        default:
                            return KI18n.i18n("This system does not allow checking the VPN, so Internet connections are paused.");
                        }
                    }
                }
                Controls.CheckBox {
                    Layout.fillWidth: true
                    Layout.leftMargin: privacyVpn.indicator.width + privacyVpn.spacing
                    visible: dialog.controller.requireVpn
                    text: KI18n.i18nc("@option:check", "Also allow the local network")
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
                    text: KI18n.i18nc("@option:check", "Hide my IP address from the other device")
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
                    text: KI18n.i18n("Everything goes through the relay server. It is a bit slower and disables the local network.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.networkOptionsVisible
                text: KI18n.i18nc("@title:group", "How to pair a new device")
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
                    text: KI18n.i18nc("@option:radio", "Automatic: local network and, if it cannot reach it, the Internet")
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
                    text: KI18n.i18nc("@option:radio", "Internet only")
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
                    placeholderText: KI18n.i18nc("@info:placeholder", "Connection server (wss://…)")
                    Accessible.name: KI18n.i18n("Internet connection server")
                    onEditingFinished: dialog.controller.configureInternet(true, text)
                }
                Controls.RadioButton {
                    id: pairByLan
                    Layout.fillWidth: true
                    text: KI18n.i18nc("@option:radio", "Local network only")
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
                    text: dialog.controller.relayOnly ? KI18n.i18n("Unavailable while you hide your IP address.") : KI18n.i18n("Unavailable: enable “Also allow the local network” in Privacy.")
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
                        Accessible.name: KI18n.i18n("This computer's local network")
                        onActivated: index => dialog.controller.configureNetwork(true, index)
                    }
                    Controls.ToolButton {
                        icon.name: "view-refresh"
                        text: KI18n.i18nc("@action:button", "Refresh Networks")
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
                    text: KI18n.i18n("No local network was found. Connect to Wi-Fi or Ethernet and refresh the networks.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.RadioButton {
                    id: pairByEmulator
                    Layout.fillWidth: true
                    text: KI18n.i18nc("@option:radio", "Android emulator on this computer (advanced)")
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
                text: KI18n.i18nc("@title:group", "Authorized devices")
            }
            Controls.Label {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: dialog.devices.length === 0 && dialog.controller.busy
                text: KI18n.i18n("Loading authorized devices…")
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
                            text: KI18n.i18n("More actions for %1", dialog.deviceName(deviceRow.modelData))
                            display: Controls.AbstractButton.IconOnly
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.text: KI18n.i18nc("@info:tooltip", "More actions")
                            onClicked: deviceMenu.popup(deviceMenuButton, 0, deviceMenuButton.height)
                            Controls.Menu {
                                id: deviceMenu
                                Controls.MenuItem {
                                    text: KI18n.i18nc("@action:inmenu", "View Details and Options")
                                    icon.name: "document-properties"
                                    onTriggered: dialog.openDevice(deviceRow.modelData)
                                }
                                Controls.MenuItem {
                                    text: deviceRow.modelData.isActive ? KI18n.i18nc("@action", "Disconnect") : KI18n.i18nc("@action", "Reconnect")
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
                                    text: KI18n.i18nc("@action:inmenu", "Rename…")
                                    icon.name: "edit-rename"
                                    enabled: !dialog.controller.aliasBusy && !dialog.controller.busy
                                    onTriggered: dialog.openAliasEditor(deviceRow.modelData)
                                }
                                Controls.MenuSeparator {}
                                Controls.MenuItem {
                                    text: KI18n.i18nc("@action", "Forget Device…")
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
                text: KI18n.i18nc("@title placeholder", "No paired devices")
                explanation: KI18n.i18n("Pair KryptoPass for Android to keep a secure connection with this computer.")
                helpfulAction: Kirigami.Action {
                    text: KI18n.i18nc("@action:button", "Pair Device")
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
                text: KI18n.i18nc("@action:button", "Pair Device")
                visible: !dialog.showingDetail
                icon.name: "network-connect"
                enabled: dialog.canPair || dialog.controller.sessionActive
                onClicked: dialog.startPairing()
            }
            Controls.Button {
                enabled: !dialog.controller.maintenanceBusy
                text: dialog.controller.busy ? KI18n.i18nc("@action:button", "Cancel") : KI18n.i18nc("@action:button", "Close")
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
        title: dialog.resetRequested ? KI18n.i18nc("@title:window", "Delete All Pairings?") : KI18n.i18nc("@title:window", "Forget Device?")
        width: Math.min(Kirigami.Units.gridUnit * 24, dialog.width - Kirigami.Units.gridUnit * 2)
        standardButtons: Controls.Dialog.NoButton
        contentItem: Controls.Label {
            text: dialog.resetRequested ? KI18n.i18n("The connection will close, and this pairing profile's private identity, its authorizations and local names will be deleted. The vault and the OpenPGP keys are kept. To pair again, compare a new SAS.") : KI18n.i18n("The connection will close, and the authorization and local name of “%1” will be removed on this computer. Also forget this computer on Android before pairing them again. The vault and OpenPGP are kept.", dialog.forgetTargetName)
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
        }
        footer: Controls.DialogButtonBox {
            Controls.Button {
                text: KI18n.i18nc("@action:button", "Cancel")
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
            }
            Controls.Button {
                text: dialog.resetRequested ? KI18n.i18nc("@action:button", "Delete All Pairings") : KI18n.i18nc("@action:button", "Forget Device")
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
        title: KI18n.i18nc("@title:window", "One Connection at a Time")
        width: Math.min(Kirigami.Units.gridUnit * 24, dialog.width - Kirigami.Units.gridUnit * 2)
        contentItem: Controls.Label {
            text: KI18n.i18n("You are now connected to %1. To connect another device, open this connection and press Disconnect. Your devices stay paired.", dialog.activeDeviceName)
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        footer: Controls.DialogButtonBox {
            Controls.Button {
                text: KI18n.i18nc("@action:button dismiss", "Back")
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
            }
            Controls.Button {
                text: KI18n.i18nc("@action:button", "View Current Connection")
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
        title: vaultIdentity ? KI18n.i18nc("@title:window", "Send Your OpenPGP Key to the Phone") : KI18n.i18nc("@title:window", "Confirm Sending the Test Key")
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
                        text: KI18n.i18n("Recipient: %1", syntheticTransferApproval.recipient)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: syntheticTransferApproval.vaultIdentity ? KI18n.i18n("Chosen key: %1\nFull fingerprint; compare it with the phone:", syntheticTransferApproval.keyTitle) : KI18n.i18n("Full fingerprint of the synthetic OpenPGP identity:")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: syntheticTransferApproval.fingerprint
                        textFormat: Text.PlainText
                        font.family: "monospace"
                        wrapMode: Text.WrapAnywhere
                        Accessible.name: syntheticTransferApproval.vaultIdentity ? KI18n.i18n("Full fingerprint of your key") : KI18n.i18n("Full fingerprint of the test key")
                    }
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !syntheticTransferApproval.approved
                    type: Kirigami.MessageType.Warning
                    text: syntheticTransferApproval.vaultIdentity ? KI18n.i18n("Only the chosen private key will be sent to this phone, after you accept it there. GnuPG will ask for its passphrase to export it, and it stays protected by it. It does not change the vault's main key or replace an existing identity on the phone. If vault sync is enabled with this phone, items that include this key will also sync with it. This confirmation authorizes a single transfer in this session.") : KI18n.i18n("Only the synthetic test key will be sent, after Android accepts it. This confirmation authorizes a single transfer for this session.")
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !syntheticTransferApproval.approved && (syntheticTransferApproval.transferDevice.connection !== "connected" || String(syntheticTransferApproval.transferDevice.connectionGeneration) !== syntheticTransferApproval.connectionGeneration || (syntheticTransferApproval.vaultIdentity && !syntheticTransferApproval.chosenKeyAvailable))
                    type: Kirigami.MessageType.Warning
                    text: KI18n.i18n("The connection or the key changed. Close this confirmation and choose the key again.")
                }
                Controls.BusyIndicator {
                    Layout.alignment: Qt.AlignHCenter
                    visible: dialog.controller.syntheticTransferPreparing || syntheticTransferApproval.transferPending
                    running: visible
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: syntheticTransferApproval.approved
                    text: dialog.controller.syntheticTransferPreparing ? (syntheticTransferApproval.vaultIdentity ? KI18n.i18n("Exporting your key with GnuPG; enter its passphrase if asked…") : KI18n.i18n("Preparing the test key in GnuPG…")) : (syntheticTransferApproval.vaultIdentity ? dialog.keyTransferLabel(syntheticTransferApproval.transferPhase) : dialog.syntheticTransferLabel(syntheticTransferApproval.transferPhase))
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
                text: syntheticTransferApproval.approved ? (syntheticTransferApproval.transferPending ? KI18n.i18nc("@action:button", "Cancel Transfer") : KI18n.i18nc("@action:button", "Close")) : KI18n.i18nc("@action:button", "Cancel")
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
                text: !syntheticTransferApproval.approved ? KI18n.i18nc("@action:button", "Confirm and Send") : dialog.controller.syntheticTransferPreparing ? KI18n.i18n("Preparing…") : syntheticTransferApproval.transferPending ? KI18n.i18n("Sending…") : syntheticTransferApproval.canConfirm ? KI18n.i18nc("@action:button", "Retry") : KI18n.i18nc("@action:button", "Back to Device")
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
        title: KI18n.i18nc("@title:window", "Receive Your OpenPGP Key from the Phone")
        width: Math.min(Kirigami.Units.gridUnit * 28, dialog.width - Kirigami.Units.gridUnit * 2)
        standardButtons: Controls.Dialog.NoButton
        readonly property var transfer: dialog.inboundTransfer
        readonly property string phase: String(transfer.phase || "")
        readonly property bool working: ["assessing", "importing", "verifying"].includes(phase)

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.mediumSpacing
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18n("From: %1", String(inboundKeyDialog.transfer.deviceName || ""))
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18n("Full fingerprint of the offered key. Compare it with the one the phone shows:")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: String(inboundKeyDialog.transfer.fingerprint || "")
                textFormat: Text.PlainText
                font.family: "monospace"
                wrapMode: Text.WrapAnywhere
                Accessible.name: KI18n.i18n("Full fingerprint of the offered key")
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
                text: KI18n.i18n("Only the passphrase-protected private key with this fingerprint will be accepted. After importing it, GnuPG may ask for its passphrase to check that it works.")
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
                text: KI18n.i18nc("@action:button", "Reject")
                icon.name: "dialog-cancel"
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
                onClicked: dialog.controller.rejectInboundKeyTransfer()
            }
            Controls.Button {
                visible: inboundKeyDialog.transfer.canDecide === true
                text: KI18n.i18nc("@action:button", "Accept and Import")
                icon.name: "document-import"
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
                onClicked: dialog.controller.acceptInboundKeyTransfer()
            }
            Controls.Button {
                visible: inboundKeyDialog.transfer.finished === true
                text: KI18n.i18nc("@action:button", "Close")
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
        title: KI18n.i18nc("@title:window", "Device Name")
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
                placeholderText: KI18n.i18nc("@title:window", "Device Name")
                Accessible.name: KI18n.i18n("Local device name")
                onTextEdited: dialog.aliasDraft = text
                onAccepted: {
                    dialog.aliasDraft = text;
                    dialog.saveAlias();
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18n("It is stored only on this computer.")
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
                text: KI18n.i18nc("@action:button", "Remove Name")
                enabled: !dialog.controller.aliasBusy
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ResetRole
                onClicked: {
                    dialog.aliasDraft = "";
                    aliasField.text = "";
                    dialog.saveAlias();
                }
            }
            Controls.Button {
                text: KI18n.i18nc("@action:button", "Cancel")
                enabled: !dialog.controller.aliasBusy
                Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
                onClicked: aliasDialog.close()
            }
            Controls.Button {
                text: dialog.controller.aliasBusy ? KI18n.i18n("Saving…") : KI18n.i18nc("@action:button", "Save")
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
