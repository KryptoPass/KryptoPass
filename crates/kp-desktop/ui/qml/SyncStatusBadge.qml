pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// Corner indicator of the vault Sync, like a cloud client: it spins while changes are
// exchanged, shows a check when everything is in step, and says why it is stopped.
Controls.Control {
    id: badge
    required property var controller
    required property bool vaultLocked
    signal activated

    // Peers with Sync on; without them there is nothing to show.
    readonly property bool syncOn: controller.vaultSyncPeers.length > 0
    readonly property string phase: {
        if (vaultLocked)
            return "paused";
        if (controller.vaultSyncLockedKey.length > 0)
            return "locked";
        if (controller.vaultSyncFailed)
            return "error";
        if (controller.vaultMirrorActive || holdSpin.running)
            return "syncing";
        return "ok";
    }
    property double lastOkMs: 0

    visible: syncOn
    padding: Kirigami.Units.smallSpacing
    implicitWidth: row.implicitWidth + leftPadding + rightPadding
    implicitHeight: row.implicitHeight + topPadding + bottomPadding

    // Keep spinning for a moment after a run ends, so a quick exchange is still visible.
    Timer {
        id: holdSpin
        interval: 1200
    }
    Connections {
        target: badge.controller
        function onVaultMirrorActiveChanged() {
            if (badge.controller.vaultMirrorActive)
                holdSpin.restart();
            else if (!badge.controller.vaultSyncFailed)
                badge.lastOkMs = Date.now();
        }
    }

    readonly property string label: {
        switch (phase) {
        case "syncing":
            return qsTr("Sincronizando…");
        case "paused":
            return qsTr("En pausa: la bóveda está bloqueada");
        case "locked":
            return qsTr("En pausa: falta la contraseña de una clave");
        case "error":
            return controller.vaultSyncMessage.length > 0 ? controller.vaultSyncMessage : qsTr("Error de sincronización");
        default:
            return badge.lastOkMs > 0 ? qsTr("Sincronizada") : qsTr("Sincronización activa");
        }
    }

    background: Rectangle {
        radius: height / 2
        color: Kirigami.Theme.backgroundColor
        border.color: Kirigami.ColorUtils.tintWithAlpha(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.25)
        opacity: hover.hovered ? 1 : 0.92
    }
    contentItem: RowLayout {
        id: row
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Icon {
            id: icon
            implicitWidth: Kirigami.Units.iconSizes.smallMedium
            implicitHeight: Kirigami.Units.iconSizes.smallMedium
            source: {
                switch (badge.phase) {
                case "syncing":
                    return "state-sync";
                case "paused":
                case "locked":
                    return "state-pause";
                case "error":
                    return "state-error";
                default:
                    return "state-ok";
                }
            }
            color: badge.phase === "error" ? Kirigami.Theme.negativeTextColor : (badge.phase === "paused" || badge.phase === "locked" ? Kirigami.Theme.neutralTextColor : (badge.phase === "ok" ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor))
            Accessible.ignored: true
            RotationAnimator on rotation {
                running: badge.phase === "syncing" && badge.visible
                from: 0
                to: 360
                duration: 1100
                loops: Animation.Infinite
            }
            // The spinner must come back upright when it stops.
            onSourceChanged: if (badge.phase !== "syncing")
                rotation = 0
        }
        Controls.Label {
            text: badge.label
            textFormat: Text.PlainText
            elide: Text.ElideRight
            Layout.maximumWidth: Kirigami.Units.gridUnit * 16
            visible: badge.phase !== "ok" && badge.phase !== "syncing" || hover.hovered
        }
    }
    HoverHandler {
        id: hover
    }
    TapHandler {
        onTapped: badge.activated()
    }
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Estado de la sincronización: %1").arg(label)
    Controls.ToolTip.visible: hover.hovered
    Controls.ToolTip.text: label
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
}
