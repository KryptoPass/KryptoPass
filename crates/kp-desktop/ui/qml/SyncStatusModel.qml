pragma ComponentBehavior: Bound
import QtQuick

// One place that decides what the vault Sync status is, so the footer indicator and its
// popover never disagree. It owns the rules (when to spin, what to say), the per-device
// summary and the retry action. Views only read it: declare a single instance and hand it
// to whatever needs to show the status.
QtObject {
    id: model

    // The device controller and the devices window (it owns the device list and key store).
    required property var controller
    required property var host
    // Latest message of the vault or key store, and whether it is busy or failed.
    property string storeStatus: ""
    property bool busy: false
    property bool failed: false

    // Phones with Sync on; without them the status still reports the vault itself.
    readonly property bool syncOn: controller.vaultSyncPeers.length > 0
    // "ok" | "working" | "locked" | "error"
    readonly property string phase: {
        if (failed)
            return "error";
        if (syncOn && controller.vaultSyncLockedKey.length > 0)
            return "locked";
        if (syncOn && controller.vaultSyncFailed)
            return "error";
        if (working)
            return "working";
        return "ok";
    }
    // Spin only for real work: an operation that outlasts a blink, a mirror pass that takes
    // time, or data that really moved. The mirror polls every few seconds and most passes
    // find nothing, and those must not make the indicator turn.
    readonly property bool working: (busy && slowBusy.elapsed) || (syncOn && (slowRun.elapsed || holdSpin.running))
    readonly property string label: {
        switch (phase) {
        case "locked":
            return qsTr("Falta una contraseña");
        case "error":
            return syncOn && controller.vaultSyncFailed && storeStatus.length === 0 ? qsTr("Error de sincronización") : storeStatus;
        case "working":
            return busy ? storeStatus : qsTr("Sincronizando…");
        default:
            return storeStatus.length > 0 ? storeStatus : (syncOn ? qsTr("Sincronizada") : "");
        }
    }
    readonly property string detail: {
        if (phase === "locked")
            return qsTr("La sincronización está detenida: una clave necesita su contraseña.");
        if (phase === "error" && syncOn && controller.vaultSyncFailed && controller.vaultSyncMessage.length > 0)
            return controller.vaultSyncMessage;
        return label;
    }

    // When data last moved with a phone (ms since the epoch), and a coarse clock to age it.
    property double lastSyncMs: 0
    property double nowMs: Date.now()

    function ago(ms) {
        if (ms <= 0)
            return "";
        const seconds = Math.max(0, Math.round((nowMs - ms) / 1000));
        if (seconds < 10)
            return qsTr("ahora");
        if (seconds < 60)
            return qsTr("hace %1 s").arg(seconds);
        const minutes = Math.round(seconds / 60);
        if (minutes < 60)
            return qsTr("hace %1 min").arg(minutes);
        return qsTr("hace %1 h").arg(Math.round(minutes / 60));
    }

    // One summary per device that syncs the vault: just what a glance needs.
    readonly property var devices: (host.devices || []).filter(device => controller.vaultSyncPeers.indexOf(String(device.deviceId)) >= 0).map(device => {
        const id = String(device.deviceId);
        const connected = device.connection === "connected";
        return {
            id: id,
            name: host.deviceName(device),
            connected: connected,
            route: connected ? (device.routeLabel || "") : "",
            keys: host.vaultSyncKeyIds(id).length,
            // The phone's own message when it is not connected, else when data last moved.
            stats: connected ? (lastSyncMs > 0 ? qsTr("Última sincronización %1").arg(ago(lastSyncMs)) : qsTr("Conectado")) : qsTr("Sin conexión")
        };
    })

    // Renegotiates with the phone so it reads the current keys and binding.
    function retry(deviceId) {
        controller.retryVaultSync(deviceId);
    }

    property Timer holdSpin: Timer {
        interval: 1500
    }
    // A mirror pass or an operation counts as work only once it has lasted a moment.
    property Timer slowRun: Timer {
        property bool elapsed: false
        interval: 600
        running: model.syncOn && model.controller.vaultMirrorActive
        onTriggered: elapsed = true
        onRunningChanged: if (!running)
            elapsed = false
    }
    property Timer slowBusy: Timer {
        property bool elapsed: false
        interval: 600
        running: model.busy
        onTriggered: elapsed = true
        onRunningChanged: if (!running)
            elapsed = false
    }
    property Timer clock: Timer {
        interval: 5000
        repeat: true
        running: model.syncOn
        onTriggered: model.nowMs = Date.now()
    }
    property Connections activity: Connections {
        target: model.controller
        // Data really moved with a phone: show it even if the pass was quick.
        function onVaultSyncActivityChanged() {
            model.lastSyncMs = Date.now();
            model.nowMs = model.lastSyncMs;
            holdSpin.restart();
        }
    }
}
