pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.ScrollView {
    id: deviceRow
    required property var host
    required property var modelData
    readonly property string deviceId: String(modelData.deviceId || "")
    readonly property string connection: String(modelData.connection || "")
    readonly property string connectionGeneration: String(modelData.connectionGeneration || "")
    readonly property string syntheticTransferPhase: String(modelData.syntheticTransferPhase || "unsupported")
    readonly property int keyTransferVersion: Number(modelData.keyTransferVersion || 0)
    readonly property int keyTransferUnavailableReason: Number(modelData.keyTransferUnavailableReason || 0)
    readonly property string keyTransferUnavailableText: keyTransferUnavailableReason === 1 ? KI18n.i18n("The phone has no fingerprint registered. Register it in its settings to receive the key.") : keyTransferUnavailableReason === 2 ? KI18n.i18n("The phone already stores the maximum number of keys.") : keyTransferUnavailableReason === 3 ? KI18n.i18n("The phone cannot receive keys right now.") : KI18n.i18n("Update KryptoPass on the phone to receive the key.")
    readonly property var syncedKeys: host.vaultSyncKeyIds(deviceId)
    readonly property var syncKeyRows: keyList.syncKeyRows
    readonly property bool vaultSyncOn: host.controller.vaultSyncPeers.indexOf(deviceId) >= 0
    readonly property bool transferPending: (modelData.isActive && host.controller.syntheticTransferPreparing) || ["waiting_acceptance", "sending", "waiting_receipt"].includes(syntheticTransferPhase)
    readonly property bool canSend: connection === "connected" && keyTransferVersion >= 2 && syntheticTransferPhase === "ready" && !host.controller.syntheticTransferPreparing && !host.controller.syncBusy && !host.controller.busy && !host.controller.maintenanceBusy
    readonly property bool transferEnded: ["complete", "rejected", "cancelled", "expired", "failed", "uncertain"].includes(syntheticTransferPhase)
    readonly property bool canRecoverTransfer: !transferPending && !host.controller.busy && !host.controller.syncBusy && !host.controller.maintenanceBusy && host.controller.statusCode !== "closing" && ((connection === "connected" && transferEnded) || connection === "disconnected" || connection === "failed")
    property bool technicalOpen: false
    property bool peersExpanded: false
    readonly property int index: host.devices.findIndex(device => String(device.deviceId) === deviceId)
    onDeviceIdChanged: {
        technicalOpen = false;
        peersExpanded = false;
    }
    contentWidth: availableWidth
    clip: true

    ColumnLayout {
        width: deviceRow.availableWidth
        spacing: Kirigami.Units.largeSpacing

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            visible: deviceRow.host.statusIsProblem
            type: Kirigami.MessageType.Error
            text: deviceRow.host.controller.status + "\n" + deviceRow.host.statusHint
        }
        Controls.Frame {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.largeSpacing
            Layout.topMargin: Kirigami.Units.largeSpacing
            padding: Kirigami.Units.largeSpacing
            contentItem: RowLayout {
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    source: "smartphone"
                    fallback: "computer"
                    implicitWidth: Kirigami.Units.iconSizes.large
                    implicitHeight: Kirigami.Units.iconSizes.large
                    Accessible.ignored: true
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Heading {
                        Layout.fillWidth: true
                        level: 3
                        text: deviceRow.host.deviceName(deviceRow.modelData)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Rectangle {
                            implicitWidth: 8
                            implicitHeight: 8
                            radius: width / 2
                            color: deviceRow.host.connectionColor(deviceRow.connection)
                            Accessible.ignored: true
                        }
                        Controls.Label {
                            text: deviceRow.host.connectionLabel(deviceRow.connection)
                            color: deviceRow.host.connectionColor(deviceRow.connection)
                            textFormat: Text.PlainText
                        }
                        Controls.Label {
                            text: KI18n.i18nc("@label device status suffix", "· Paired")
                            color: Kirigami.Theme.disabledTextColor
                        }
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: deviceRow.connection === "connected"
                        text: deviceRow.host.routeLine(deviceRow.modelData)
                        color: Kirigami.Theme.disabledTextColor
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                }
                Controls.ToolButton {
                    icon.name: "edit-rename"
                    text: KI18n.i18nc("@action:button", "Rename")
                    display: Controls.AbstractButton.IconOnly
                    enabled: !deviceRow.host.controller.aliasBusy && !deviceRow.host.controller.busy
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.text: text
                    onClicked: deviceRow.host.openAliasEditor(deviceRow.modelData)
                }
            }
        }

        Controls.Frame {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.largeSpacing
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                RowLayout {
                    Layout.fillWidth: true
                    Kirigami.Icon {
                        source: "folder-sync"
                        implicitWidth: Kirigami.Units.iconSizes.smallMedium
                        implicitHeight: Kirigami.Units.iconSizes.smallMedium
                        Accessible.ignored: true
                    }
                    Kirigami.Heading {
                        Layout.fillWidth: true
                        level: 4
                        text: KI18n.i18nc("@title:group", "Vault sync")
                        wrapMode: Text.Wrap
                    }
                    Controls.Switch {
                        checked: deviceRow.vaultSyncOn
                        enabled: !deviceRow.host.controller.busy && !deviceRow.host.controller.maintenanceBusy
                        Accessible.name: KI18n.i18n("Sync the vault with this device")
                        onToggled: {
                            deviceRow.host.setVaultSync(deviceRow.deviceId, checked);
                            checked = Qt.binding(() => deviceRow.vaultSyncOn);
                        }
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: deviceRow.vaultSyncOn ? (deviceRow.connection === "connected" ? KI18n.i18n("On · syncs while the vault is unlocked.") : KI18n.i18n("On · waiting for the device to connect.")) : KI18n.i18n("Shares this vault's items. The phone needs its OpenPGP key and Sync turned on.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                SyncKeyList {
                    id: keyList
                    Layout.fillWidth: true
                    visible: deviceRow.vaultSyncOn
                    host: deviceRow.host
                    deviceId: deviceRow.deviceId
                    device: deviceRow.modelData
                }
                Kirigami.InlineMessage {
                    id: newPhoneKeyMessage
                    readonly property var offered: deviceRow.syncKeyRows.filter(row => row.canAdd && row.state === "available")
                    Layout.fillWidth: true
                    visible: deviceRow.vaultSyncOn && offered.length > 0
                    type: Kirigami.MessageType.Information
                    text: offered.length > 0 ? KI18n.i18n("The phone has %1's key, but it does not sync. Until you add it, only items with keys that do sync open there, even if that key has access to them.", offered.map(row => row.title).join(", ")) : ""
                    actions: [
                        Kirigami.Action {
                            text: newPhoneKeyMessage.offered.length > 0 ? KI18n.i18nc("@action:button", "Sync %1's Key", newPhoneKeyMessage.offered[0].title) : ""
                            icon.name: "list-add"
                            enabled: !deviceRow.host.controller.busy && newPhoneKeyMessage.offered.length > 0
                            onTriggered: deviceRow.host.controller.enableVaultSync(deviceRow.deviceId, deviceRow.syncedKeys.concat([newPhoneKeyMessage.offered[0].id]).join(","))
                        }
                    ]
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: deviceRow.vaultSyncOn && deviceRow.host.vaultLocked
                    type: Kirigami.MessageType.Information
                    text: KI18n.i18n("The Desktop vault is locked. Changes keep arriving encrypted and can be seen once you unlock it.")
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: deviceRow.vaultSyncOn && deviceRow.host.controller.vaultSyncLockedKey.length > 0
                    type: Kirigami.MessageType.Information
                    text: KI18n.i18n("Sync continues, but Desktop cannot open what arrives encrypted for %1's key until you unlock it.", deviceRow.host.keyTitle(deviceRow.host.controller.vaultSyncLockedKey))
                    actions: [
                        Kirigami.Action {
                            text: KI18n.i18nc("@action:button", "Unlock Key…")
                            icon.name: "dialog-password"
                            onTriggered: deviceRow.host.controller.unlockSyncKey(deviceRow.host.controller.vaultSyncLockedKey)
                        }
                    ]
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: deviceRow.host.syncMessageDeviceId === deviceRow.deviceId && deviceRow.host.controller.vaultSyncMessage.length > 0
                    type: deviceRow.host.controller.vaultSyncFailed ? Kirigami.MessageType.Error : Kirigami.MessageType.Information
                    text: deviceRow.host.controller.vaultSyncMessage
                }
            }
        }

        Controls.Frame {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.largeSpacing
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 4
                    text: KI18n.i18nc("@title", "OpenPGP key")
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: KI18n.i18n("Choose one of your private keys and review the recipient before sending it. It stays protected by its passphrase.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Button {
                    text: deviceRow.connection === "disconnected" || deviceRow.connection === "failed" ? KI18n.i18nc("@action:button", "Reconnect to Send") : deviceRow.transferEnded ? KI18n.i18nc("@action:button", "Disconnect to Retry") : KI18n.i18nc("@action:button", "Choose Key to Send…")
                    icon.name: deviceRow.canSend ? "document-send" : deviceRow.connection === "connected" && deviceRow.transferEnded ? "network-disconnect" : "network-connect"
                    enabled: deviceRow.canSend || deviceRow.canRecoverTransfer
                    onClicked: {
                        if (deviceRow.connection === "disconnected" || deviceRow.connection === "failed")
                            deviceRow.host.reconnectDevice(deviceRow.modelData);
                        else if (deviceRow.transferEnded)
                            deviceRow.host.controller.disconnectPeer(deviceRow.deviceId);
                        else
                            deviceRow.host.chooseKey(deviceRow.modelData);
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !deviceRow.canSend && !deviceRow.transferPending
                    text: deviceRow.connection !== "connected" ? KI18n.i18n("Connect this device to send the key.") : deviceRow.transferEnded ? KI18n.i18n("To prepare another transfer, disconnect and reconnect this device. The pairing is kept.") : deviceRow.keyTransferVersion < 2 ? deviceRow.keyTransferUnavailableText : deviceRow.syntheticTransferPhase === "ready" ? KI18n.i18n("Wait for the current operation to finish.") : deviceRow.host.keyTransferLabel(deviceRow.syntheticTransferPhase)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                RowLayout {
                    Layout.fillWidth: true
                    visible: deviceRow.host.keyTransferDeviceId === deviceRow.deviceId && deviceRow.host.keyTransferVault && deviceRow.transferPending
                    Controls.BusyIndicator {
                        running: parent.visible
                        implicitWidth: Kirigami.Units.iconSizes.smallMedium
                        implicitHeight: implicitWidth
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: deviceRow.host.controller.syntheticTransferPreparing ? KI18n.i18n("Preparing the key…") : deviceRow.host.keyTransferLabel(deviceRow.syntheticTransferPhase)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Button {
                        text: KI18n.i18nc("@action:button", "Cancel Sending")
                        icon.name: "dialog-cancel"
                        onClicked: deviceRow.host.controller.cancelSyntheticKeyTransfer()
                    }
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: deviceRow.host.keyTransferDeviceId === deviceRow.deviceId && deviceRow.host.keyTransferVault && deviceRow.host.controller.syntheticTransferError.length > 0
                    type: Kirigami.MessageType.Error
                    text: deviceRow.host.controller.syntheticTransferError
                }
            }
        }

        Controls.ToolButton {
            Layout.leftMargin: Kirigami.Units.largeSpacing
            text: KI18n.i18nc("@title:group", "Connection details")
            icon.name: deviceRow.technicalOpen ? "arrow-down" : "arrow-right"
            checkable: true
            checked: deviceRow.technicalOpen
            onClicked: deviceRow.technicalOpen = checked
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing * 2
            Layout.rightMargin: Kirigami.Units.largeSpacing
            visible: deviceRow.technicalOpen
            spacing: Kirigami.Units.smallSpacing
            Controls.Label {
                text: KI18n.i18nc("@label", "Device identifier")
                color: Kirigami.Theme.disabledTextColor
            }
            Controls.Label {
                Layout.fillWidth: true
                text: deviceRow.deviceId
                textFormat: Text.PlainText
                font.family: "monospace"
                wrapMode: Text.WrapAnywhere
            }
            Controls.ToolButton {
                text: KI18n.i18nc("@action:button", "Copy Identifier")
                icon.name: "edit-copy"
                onClicked: deviceRow.host.copyRequested(deviceRow.deviceId)
            }
        }

        Controls.Frame {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.largeSpacing
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: KI18n.i18n("Disconnecting keeps the pairing. Forgetting removes this device's permission on this computer.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Flow {
                    id: deviceActions
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.largeSpacing
                    Controls.Button {
                        text: deviceRow.modelData.isActive ? KI18n.i18nc("@action", "Disconnect") : KI18n.i18nc("@action", "Reconnect")
                        icon.name: deviceRow.modelData.isActive ? "network-disconnect" : "network-connect"
                        Kirigami.Theme.textColor: deviceRow.modelData.isActive ? deviceActions.Kirigami.Theme.negativeTextColor : deviceActions.Kirigami.Theme.linkColor
                        enabled: !deviceRow.host.controller.busy && !deviceRow.host.controller.syncBusy && !deviceRow.host.controller.maintenanceBusy && deviceRow.host.controller.statusCode !== "closing"
                        onClicked: {
                            if (deviceRow.modelData.isActive)
                                deviceRow.host.controller.disconnectPeer(deviceRow.deviceId);
                            else
                                deviceRow.host.reconnectDevice(deviceRow.modelData);
                        }
                    }
                    Controls.Button {
                        text: KI18n.i18nc("@action", "Forget Device…")
                        icon.name: "edit-delete"
                        Kirigami.Theme.textColor: deviceActions.Kirigami.Theme.negativeTextColor
                        enabled: !deviceRow.host.controller.busy && !deviceRow.host.controller.aliasBusy
                        onClicked: deviceRow.host.forgetDevice(deviceRow.modelData)
                    }
                }
            }
        }

        Controls.ToolButton {
            Layout.leftMargin: Kirigami.Units.largeSpacing
            text: KI18n.i18nc("@item folder icon", "Development")
            icon.name: deviceRow.peersExpanded ? "arrow-down" : "arrow-right"
            checkable: true
            checked: deviceRow.peersExpanded
            onClicked: deviceRow.peersExpanded = checked
        }
        ColumnLayout {
            id: diagnostics
            Layout.fillWidth: true
            visible: deviceRow.peersExpanded
            spacing: Kirigami.Units.largeSpacing
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: deviceRow.host.controller.syntheticTransferFingerprint.length > 0 && deviceRow.modelData.trust === "trusted"
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    text: KI18n.i18n("Debug test with a protected synthetic key. It never uses or sends real OpenPGP keys.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Flow {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing
                    Controls.Button {
                        visible: deviceRow.connection === "connected" && deviceRow.syntheticTransferPhase === "ready"
                        text: KI18n.i18nc("@action:button", "Send Test Key")
                        icon.name: "mail-send"
                        enabled: !deviceRow.host.controller.syntheticTransferPreparing && !deviceRow.host.controller.syncBusy && !deviceRow.host.controller.busy && !deviceRow.host.controller.maintenanceBusy
                        onClicked: {
                            deviceRow.host.approveTransfer(deviceRow.modelData, false, deviceRow.host.controller.syntheticTransferFingerprint, "");
                        }
                    }
                    Controls.Button {
                        visible: deviceRow.host.keyTransferDeviceId === deviceRow.deviceId && !deviceRow.host.keyTransferVault && ["waiting_acceptance", "sending", "waiting_receipt"].includes(deviceRow.syntheticTransferPhase)
                        text: KI18n.i18nc("@action:button", "Cancel Transfer")
                        icon.name: "dialog-cancel"
                        onClicked: {
                            console.info("Key transfer cancel from device details cancel button");
                            deviceRow.host.controller.cancelSyntheticKeyTransfer();
                        }
                    }
                    Controls.BusyIndicator {
                        running: deviceRow.host.keyTransferDeviceId === deviceRow.deviceId && !deviceRow.host.keyTransferVault && deviceRow.host.controller.syntheticTransferPreparing
                        visible: running
                        implicitWidth: Kirigami.Units.iconSizes.small
                        implicitHeight: implicitWidth
                    }
                    Controls.Label {
                        visible: deviceRow.host.keyTransferDeviceId === deviceRow.deviceId && !deviceRow.host.keyTransferVault && deviceRow.connection === "connected" && deviceRow.syntheticTransferPhase !== "unsupported" && deviceRow.syntheticTransferPhase !== "ready"
                        text: deviceRow.host.syntheticTransferLabel(deviceRow.syntheticTransferPhase)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: deviceRow.host.keyTransferDeviceId === deviceRow.deviceId && !deviceRow.host.keyTransferVault && deviceRow.host.controller.syntheticTransferError.length > 0
                    type: Kirigami.MessageType.Error
                    text: deviceRow.host.controller.syntheticTransferError
                }
            }
        }
        Item {
            Layout.preferredHeight: Kirigami.Units.largeSpacing
        }
    }
}
