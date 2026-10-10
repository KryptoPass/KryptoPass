pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Item {
    id: root

    property Item overlayItem: Controls.Overlay.overlay
    property string message: ""
    property string severity: "info"
    property int timeoutMs: 3500
    property bool closing: false
    readonly property int enterDuration: Math.min(160, Kirigami.Units.shortDuration)
    readonly property int exitDuration: Math.min(120, Kirigami.Units.shortDuration)
    readonly property int messageType: severity === "success" ? Kirigami.MessageType.Positive
        : severity === "warning" ? Kirigami.MessageType.Warning
        : severity === "error" ? Kirigami.MessageType.Error : Kirigami.MessageType.Information

    width: 0
    height: 0

    function show(messageText, messageSeverity, durationMs) {
        const nextMessage = String(messageText || "");
        if (!nextMessage.trim()) {
            close();
            return;
        }
        message = nextMessage;
        severity = String(messageSeverity || "info").toLowerCase();
        const defaultDuration = severity === "success" ? 1800 : 3500;
        const duration = durationMs === undefined ? defaultDuration : Number(durationMs);
        timeoutMs = isFinite(duration) ? Math.max(0, duration) : 3500;
        messageView.visible = true;
        if (!popup.visible || closing) {
            closing = false;
            popup.open();
        }
        hideTimer.stop();
        if (timeoutMs > 0) {
            hideTimer.interval = timeoutMs;
            hideTimer.start();
        }
    }

    function escapedMessage(value) {
        return String(value).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }

    function close() {
        hideTimer.stop();
        if (popup.visible && !closing) {
            closing = true;
            popup.close();
        }
    }

    Timer {
        id: hideTimer
        repeat: false
        onTriggered: root.close()
    }

    Controls.Popup {
        id: popup
        parent: root.overlayItem
        x: parent ? (parent.width - width) / 2 : 0
        y: Kirigami.Units.largeSpacing
        width: parent ? Math.max(0, Math.min(parent.width - Kirigami.Units.largeSpacing * 2, Kirigami.Units.gridUnit * 40)) : Kirigami.Units.gridUnit * 32
        z: 10000
        modal: false
        focus: false
        padding: 0
        closePolicy: Controls.Popup.NoAutoClose
        enter: Transition {
            ParallelAnimation {
                NumberAnimation {
                    target: popup
                    property: "opacity"
                    from: 0
                    to: 1
                    duration: root.enterDuration
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    target: toastTranslate
                    property: "y"
                    from: -6
                    to: 0
                    duration: root.enterDuration
                    easing.type: Easing.OutCubic
                }
            }
        }
        exit: Transition {
            ParallelAnimation {
                NumberAnimation {
                    target: popup
                    property: "opacity"
                    from: 1
                    to: 0
                    duration: root.exitDuration
                    easing.type: Easing.InCubic
                }
                NumberAnimation {
                    target: toastTranslate
                    property: "y"
                    from: 0
                    to: -6
                    duration: root.exitDuration
                    easing.type: Easing.InCubic
                }
            }
        }
        contentItem: Item {
            id: toastContent
            width: popup.availableWidth
            implicitHeight: Math.max(messageView.implicitHeight, closeButton.implicitHeight)
            transform: Translate {
                id: toastTranslate
            }
            Kirigami.InlineMessage {
                id: messageView
                anchors.fill: parent
                text: "<span>" + root.escapedMessage(root.message).replace(/\n/g, "<br/>") + "</span>"
                type: root.messageType
                visible: true
                showCloseButton: false
                rightPadding: closeButton.implicitWidth + Kirigami.Units.smallSpacing
                Accessible.name: root.message
            }
            Controls.ToolButton {
                id: closeButton
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                icon.name: "window-close"
                display: Controls.AbstractButton.IconOnly
                Accessible.name: qsTr("Cerrar notificación")
                onClicked: root.close()
            }
        }
        onClosed: {
            hideTimer.stop();
            root.closing = false;
            messageView.visible = false;
        }
    }
}
