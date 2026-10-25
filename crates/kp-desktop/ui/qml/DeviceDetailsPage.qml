pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.ScrollView {
    id: deviceRow
    required property var host
    required property var modelData
    readonly property string deviceId: String(modelData.deviceId || "")
    readonly property string connection: String(modelData.connection || "")
    readonly property string connectionGeneration: String(modelData.connectionGeneration || "")
    readonly property string syntheticTransferPhase: String(modelData.syntheticTransferPhase || "unsupported")
    readonly property int keyTransferVersion: Number(modelData.keyTransferVersion || 0)
    readonly property var syncedKeys: host.vaultSyncKeyIds(deviceId)
    // Keys the phone declared for this connection; null while unknown (offline or older app).
    readonly property var phoneKeys: modelData.vaultKeys || null
    // One row per key that matters for Sync with this device, with what is true of it and
    // what the user can do about it.
    readonly property var syncKeyRows: {
        const upper = id => String(id).toUpperCase();
        const selected = syncedKeys.map(upper);
        const declared = phoneKeys === null ? null : phoneKeys.map(upper);
        const lockedKey = upper(host.controller.vaultSyncLockedKey);
        const ids = [];
        const add = id => { if (ids.indexOf(id) < 0) ids.push(id); };
        selected.forEach(add);
        if (declared !== null) declared.forEach(add);
        host.keyStore.entries.filter(key => key.hasSecret).forEach(key => add(upper(key.id)));
        return ids.map(id => {
            const entry = host.keyStore.entries.find(key => upper(key.id) === id);
            const onDesktop = !!entry && entry.hasSecret;
            const inSet = selected.indexOf(id) >= 0;
            const onPhone = declared === null ? null : declared.indexOf(id) >= 0;
            let state = "idle";
            let text = "";
            if (inSet && onPhone === false) {
                state = "warning";
                text = qsTr("El teléfono no tiene esta clave, así que no se sincroniza.");
            } else if (inSet) {
                state = "active";
                text = onPhone === null ? qsTr("Se sincroniza. No se pudo comprobar el teléfono (desconectado o versión anterior).") : qsTr("Se sincroniza.");
            } else if (onPhone) {
                state = onDesktop ? "available" : "idle";
                text = onDesktop ? qsTr("El teléfono la tiene, pero no se sincroniza.") : qsTr("Solo la tiene el teléfono; Desktop no tiene su clave privada.");
            } else {
                text = onPhone === false ? qsTr("Solo en Desktop: el teléfono no la tiene.") : qsTr("Solo en Desktop.");
            }
            return {
                id: id,
                title: entry ? (entry.title || qsTr("Sin nombre")) : qsTr("Clave desconocida"),
                email: entry ? (entry.email || "") : "",
                short: id.slice(-16),
                state: state,
                text: text,
                inSet: inSet,
                canAdd: !inSet && onDesktop && onPhone !== false,
                locked: lockedKey === id
            };
        });
    }
    readonly property bool vaultSyncOn: host.controller.vaultSyncPeers.indexOf(deviceId) >= 0
    readonly property bool transferPending: (modelData.isActive && host.controller.syntheticTransferPreparing) || ["waiting_acceptance", "sending", "waiting_receipt"].includes(syntheticTransferPhase)
    readonly property bool canSend: connection === "connected" && keyTransferVersion >= 2 && syntheticTransferPhase === "ready" && !host.controller.syntheticTransferPreparing && !host.controller.syncBusy && !host.controller.busy && !host.controller.maintenanceBusy
    readonly property bool transferEnded: ["complete", "rejected", "cancelled", "expired", "failed", "uncertain"].includes(syntheticTransferPhase)
    readonly property bool canRecoverTransfer: !transferPending && !host.controller.busy && !host.controller.syncBusy && !host.controller.maintenanceBusy && host.controller.statusCode !== "closing" && ((connection === "connected" && transferEnded) || connection === "disconnected" || connection === "failed")
    property bool technicalOpen: false
    property bool labExpanded: false
    readonly property int index: host.devices.findIndex(device => String(device.deviceId) === deviceId)
    onDeviceIdChanged: {
        technicalOpen = false;
        labExpanded = false;
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
                            text: qsTr("· Vinculado")
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
                    text: qsTr("Cambiar nombre")
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
                        text: qsTr("Sincronización de la bóveda")
                        wrapMode: Text.Wrap
                    }
                    Controls.Switch {
                        checked: deviceRow.vaultSyncOn
                        enabled: deviceRow.host.available && !deviceRow.host.controller.busy && !deviceRow.host.controller.maintenanceBusy
                        Accessible.name: qsTr("Sincronizar la bóveda con este dispositivo")
                        onToggled: {
                            deviceRow.host.setVaultSync(deviceRow.deviceId, checked);
                            checked = Qt.binding(() => deviceRow.vaultSyncOn);
                        }
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: deviceRow.vaultSyncOn ? (deviceRow.connection === "connected" ? qsTr("Activada · se sincroniza mientras la bóveda está desbloqueada.") : qsTr("Activada · esperando que el dispositivo se conecte.")) : qsTr("Comparte los elementos de esta bóveda. El teléfono necesita su clave OpenPGP y tener Sync activado.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    visible: deviceRow.vaultSyncOn
                    spacing: Kirigami.Units.smallSpacing
                    RowLayout {
                        Layout.fillWidth: true
                        Controls.Label {
                            Layout.fillWidth: true
                            text: qsTr("Claves en Sincronización (%1 de %2)").arg(deviceRow.syncedKeys.length).arg(deviceRow.syncKeyRows.length)
                            font.weight: Font.DemiBold
                            wrapMode: Text.Wrap
                        }
                        Controls.Button {
                            text: qsTr("Actualizar")
                            icon.name: "view-refresh"
                            flat: true
                            enabled: deviceRow.connection === "connected"
                            onClicked: deviceRow.host.controller.retryVaultSync(deviceRow.deviceId)
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.text: qsTr("Vuelve a negociar con el teléfono para leer sus claves actuales")
                        }
                    }
                    Repeater {
                        model: deviceRow.syncKeyRows
                        delegate: RowLayout {
                            id: keyRow
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.largeSpacing
                            Kirigami.Icon {
                                source: keyRow.modelData.state === "active" ? "emblem-ok-symbolic" : (keyRow.modelData.state === "warning" ? "dialog-warning" : "emblem-unavailable")
                                color: keyRow.modelData.state === "active" ? Kirigami.Theme.positiveTextColor : (keyRow.modelData.state === "warning" ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.disabledTextColor)
                                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                                implicitHeight: Kirigami.Units.iconSizes.smallMedium
                                Accessible.ignored: true
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: keyRow.modelData.title + (keyRow.modelData.email ? " · " + keyRow.modelData.email : "")
                                    textFormat: Text.PlainText
                                    elide: Text.ElideRight
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: keyRow.modelData.text + " · …" + keyRow.modelData.short
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    font: Kirigami.Theme.smallFont
                                    color: Kirigami.Theme.disabledTextColor
                                }
                            }
                            Controls.Button {
                                visible: keyRow.modelData.locked
                                text: qsTr("Desbloquear…")
                                icon.name: "dialog-password"
                                onClicked: deviceRow.host.controller.unlockSyncKey(keyRow.modelData.id)
                            }
                            Controls.Button {
                                visible: keyRow.modelData.canAdd
                                text: qsTr("Sincronizar")
                                icon.name: "list-add"
                                enabled: !deviceRow.host.controller.busy
                                onClicked: deviceRow.host.controller.enableVaultSync(deviceRow.deviceId, deviceRow.syncedKeys.concat([keyRow.modelData.id]).join(","))
                            }
                            Controls.Button {
                                visible: keyRow.modelData.inSet && deviceRow.syncedKeys.length > 1
                                text: qsTr("Dejar de sincronizar")
                                flat: true
                                enabled: !deviceRow.host.controller.busy
                                onClicked: deviceRow.host.controller.enableVaultSync(deviceRow.deviceId, deviceRow.syncedKeys.filter(id => String(id).toUpperCase() !== keyRow.modelData.id).join(","))
                            }
                        }
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: deviceRow.syncKeyRows.length === 0
                        text: qsTr("Aún no hay claves con parte privada en este equipo.")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Kirigami.Theme.disabledTextColor
                    }
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: deviceRow.vaultSyncOn && deviceRow.host.vaultLocked
                    type: Kirigami.MessageType.Warning
                    text: qsTr("La sincronización está en pausa: la bóveda de Desktop está bloqueada. Desbloquéala para que los cambios se intercambien con el teléfono.")
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: deviceRow.vaultSyncOn && deviceRow.host.controller.vaultSyncLockedKey.length > 0
                    type: Kirigami.MessageType.Warning
                    text: qsTr("La sincronización está detenida: falta la contraseña de la clave de %1 y Desktop no puede leer sus elementos.").arg(deviceRow.host.keyTitle(deviceRow.host.controller.vaultSyncLockedKey))
                    actions: [
                        Kirigami.Action {
                            text: qsTr("Desbloquear clave…")
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
                    text: qsTr("Clave OpenPGP")
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Elige una de tus claves privadas y revisa el destinatario antes de enviarla. Se conserva protegida con su contraseña.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Button {
                    text: deviceRow.connection === "disconnected" || deviceRow.connection === "failed" ? qsTr("Reconectar para enviar") : deviceRow.transferEnded ? qsTr("Desconectar para reintentar") : qsTr("Elegir clave para enviar…")
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
                    text: deviceRow.connection !== "connected" ? qsTr("Conecta este dispositivo para enviar la clave.") : deviceRow.transferEnded ? qsTr("Para preparar otro envío, desconecta y vuelve a conectar este dispositivo. La vinculación se conserva.") : deviceRow.keyTransferVersion < 2 ? qsTr("Actualiza KryptoPass en el teléfono para recibir la clave.") : deviceRow.syntheticTransferPhase === "ready" ? qsTr("Espera a que termine la operación actual.") : deviceRow.host.keyTransferLabel(deviceRow.syntheticTransferPhase)
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
                        text: deviceRow.host.controller.syntheticTransferPreparing ? qsTr("Preparando la clave…") : deviceRow.host.keyTransferLabel(deviceRow.syntheticTransferPhase)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Button {
                        text: qsTr("Cancelar envío")
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
            text: qsTr("Detalles de conexión")
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
                text: qsTr("Identificador del dispositivo")
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
                text: qsTr("Copiar identificador")
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
                    text: qsTr("Desconectar conserva la vinculación. Olvidar retira el permiso de este dispositivo en este equipo.")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Flow {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.largeSpacing
                    Controls.Button {
                        text: deviceRow.modelData.isActive ? qsTr("Desconectar") : qsTr("Reconectar")
                        icon.name: deviceRow.modelData.isActive ? "network-disconnect" : "network-connect"
                        Kirigami.Theme.textColor: deviceRow.modelData.isActive ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.linkColor
                        enabled: !deviceRow.host.controller.busy && !deviceRow.host.controller.syncBusy && !deviceRow.host.controller.maintenanceBusy && deviceRow.host.controller.statusCode !== "closing"
                        onClicked: {
                            if (deviceRow.modelData.isActive)
                                deviceRow.host.controller.disconnectPeer(deviceRow.deviceId);
                            else
                                deviceRow.host.reconnectDevice(deviceRow.modelData);
                        }
                    }
                    Controls.Button {
                        text: qsTr("Olvidar dispositivo…")
                        icon.name: "edit-delete"
                        Kirigami.Theme.textColor: Kirigami.Theme.negativeTextColor
                        enabled: !deviceRow.host.controller.busy && !deviceRow.host.controller.aliasBusy
                        onClicked: deviceRow.host.forgetDevice(deviceRow.modelData)
                    }
                }
            }
        }

        Controls.ToolButton {
            Layout.leftMargin: Kirigami.Units.largeSpacing
            visible: deviceRow.host.available
            text: qsTr("Desarrollo")
            icon.name: deviceRow.labExpanded ? "arrow-down" : "arrow-right"
            checkable: true
            checked: deviceRow.labExpanded
            onClicked: deviceRow.labExpanded = checked
        }
        ColumnLayout {
            id: lab
            Layout.fillWidth: true
            visible: deviceRow.labExpanded
            spacing: Kirigami.Units.largeSpacing
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: deviceRow.host.available && deviceRow.host.controller.syntheticTransferFingerprint.length > 0 && deviceRow.modelData.trust === "trusted"
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
                        enabled: !deviceRow.host.controller.syntheticTransferPreparing && !deviceRow.host.controller.syncBusy && !deviceRow.host.controller.busy && !deviceRow.host.controller.maintenanceBusy
                        onClicked: {
                            deviceRow.host.approveTransfer(deviceRow.modelData, false, deviceRow.host.controller.syntheticTransferFingerprint, "");
                        }
                    }
                    Controls.Button {
                        visible: deviceRow.host.keyTransferDeviceId === deviceRow.deviceId && !deviceRow.host.keyTransferVault && ["waiting_acceptance", "sending", "waiting_receipt"].includes(deviceRow.syntheticTransferPhase)
                        text: qsTr("Cancelar transferencia")
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
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: deviceRow.labExpanded
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
                        enabled: !deviceRow.host.controller.syncBusy && !deviceRow.host.controller.busy && !deviceRow.host.controller.maintenanceBusy
                        onClicked: {
                            deviceRow.host.controller.selectDevice(deviceRow.index);
                            deviceRow.host.controller.activateSyntheticSync();
                        }
                    }
                    Controls.Button {
                        text: qsTr("Actualizar prueba")
                        icon.name: "view-refresh"
                        enabled: deviceRow.host.controller.syncConfigured && !deviceRow.host.controller.syncBusy && !deviceRow.host.controller.busy && !deviceRow.host.controller.maintenanceBusy
                        onClicked: {
                            deviceRow.host.controller.selectDevice(deviceRow.index);
                            deviceRow.host.controller.refreshSyntheticSync();
                        }
                    }
                    Controls.Button {
                        text: qsTr("Crear credencial de prueba")
                        icon.name: "list-add"
                        enabled: deviceRow.host.controller.syncConfigured && !deviceRow.host.controller.syncBusy && !deviceRow.host.controller.busy && !deviceRow.host.controller.maintenanceBusy
                        onClicked: {
                            deviceRow.host.controller.selectDevice(deviceRow.index);
                            deviceRow.host.controller.createSyntheticCredential();
                        }
                    }
                }
                Controls.BusyIndicator {
                    Layout.alignment: Qt.AlignLeft
                    running: deviceRow.host.controller.syncBusy && deviceRow.host.controller.syncPeerId === deviceRow.deviceId
                    visible: running
                    implicitWidth: Kirigami.Units.iconSizes.small
                    implicitHeight: implicitWidth
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: deviceRow.host.controller.syncPeerId === deviceRow.deviceId && deviceRow.host.controller.syncError.length > 0
                    type: Kirigami.MessageType.Error
                    text: deviceRow.host.controller.syncError
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: deviceRow.host.controller.syncPeerId === deviceRow.deviceId && deviceRow.host.controller.syncStatus.length > 0
                    text: deviceRow.host.controller.syncStatus
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: deviceRow.host.controller.syncPeerId === deviceRow.deviceId && deviceRow.host.controller.syncConfigured
                    text: qsTr("Revisiones guardadas en este equipo: %1").arg(deviceRow.host.controller.syncLocalRevisionCount)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Repeater {
                    model: deviceRow.host.controller.syncPeerId === deviceRow.deviceId ? deviceRow.host.controller.syncEntries : []
                    Controls.Label {
                        required property string modelData
                        Layout.fillWidth: true
                        text: "• " + modelData
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
        Item {
            Layout.preferredHeight: Kirigami.Units.largeSpacing
        }
    }
}
