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
    // Every linked device with its link and whether it syncs any key. Linked devices count
    // even before a key is chosen for them: a connected phone must show as connected.
    readonly property var devices: (host.devices || []).map(device => {
        const id = String(device.deviceId);
        const raw = String(device.connection || "");
        const connected = raw === "connected";
        const syncs = controller.vaultSyncPeers.indexOf(id) >= 0;
        const keys = host.vaultSyncKeyIds(id).length;
        let stats;
        if (connected && !syncs)
            stats = qsTr("Conectado · ninguna clave se sincroniza");
        else if (connected)
            stats = lastSyncMs > 0 ? qsTr("Última sincronización %1").arg(ago(lastSyncMs)) : qsTr("Conectado");
        else if (raw === "reconnecting")
            stats = qsTr("Reconectando…");
        else if (raw === "connecting")
            stats = qsTr("Conectando…");
        else if (raw === "failed")
            stats = qsTr("Falló la conexión");
        else
            stats = qsTr("Sin conexión");
        return {
            id: id,
            name: host.deviceName(device),
            connected: connected,
            // "connected" | "connecting" | "offline": what the summary of the link is built from.
            state: connected ? "connected" : (raw === "connecting" || raw === "reconnecting" ? "connecting" : "offline"),
            syncs: syncs,
            route: connected ? (device.routeLabel || "") : "",
            keys: keys,
            stats: stats
        };
    })
    // The devices that actually sync the vault.
    readonly property var syncDevices: devices.filter(device => device.syncs)

    // The link to the devices that matter: the ones that sync when any does, else every linked
    // one. "none" (no device, or the list has not arrived yet) | "connected" (at least one) |
    // "connecting" (none yet, one on its way) | "offline" (none).
    readonly property string link: {
        const pool = syncOn ? syncDevices : devices;
        if (pool.length === 0)
            return "none";
        let connecting = false;
        for (const device of pool) {
            if (device.state === "connected")
                return "connected";
            if (device.state === "connecting")
                connecting = true;
        }
        return connecting ? "connecting" : "offline";
    }
    // "ok" | "working" | "connecting" | "offline" | "setup" | "idle" | "locked" | "error".
    // "ok" means Sync is on and a phone is connected; "setup" that devices are linked but no key
    // syncs with them; "idle" that nothing is linked.
    readonly property string phase: {
        if (failed)
            return "error";
        if (syncOn && controller.vaultSyncLockedKey.length > 0)
            return "locked";
        if (syncOn && controller.vaultSyncFailed)
            return "error";
        if (working)
            return "working";
        if (!syncOn)
            return devices.length === 0 ? "idle" : "setup";
        if (link === "connecting")
            return "connecting";
        if (link === "offline")
            return "offline";
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
        case "connecting":
            return qsTr("Conectando…");
        case "offline":
            return qsTr("Sin conexión");
        case "setup":
            return qsTr("Sin claves que sincronizar");
        default:
            return storeStatus.length > 0 ? storeStatus : (syncOn ? qsTr("Sincronizada") : "");
        }
    }
    readonly property string detail: {
        if (phase === "locked")
            return qsTr("La sincronización sigue. Lo que llega cifrado para una clave sin contraseña se verá al desbloquearla.");
        if (phase === "error" && syncOn && controller.vaultSyncFailed && controller.vaultSyncMessage.length > 0)
            return controller.vaultSyncMessage;
        const pool = syncOn ? syncDevices : devices;
        const who = pool.length === 1 ? pool[0].name : qsTr("tus dispositivos");
        if (phase === "offline")
            return qsTr("Sin conexión con %1. La bóveda se sincroniza al volver a conectarse.").arg(who);
        if (phase === "connecting")
            return qsTr("Conectando con %1…").arg(who);
        if (phase === "setup")
            return (link === "connected" ? qsTr("%1 está conectado, pero ninguna clave se sincroniza con él.") : qsTr("Ninguna clave se sincroniza con %1.")).arg(who) + " " + qsTr("Elige las claves en Dispositivos.");
        if (phase === "idle" && label.length === 0)
            return qsTr("Ningún dispositivo vinculado. La bóveda solo está en este equipo.");
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

    // Renegotiates with the phone so it reads the current keys and binding.
    function retry(deviceId) {
        controller.retryVaultSync(deviceId);
    }

    // Every change of what the indicator says, with the link that caused it, so a status that
    // does not match the real connection can be traced in the log.
    onPhaseChanged: console.info("sync status phase=" + phase + " link=" + link + " syncOn=" + syncOn + " devices=" + devices.map(device => device.state + (device.syncs ? "+sync" : "-nosync") + (device.connected ? "/" + (device.route || "?") : "")).join(","))

    onLinkChanged: console.info("sync link=" + link + " syncOn=" + syncOn + " linked=" + devices.length + " syncing=" + syncDevices.length)

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
