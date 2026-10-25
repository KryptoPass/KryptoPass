pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// Status of the vault for the list footer, in the manner of KDE's task indicators: a small
// flat icon. It spins only while there is real work (a slow load, or changes that actually
// moved to or from a phone), turns into a check when everything is in step, and flags what
// needs attention. The vault's own message ("Bóveda cargada"...) is its tooltip.
Controls.ToolButton {
    id: badge
    required property var syncController
    // Latest message of the vault or key store, and whether it is busy or failed.
    required property string storeStatus
    required property bool busy
    required property bool failed
    signal activated

    // Phones with Sync on; without them the indicator still reports the vault itself.
    readonly property bool syncOn: syncController.vaultSyncPeers.length > 0
    readonly property string phase: {
        if (failed)
            return "error";
        if (syncOn && syncController.vaultSyncLockedKey.length > 0)
            return "locked";
        if (syncOn && syncController.vaultSyncFailed)
            return "error";
        if (working)
            return "working";
        return "ok";
    }
    // Spin only for real work: an operation that outlasts a blink, a mirror pass that takes
    // time, or data that really moved. The mirror polls every few seconds and most passes
    // find nothing, and those must not make the icon spin.
    readonly property bool working: (busy && slowBusy.elapsed) || (syncOn && (slowRun.elapsed || holdSpin.running))
    readonly property string text_: {
        switch (phase) {
        case "locked":
            return qsTr("Falta una contraseña");
        case "error":
            return syncOn && syncController.vaultSyncFailed && storeStatus.length === 0 ? qsTr("Error de sincronización") : storeStatus;
        case "working":
            return busy ? storeStatus : qsTr("Sincronizando…");
        default:
            return storeStatus.length > 0 ? storeStatus : (syncOn ? qsTr("Sincronizada") : "");
        }
    }
    readonly property string detail: {
        if (phase === "locked")
            return qsTr("La sincronización está detenida: una clave necesita su contraseña.");
        if (phase === "error" && syncOn && syncController.vaultSyncFailed && syncController.vaultSyncMessage.length > 0)
            return syncController.vaultSyncMessage;
        const sync = syncOn ? (phase === "working" && !busy ? "" : qsTr("Sincronización con el teléfono: activa.")) : "";
        return sync.length > 0 && text_.length > 0 ? text_ + "\n" + sync : (text_.length > 0 ? text_ : sync);
    }

    flat: true
    // A status, not a control to tab through; with Sync on, clicking opens the devices window.
    focusPolicy: Qt.NoFocus
    hoverEnabled: syncOn
    padding: Kirigami.Units.smallSpacing
    onClicked: if (syncOn)
        activated()

    Timer {
        id: holdSpin
        interval: 1500
    }
    // A mirror pass or an operation counts as work only once it has lasted a moment.
    Timer {
        id: slowRun
        property bool elapsed: false
        interval: 600
        running: badge.syncOn && badge.syncController.vaultMirrorActive
        onTriggered: elapsed = true
        onRunningChanged: if (!running)
            elapsed = false
    }
    Timer {
        id: slowBusy
        property bool elapsed: false
        interval: 600
        running: badge.busy
        onTriggered: elapsed = true
        onRunningChanged: if (!running)
            elapsed = false
    }
    Connections {
        target: badge.syncController
        // Data really moved with a phone: show it even if the pass was quick.
        function onVaultSyncActivityChanged() {
            holdSpin.restart();
        }
    }

    contentItem: RowLayout {
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Icon {
            id: icon
            implicitWidth: Kirigami.Units.iconSizes.small
            implicitHeight: Kirigami.Units.iconSizes.small
            source: {
                switch (badge.phase) {
                case "working":
                    return "state-sync";
                case "locked":
                    return "state-pause";
                case "error":
                    return "state-error";
                default:
                    return "state-ok";
                }
            }
            color: badge.phase === "error" ? Kirigami.Theme.negativeTextColor : (badge.phase === "locked" ? Kirigami.Theme.neutralTextColor : (badge.phase === "ok" ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor))
            Accessible.ignored: true
            RotationAnimator on rotation {
                running: badge.phase === "working" && badge.visible
                from: 0
                to: 360
                duration: 1100
                loops: Animation.Infinite
            }
            // The spinner must come back upright when it stops.
            onSourceChanged: if (badge.phase !== "working")
                rotation = 0
        }
        Controls.Label {
            // Words only when something needs attention; otherwise the icon and its tooltip.
            visible: badge.phase === "locked" || badge.phase === "error"
            text: badge.text_
            textFormat: Text.PlainText
            elide: Text.ElideRight
            Layout.maximumWidth: Kirigami.Units.gridUnit * 10
            color: badge.phase === "error" ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.disabledTextColor
        }
    }

    Accessible.name: qsTr("Estado de la bóveda: %1").arg(detail)
    Controls.ToolTip.visible: (hovered || visualFocus) && detail.length > 0
    Controls.ToolTip.text: detail
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
}
