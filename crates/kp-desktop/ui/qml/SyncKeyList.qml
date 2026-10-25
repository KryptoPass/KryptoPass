pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// The keys that matter for Sync with one device: which are synced, which the phone holds
// but are not synced, which exist only here, and what the user can do about each. Used by
// the device page and by the status popover, so both always say the same.
ColumnLayout {
    id: list
    // Owner of the key store and the device controller (the devices window).
    required property var host
    required property string deviceId
    // The device as projected for QML (connection, vaultKeys...).
    required property var device
    readonly property var syncedKeys: host.vaultSyncKeyIds(deviceId)
    // Keys the phone declared for this connection; null while unknown (offline or older app).
    readonly property var phoneKeys: device.vaultKeys || null
    spacing: Kirigami.Units.smallSpacing
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
    RowLayout {
        Layout.fillWidth: true
        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Claves en Sincronización (%1 de %2)").arg(list.syncedKeys.length).arg(list.syncKeyRows.length)
            font.weight: Font.DemiBold
            wrapMode: Text.Wrap
        }
        Controls.Button {
            text: qsTr("Actualizar")
            icon.name: "view-refresh"
            flat: true
            enabled: String(list.device.connection || "") === "connected"
            onClicked: list.host.controller.retryVaultSync(list.deviceId)
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.text: qsTr("Vuelve a negociar con el teléfono para leer sus claves actuales")
        }
    }
    Repeater {
        model: list.syncKeyRows
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
                onClicked: list.host.controller.unlockSyncKey(keyRow.modelData.id)
            }
            Controls.Button {
                visible: keyRow.modelData.canAdd
                text: qsTr("Sincronizar")
                icon.name: "list-add"
                enabled: !list.host.controller.busy
                onClicked: list.host.controller.enableVaultSync(list.deviceId, list.syncedKeys.concat([keyRow.modelData.id]).join(","))
            }
            Controls.Button {
                visible: keyRow.modelData.inSet && list.syncedKeys.length > 1
                text: qsTr("Dejar de sincronizar")
                flat: true
                enabled: !list.host.controller.busy
                onClicked: list.host.controller.enableVaultSync(list.deviceId, list.syncedKeys.filter(id => String(id).toUpperCase() !== keyRow.modelData.id).join(","))
            }
        }
    }
    Controls.Label {
        Layout.fillWidth: true
        visible: list.syncKeyRows.length === 0
        text: qsTr("Aún no hay claves con parte privada en este equipo.")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Kirigami.Theme.disabledTextColor
    }
}
