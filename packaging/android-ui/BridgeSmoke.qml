import QtQuick
import org.kryptopass.desktop.previewbackend as Backend

QtObject {
    id: root

    property int entryNotifications: 0
    property int folderNotifications: 0
    property int detailNotifications: 0
    property int lockNotifications: 0
    property int errorNotifications: 0
    property int operationFailures: 0
    property Backend.SecretsController controller

    controller: Backend.SecretsController {
    }

    property Connections notifications

    notifications: Connections {
        function onEntryRecordsChanged() {
            root.entryNotifications += 1;
        }

        function onFolderRecordsChanged() {
            root.folderNotifications += 1;
        }

        function onDetailJsonChanged() {
            root.detailNotifications += 1;
        }

        function onLockedChanged() {
            root.lockNotifications += 1;
        }

        function onErrorMessageChanged() {
            root.errorNotifications += 1;
        }

        function onOperationFailed() {
            root.operationFailures += 1;
        }

        target: root.controller
    }

    function verify(condition, message) {
        if (!condition)
            throw new Error(message);

    }

    function selectedDetail() {
        verify(controller.detailJson.length > 0, "selected detail is empty");
        const detail = JSON.parse(controller.detailJson);
        verify(detail.id === "1", "selected detail has an unexpected ID");
        return detail;
    }

    Component.onCompleted: {
        try {
            controller.loadStore();
            verify(controller.entryRecords.length > 0, "loadStore published no entries");
            verify(controller.folderRecords.length > 0, "loadStore published no folders");
            verify(entryNotifications > 0, "entryRecordsChanged was not emitted");
            verify(folderNotifications > 0, "folderRecordsChanged was not emitted");
            let previousDetails = detailNotifications;
            controller.selectEntry("1");
            verify(detailNotifications > previousDetails, "selection did not notify detailJsonChanged");
            let detail = selectedDetail();
            verify(detail.content.fields.password === "", "selection exposed the password");
            verify(detail.content.fields.passwordRevealed === false, "selection started revealed");
            previousDetails = detailNotifications;
            controller.togglePassword();
            verify(detailNotifications > previousDetails, "password reveal did not notify detailJsonChanged");
            detail = selectedDetail();
            verify(detail.content.fields.password.length > 0, "password reveal published no password");
            verify(detail.content.fields.passwordRevealed === true, "password reveal flag is false");
            previousDetails = detailNotifications;
            let previousLocks = lockNotifications;
            controller.lockVault();
            verify(controller.locked === true, "lockVault did not lock the controller");
            verify(controller.detailJson === "", "lockVault retained the detail projection");
            verify(detailNotifications > previousDetails, "locking did not notify detailJsonChanged");
            verify(lockNotifications > previousLocks, "locking did not notify lockedChanged");
            previousDetails = detailNotifications;
            previousLocks = lockNotifications;
            controller.unlockVault();
            verify(controller.locked === false, "unlockVault did not unlock the controller");
            verify(detailNotifications > previousDetails, "unlocking did not notify detailJsonChanged");
            verify(lockNotifications > previousLocks, "unlocking did not notify lockedChanged");
            detail = selectedDetail();
            verify(detail.content.fields.password === "", "unlockVault exposed the password");
            verify(detail.content.fields.passwordRevealed === false, "unlockVault restored revelation");
            const previousFailures = operationFailures;
            const previousErrors = errorNotifications;
            controller.saveEntry("{}");
            verify(operationFailures === previousFailures + 1, "saveEntry did not emit operationFailed");
            verify(controller.errorMessage.trim().length > 0, "saveEntry published no visible error");
            verify(errorNotifications > previousErrors, "saveEntry did not notify errorMessageChanged");
            console.info("bridge-smoke: PASS");
        } catch (error) {
            console.error("bridge-smoke: FAIL", error);
        }
    }
}
