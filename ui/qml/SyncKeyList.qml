pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

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
        const add = id => {
            if (ids.indexOf(id) < 0)
                ids.push(id);
        };
        selected.forEach(add);
        if (declared !== null)
            declared.forEach(add);
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
                text = KI18n.i18n("The phone does not have this key, so it does not sync.");
            } else if (inSet) {
                state = "active";
                text = onPhone === null ? KI18n.i18n("It syncs. The phone could not be checked (disconnected or an older version).") : KI18n.i18n("It syncs.");
            } else if (onPhone) {
                state = onDesktop ? "available" : "idle";
                text = onDesktop ? KI18n.i18n("The phone has it, but it does not sync.") : KI18n.i18n("Only the phone has it; Desktop does not have its private key.");
            } else {
                text = onPhone === false ? KI18n.i18n("Only on Desktop: the phone does not have it.") : KI18n.i18n("Only on Desktop.");
            }
            return {
                id: id,
                title: entry ? (entry.title || KI18n.i18n("No name")) : KI18n.i18n("Unknown key"),
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
            text: KI18n.i18nc("@title:group", "Keys in sync (%1 of %2)", list.syncedKeys.length, list.syncKeyRows.length)
            font.weight: Font.DemiBold
            wrapMode: Text.Wrap
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Refresh")
            icon.name: "view-refresh"
            flat: true
            enabled: String(list.device.connection || "") === "connected"
            onClicked: list.host.controller.retryVaultSync(list.deviceId)
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.text: KI18n.i18nc("@info:tooltip", "Negotiate with the phone again to read its current keys")
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
                text: KI18n.i18nc("@action:button", "Unlock…")
                icon.name: "dialog-password"
                onClicked: list.host.controller.unlockSyncKey(keyRow.modelData.id)
            }
            Controls.Button {
                visible: keyRow.modelData.canAdd
                text: KI18n.i18nc("@action:button", "Sync")
                icon.name: "list-add"
                enabled: !list.host.controller.busy
                onClicked: list.host.controller.enableVaultSync(list.deviceId, list.syncedKeys.concat([keyRow.modelData.id]).join(","))
            }
            Controls.Button {
                visible: keyRow.modelData.inSet && list.syncedKeys.length > 1
                text: KI18n.i18nc("@action:button", "Stop Syncing")
                flat: true
                enabled: !list.host.controller.busy
                onClicked: list.host.controller.enableVaultSync(list.deviceId, list.syncedKeys.filter(id => String(id).toUpperCase() !== keyRow.modelData.id).join(","))
            }
        }
    }
    Controls.Label {
        Layout.fillWidth: true
        visible: list.syncKeyRows.length === 0
        text: KI18n.i18n("There are no keys with a private part on this computer yet.")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Kirigami.Theme.disabledTextColor
    }
}
