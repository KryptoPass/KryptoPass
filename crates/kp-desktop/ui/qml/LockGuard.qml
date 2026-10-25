import QtQuick

// Standard protection for a view that must not be reachable while the vault is locked.
// It closes `target` the moment the vault locks, and closes it again if anything shows it
// while still locked, so no caller has to remember to check the lock state. Declare one
// beside each protected window or dialog:
//
//     LockGuard { target: devicesWindow; locked: store.controller.locked }
//
// `target` is a Window (ApplicationWindow) or a Popup (Dialog).
Item {
    id: guard
    required property var target
    required property bool locked

    visible: false
    width: 0
    height: 0

    function enforce() {
        if (!locked || !target)
            return;
        if (target.visible === true || target.opened === true)
            target.close();
    }

    onLockedChanged: enforce()
    Component.onCompleted: enforce()

    Connections {
        target: guard.target
        ignoreUnknownSignals: true
        function onVisibleChanged() {
            guard.enforce();
        }
        function onOpenedChanged() {
            guard.enforce();
        }
    }
}
