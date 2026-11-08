pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// Vault Sync status for the list footer, in the manner of KDE's task indicators and of the
// phone: a still cloud icon with a progress ring that turns around it only while there is
// real work. What it says comes from the shared SyncStatusModel; this is only the view.
Controls.ToolButton {
    id: badge
    required property SyncStatusModel model
    signal activated
    // Whether the details popover is open: the tooltip gives way to it.
    property bool detailsOpen: false
    // Set by a click and cleared when the pointer leaves, so the tooltip that was showing
    // when the popover opened does not come back over it once it closes.
    property bool tipHidden: false

    // A status, not a control to tab through; clicking always shows the details, also when
    // nothing syncs yet: that is when the person most needs to know why.
    flat: true
    focusPolicy: Qt.NoFocus
    hoverEnabled: true
    padding: Kirigami.Units.smallSpacing
    onClicked: {
        tipHidden = true;
        activated();
    }
    onHoveredChanged: if (!hovered)
        tipHidden = false

    contentItem: RowLayout {
        spacing: Kirigami.Units.smallSpacing
        Item {
            readonly property real ring: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
            implicitWidth: ring
            implicitHeight: ring
            Controls.BusyIndicator {
                anchors.fill: parent
                padding: 0
                running: (badge.model.phase === "working" || badge.model.phase === "connecting") && badge.visible
                visible: running
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.smallMedium
                height: width
                isMask: true
                source: {
                    switch (badge.model.phase) {
                    case "locked":
                        return "lock-symbolic";
                    case "error":
                        return "state-error-symbolic";
                    case "offline":
                        return "state-offline-symbolic";
                    case "setup":
                        return "state-warning-symbolic";
                    default:
                        return "cloudstatus-symbolic";
                    }
                }
                color: {
                    switch (badge.model.phase) {
                    case "error":
                        return Kirigami.Theme.negativeTextColor;
                    case "locked":
                    case "setup":
                        return Kirigami.Theme.neutralTextColor;
                    case "offline":
                    case "idle":
                        return Kirigami.Theme.disabledTextColor;
                    case "ok":
                        return Kirigami.Theme.positiveTextColor;
                    default:
                        return Kirigami.Theme.highlightColor;
                    }
                }
                Accessible.ignored: true
            }
        }
        Controls.Label {
            // Words only when something needs attention; otherwise the icon and its tooltip.
            visible: badge.model.phase === "locked" || badge.model.phase === "error"
            text: badge.model.label
            textFormat: Text.PlainText
            elide: Text.ElideRight
            Layout.maximumWidth: Kirigami.Units.gridUnit * 10
            color: badge.model.phase === "error" ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.disabledTextColor
        }
    }

    Accessible.name: qsTr("Estado de la bóveda: %1").arg(model.detail)
    Controls.ToolTip.visible: (hovered || visualFocus) && !tipHidden && !detailsOpen && model.detail.length > 0
    Controls.ToolTip.text: model.detail
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
}
