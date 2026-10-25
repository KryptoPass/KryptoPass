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

    // A status, not a control to tab through; with Sync on, clicking shows the details.
    flat: true
    focusPolicy: Qt.NoFocus
    hoverEnabled: model.syncOn
    padding: Kirigami.Units.smallSpacing
    onClicked: if (model.syncOn)
        activated()

    contentItem: RowLayout {
        spacing: Kirigami.Units.smallSpacing
        Item {
            readonly property real ring: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
            implicitWidth: ring
            implicitHeight: ring
            Controls.BusyIndicator {
                anchors.fill: parent
                padding: 0
                running: badge.model.phase === "working" && badge.visible
                visible: running
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.smallMedium
                height: width
                isMask: true
                source: badge.model.phase === "locked" ? "state-pause-symbolic" : (badge.model.phase === "error" ? "state-error-symbolic" : "cloudstatus-symbolic")
                color: badge.model.phase === "error" ? Kirigami.Theme.negativeTextColor : (badge.model.phase === "locked" ? Kirigami.Theme.neutralTextColor : (badge.model.phase === "ok" ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.highlightColor))
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
    Controls.ToolTip.visible: (hovered || visualFocus) && model.detail.length > 0
    Controls.ToolTip.text: model.detail
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
}
