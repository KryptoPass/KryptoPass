pragma ComponentBehavior: Bound
import QtQuick
import org.kde.ki18n

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
            stats = KI18n.i18n("Connected · no key syncs");
        else if (connected)
            stats = lastSyncMs > 0 ? KI18n.i18nc("@info %1 is a relative time such as 5 min ago", "Last synced %1", ago(lastSyncMs)) : KI18n.i18nc("@info connection status", "Connected");
        else if (raw === "reconnecting")
            stats = KI18n.i18n("Reconnecting…");
        else if (raw === "connecting")
            stats = KI18n.i18n("Connecting…");
        else if (raw === "failed")
            stats = KI18n.i18n("Connection failed");
        else
            stats = KI18n.i18n("Offline");
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
            return KI18n.i18n("A passphrase is missing");
        case "error":
            return syncOn && controller.vaultSyncFailed && storeStatus.length === 0 ? KI18n.i18n("Sync error") : storeStatus;
        case "working":
            return busy ? storeStatus : KI18n.i18n("Syncing…");
        case "connecting":
            return KI18n.i18n("Connecting…");
        case "offline":
            return KI18n.i18n("Offline");
        case "setup":
            return KI18n.i18n("No keys to sync");
        default:
            return storeStatus.length > 0 ? storeStatus : (syncOn ? KI18n.i18nc("@info vault sync status", "Synced") : "");
        }
    }
    readonly property string detail: {
        if (phase === "locked")
            return KI18n.i18n("Sync continues. What arrives encrypted for a key without its passphrase will be visible once you unlock it.");
        if (phase === "error" && syncOn && controller.vaultSyncFailed && controller.vaultSyncMessage.length > 0)
            return controller.vaultSyncMessage;
        const pool = syncOn ? syncDevices : devices;
        const who = pool.length === 1 ? pool[0].name : KI18n.i18n("your devices");
        if (phase === "offline")
            return KI18n.i18n("No connection with %1. The vault syncs when it reconnects.", who);
        if (phase === "connecting")
            return KI18n.i18n("Connecting to %1…", who);
        if (phase === "setup")
            return (link === "connected" ? KI18n.i18n("%1 is connected, but no key syncs with it.", who) : KI18n.i18n("No key syncs with %1.", who)) + " " + KI18n.i18n("Choose the keys in Devices.");
        if (phase === "idle" && label.length === 0)
            return KI18n.i18n("No paired devices. The vault is only on this computer.");
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
            return KI18n.i18nc("@info relative time", "now");
        if (seconds < 60)
            return KI18n.i18nc("@info relative time", "%1 s ago", seconds);
        const minutes = Math.round(seconds / 60);
        if (minutes < 60)
            return KI18n.i18nc("@info relative time", "%1 min ago", minutes);
        return KI18n.i18nc("@info relative time", "%1 h ago", Math.round(minutes / 60));
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
