pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

Controls.ApplicationWindow {
    id: root
    width: Kirigami.Units.gridUnit * 72
    height: Kirigami.Units.gridUnit * 45
    minimumWidth: Kirigami.Units.gridUnit * 24
    minimumHeight: Kirigami.Units.gridUnit * 24
    visible: true
    title: Qt.application.name
    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true
    Backend.ShellController {
        id: shell
    }
    Backend.PairingLabController {
        id: deviceController
    }
    Component.onDestruction: deviceController.shutdownResumeOwner()
    SecretStore {
        id: store
        objectName: "secretStore"
    }
    Timer {
        interval: 250
        repeat: true
        running: true
        onTriggered: store.controller.refreshBrowserPairing()
    }
    BrowserAccessWindow {
        id: browserAccessWindow
        requestJson: store.controller.browserAccess
        entryMetadata: store.entries
        kindOptions: store.kindOptions
        folders: store.folders
        folderRows: store.folderRows
        vaultLocked: store.controller.locked
        onUsernameRequested: function (token, entryId) {
            store.controller.requestBrowserUsername(token, entryId);
        }
        onDecisionRequested: function (token, entryId, approve) {
            store.controller.decideBrowserAccess(token, entryId, approve);
        }
    }
    BrowserPairingDialog {
        id: browserPairingDialog
        requestToken: store.controller.browserPairing
        vaultLocked: store.controller.locked
        onDecisionRequested: function (token, approve) {
            store.controller.decideBrowserPairing(token, approve);
        }
    }
    DevicePairingLabDialog {
        id: devicePairingLab
        controller: deviceController
        Component.onCompleted: {
            if (available && Qt.application.arguments.indexOf("--pairing-lab") !== -1)
                open();
        }
        onCopyRequested: function (code) {
            secureClipboard.copySensitive(code);
        }
    }
    KeyStore {
        id: keys
        objectName: "keyStore"
    }
    property bool keysMode: false
    property bool allowClose: false
    property string pendingOperation: ""
    property string pendingOperationId: ""
    property string pendingSensitiveAction: ""
    property url pendingVaultPath: ""
    property string closeMessage: ""
    readonly property bool storeOperationBusy: store.controller.busy || store.controller.editorBusy || store.controller.saving
    readonly property bool mutationsBlocked: storeOperationBusy || backupDialog.busy || importWindow.busy || editor.savePending || folderEditor.pending
    readonly property bool sensitiveTransitionBlocked: store.controller.busy || store.controller.saving || backupDialog.busy || importWindow.busy || editor.savePending || folderEditor.pending || keys.controller.operationBusy
    readonly property bool localStoreErrorVisible: (editor.opened && !!editor.saveError) || (folderEditor.opened && !!folderEditor.saveError) || (moveDialog.opened && !!moveDialog.operationError) || (folderMoveDialog.opened && !!folderMoveDialog.operationError) || (deleteEntryDialog.opened && !!deleteEntryDialog.operationError) || (deleteFolderDialog.opened && !!deleteFolderDialog.operationError) || (passwordGenerator.opened && !!passwordGenerator.operationError)
    readonly property var activeStore: keysMode ? keys : store
    function selectSection(showKeys) {
        if (keysMode === showKeys) {
            panes.showPane(1);
            return;
        }
        editor.guardNavigation(function () {
            root.applySection(showKeys);
        });
    }
    function selectScope(showKeys, scopeId) {
        if (keysMode === showKeys && activeStore.scope === scopeId) {
            panes.showPane(1);
            return;
        }
        editor.guardNavigation(function () {
            root.applySection(showKeys);
            root.activeStore.selectScope(scopeId);
            panes.showPane(1);
        });
    }
    function syncFavoriteActionCheck() {
        favoriteEntryAction.checked = Qt.binding(() => !!store.selected?.favorite);
    }
    function syncSectionActionChecks() {
        showVaultAction.checked = Qt.binding(() => !root.keysMode);
        showKeysAction.checked = Qt.binding(() => root.keysMode);
    }
    function syncScopeActionChecks() {
        showAllAction.checked = Qt.binding(() => !root.keysMode && store.scope === "all");
        showFavoritesAction.checked = Qt.binding(() => !root.keysMode && store.scope === "favorites");
        showTrashAction.checked = Qt.binding(() => !root.keysMode && store.scope === "trash");
    }
    function applySection(showKeys) {
        if (keysMode === showKeys)
            return;
        keysMode = showKeys;
        search.text = activeStore.query;
        panes.showPane(1);
        if (showKeys) {
            store.controller.selectEntry("");
            keys.controller.refreshKeys();
            keys.contactsController.refreshContacts();
        } else {
            store.controller.selectEntry(store.selectedId);
        }
    }
    function navigateToRelatedEntry(entry) {
        if (!entry)
            return;
        const target = store.entries.find(candidate => candidate.id === entry.id);
        if (!target)
            return;
        store.navigateToEntry(target);
    }
    function beginOperation(name, id) {
        pendingOperation = name;
        pendingOperationId = id || "";
    }
    function cancelSensitiveTransition() {
        pendingSensitiveAction = "";
        pendingVaultPath = "";
    }
    function unlockVault() {
        store.controller.unlockVault();
        if (!keysMode)
            store.controller.selectEntry(store.selectedId);
    }
    function requestLock() {
        if (store.controller.locked) {
            unlockVault();
            return;
        }
        if (sensitiveTransitionBlocked || passwordGenerator.generating) {
            showCloseMessage(qsTr("Espera a que termine la operación actual antes de bloquear la bóveda."));
            return;
        }
        pendingSensitiveAction = "lock";
        pendingVaultPath = "";
        continueSensitiveTransition();
    }
    function requestActiveVault(path) {
        if (sensitiveTransitionBlocked || passwordGenerator.generating) {
            showCloseMessage(qsTr("Espera a que termine la operación actual antes de cambiar de bóveda."));
            return;
        }
        pendingSensitiveAction = "switch";
        pendingVaultPath = path;
        continueSensitiveTransition();
    }
    function continueSensitiveTransition() {
        if (pendingSensitiveAction === "lock" && folderEditor.opened) {
            folderEditor.requestVaultLock();
            return;
        }
        if (pendingSensitiveAction === "switch" && folderEditor.opened) {
            folderEditor.requestVaultSwitch();
            return;
        }

        if (passwordGenerator.opened)
            passwordGenerator.clearSensitiveState();
        if (moveDialog.opened)
            moveDialog.close();
        if (folderMoveDialog.opened)
            folderMoveDialog.close();
        if (deleteEntryDialog.opened)
            deleteEntryDialog.close();
        if (deleteFolderDialog.opened)
            deleteFolderDialog.close();
        if (aboutDialog.opened)
            aboutDialog.close();
        if (keyErrorDialog.opened)
            keyErrorDialog.close();
        if (importWindow.opened)
            importWindow.close();
        if (backupDialog.opened)
            backupDialog.close();
        if (mainKeyOperationDialog.opened)
            mainKeyOperationDialog.close();

        if (!editor.opened || editor.waitingForEditor) {
            completeSensitiveTransition();
            return;
        }
        editor.guardNavigation(function () {
            root.completeSensitiveTransition();
        });
    }
    function completeSensitiveTransition() {
        if (sensitiveTransitionBlocked || passwordGenerator.generating) {
            cancelSensitiveTransition();
            showCloseMessage(qsTr("Espera a que termine la operación actual antes de continuar."));
            return;
        }
        const action = pendingSensitiveAction;
        const path = pendingVaultPath;
        cancelSensitiveTransition();
        if (action === "lock" || action === "switch")
            editor.closeAndClear();
        if (action === "lock") {
            if (folderEditor.opened)
                folderEditor.clearAndClose();
            if (passwordGenerator.opened)
                passwordGenerator.clearSensitiveState();
            secureClipboard.clearOwned();
            store.controller.lockVault();
        } else if (action === "switch") {
            store.controller.setActiveVault(path);
        }
    }
    function failPendingOperation() {
        if (pendingOperation === "generate")
            passwordGenerator.rejectGeneration(store.controller.errorMessage);
        pendingOperation = "";
        pendingOperationId = "";
    }
    function showCloseMessage(message) {
        closeMessage = "";
        if (passwordGenerator.opened)
            passwordGenerator.operationError = message;
        else if (folderEditor.opened)
            folderEditor.saveError = message;
        else if (moveDialog.opened)
            moveDialog.operationError = message;
        else if (deleteEntryDialog.opened)
            deleteEntryDialog.operationError = message;
        else if (deleteFolderDialog.opened)
            deleteFolderDialog.operationError = message;
        else if (editor.opened)
            editor.saveError = message;
        else {
            closeMessage = message;
            closeMessageTimer.restart();
        }
    }
    function notify(message, severity, timeoutMs) {
        toastHost.show(message, severity, timeoutMs);
    }
    function approveWindowClose() {
        if (sensitiveTransitionBlocked) {
            showCloseMessage(qsTr("Espera a que termine la operación actual antes de salir."));
            return;
        }
        editor.closeAndClear();
        if (folderEditor.opened)
            folderEditor.clearAndClose();
        if (passwordGenerator.opened)
            passwordGenerator.close();
        store.controller.clearSensitiveState();
        secureClipboard.clearOwned();
        allowClose = true;
        close();
    }
    readonly property bool revealed: !!store.detail?.content?.fields?.passwordRevealed
    readonly property bool selectedEntryActionsEnabled: !root.keysMode && !store.controller.locked && !editor.opened && !editor.waitingForEditor && !root.mutationsBlocked && !store.controller.detailBusy && !!store.selected
    readonly property var selectedDetailForActions: store.selected && store.detail && store.detail.id === store.selected.id && !store.controller.detailBusy ? store.detail : null
    readonly property bool selectedDetailCopyActionsEnabled: root.selectedEntryActionsEnabled && !store.controller.detailBusy && !!root.selectedDetailForActions && !store.selected.trashed
    readonly property string scopeTitle: keysMode ? (keys.scope === "own" ? qsTr("Claves propias") : keys.scope === "contacts" ? qsTr("Contactos") : qsTr("Todas las claves")) : store.scope === "all" ? qsTr("Todos los elementos") : store.scope === "favorites" ? qsTr("Favoritos") : store.scope === "trash" ? qsTr("Papelera") : store.folderName(store.scope)
    Component.onCompleted: {
        deviceController.startResumeOwner();
        store.controller.loadStore();
        keys.controller.refreshKeys();
        if (shell.launchBackup.toString())
            Qt.callLater(function () {
                backupDialog.openForRestoreFile(shell.launchBackup);
            });
        if (Qt.application.arguments.indexOf("--screenshot-about") !== -1)
            Qt.callLater(function () {
                aboutDialog.open();
            });
    }
    onClosing: function (close) {
        if (root.allowClose) {
            store.controller.clearSensitiveState();
            secureClipboard.clearOwned();
            return;
        }
        if (root.sensitiveTransitionBlocked || passwordGenerator.generating) {
            close.accepted = false;
            root.showCloseMessage(qsTr("Espera a que termine la operación actual antes de salir."));
            return;
        }
        if (folderEditor.opened && folderEditor.dirty) {
            close.accepted = false;
            root.showCloseMessage(qsTr("Guarda o descarta los cambios de carpeta antes de salir."));
            return;
        }
        if (editor.waitingForEditor) {
            close.accepted = false;
            editor.closeAndClear();
            root.approveWindowClose();
            return;
        }
        if (editor.opened || editor.loaded) {
            close.accepted = false;
            if (passwordGenerator.opened)
                passwordGenerator.close();
            editor.requestWindowClose();
            return;
        }
        if (folderEditor.opened)
            folderEditor.clearAndClose();
        if (passwordGenerator.opened)
            passwordGenerator.close();
        store.controller.clearSensitiveState();
        secureClipboard.clearOwned();
    }
    Timer {
        id: closeMessageTimer
        interval: 5000
        onTriggered: root.closeMessage = ""
    }
    Kirigami.InlineMessage {
        z: 1000
        anchors.top: parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(parent.width - Kirigami.Units.largeSpacing * 2, Kirigami.Units.gridUnit * 40)
        visible: !!root.closeMessage
        type: Kirigami.MessageType.Warning
        text: root.closeMessage
    }
    Connections {
        target: store
        function onQueryChanged() {
            if (!root.keysMode && search.text !== store.query)
                search.text = store.query;
        }
    }
    Connections {
        target: keys
        function onQueryChanged() {
            if (root.keysMode && search.text !== keys.query)
                search.text = keys.query;
        }
    }
    Connections {
        target: shell
        function onFileManagerFailed() {
            root.notify(qsTr("No se pudo abrir la carpeta del respaldo."), "error");
        }
    }
    Connections {
        target: store.controller
        function onClipboardRequested(value) {
            const copied = secureClipboard.copySensitive(value);
            store.controller.confirmCopy(copied);
            if (copied)
                root.notify(qsTr("Copiado al portapapeles"), "success");
        }
        function onActivityExported(path) {
            if (!Qt.openUrlExternally(path))
                root.notify(qsTr("El archivo de actividad se generó, pero no se pudo abrir el editor de texto."), "warning");
        }
        function onHistoryRecordFailed() {
            root.notify(qsTr("La acción se completó, pero no se pudo guardar en el historial."), "warning");
        }
        function onPasswordGenerated(value) {
            if (root.pendingOperation !== "generate")
                return;
            root.pendingOperation = "";
            root.pendingOperationId = "";
            editor.applyGeneratedPassword(value);
            passwordGenerator.acceptGenerated();
        }
        function onOperationSucceeded() {
            const operation = root.pendingOperation;
            const operationId = root.pendingOperationId;
            root.pendingOperation = "";
            root.pendingOperationId = "";
            if (operation === "restore") {
                store.scope = "all";
                store.query = "";
                if (store.selectedId !== operationId)
                    store.selectedId = operationId;
                else if (operationId)
                    store.controller.selectEntry(operationId);
            }
        }
        function onOperationFailed() {
            if (store.controller.busy)
                return;
            if (root.pendingSensitiveAction === "switch")
                root.cancelSensitiveTransition();
            root.failPendingOperation();
        }
        function onVaultLocked() {
            root.cancelSensitiveTransition();
            root.pendingOperation = "";
            root.pendingOperationId = "";
            secureClipboard.clearOwned();
            importWindow.clearSensitiveState();
            editor.closeAndClear();
            if (folderEditor.opened)
                folderEditor.clearAndClose();
            if (passwordGenerator.opened)
                passwordGenerator.clearSensitiveState();
        }
        function onVaultActivated() {
            root.keysMode = false;
            store.scope = "all";
            store.query = "";
            store.selectedId = "";
            search.text = "";
        }
    }

    Backend.SensitiveClipboard {
        id: secureClipboard
        objectName: "sensitiveClipboard"
    }

    ToastHost {
        id: toastHost
        overlayItem: Controls.Overlay.overlay
    }

    DesktopAction {
        id: newEntry
        controller: shell
        actionId: "newEntry"
        enabled: !root.keysMode && !root.mutationsBlocked && !store.controller.locked
        onTriggered: editor.openEntry(null)
    }
    DesktopAction {
        id: editEntry
        controller: shell
        actionId: "editEntry"
        enabled: !root.keysMode && !store.controller.locked && !editor.opened && !editor.waitingForEditor && !root.mutationsBlocked && !store.controller.detailBusy && !!store.selected && !store.selected.trashed
        onTriggered: editor.openEntry(store.selected)
    }
    DesktopAction {
        id: trashEntry
        controller: shell
        actionId: "trashEntry"
        enabled: !root.keysMode && !store.controller.locked && !editor.opened && !editor.waitingForEditor && !root.mutationsBlocked && !store.controller.detailBusy && !!store.selected && !store.selected.trashed
        onTriggered: {
            root.beginOperation("trash", store.selectedId);
            store.updateEntry(store.selectedId, {
                trashed: true
            });
        }
    }
    DesktopAction {
        id: newFolder
        controller: shell
        actionId: "newFolder"
        enabled: !root.keysMode && !store.controller.locked && !editor.opened && !root.mutationsBlocked
        onTriggered: folderEditor.openFolder(false)
    }
    DesktopAction {
        id: refreshKeys
        controller: shell
        actionId: "refreshKeys"
        enabled: root.keysMode && !keys.controller.busy && !keys.controller.operationBusy
        onTriggered: keys.controller.refreshKeys()
    }
    DesktopAction {
        id: createKey
        controller: shell
        actionId: "createKey"
        enabled: root.keysMode && !keys.controller.busy && !keys.controller.operationBusy
        onTriggered: mainKeyOperationDialog.openFor("create")
    }
    DesktopAction {
        id: importKey
        controller: shell
        actionId: "importKey"
        enabled: root.keysMode && !keys.controller.busy && !keys.controller.operationBusy
        onTriggered: mainKeyOperationDialog.openFor("import")
    }
    DesktopAction {
        id: importVaultAction
        controller: shell
        actionId: "importVault"
        enabled: !root.mutationsBlocked
        onTriggered: importWindow.prepare()
    }
    DesktopAction {
        id: exportBackupAction
        controller: shell
        actionId: "exportBackup"
        enabled: !root.mutationsBlocked
        onTriggered: backupDialog.openForExport()
    }
    DesktopAction {
        id: restoreBackupAction
        controller: shell
        actionId: "restoreBackup"
        enabled: !root.mutationsBlocked
        onTriggered: backupDialog.openForRestore()
    }
    DesktopAction {
        id: showBackupInFolderAction
        controller: shell
        actionId: "showBackupInFolder"
        enabled: backupDialog.opened && !backupDialog.busy && backupDialog.exportMode && !!backupDialog.completedExportPath.toString()
        onTriggered: shell.showInFolder(backupDialog.completedExportPath)
    }
    DesktopAction {
        id: copyFingerprint
        controller: shell
        actionId: "copyFingerprint"
        enabled: root.keysMode && !!keys.selected && !keys.controller.busy && !keys.controller.operationBusy
        onTriggered: entryDetailPane.copySelectedFingerprint()
    }
    DesktopAction {
        id: moveEntryAction
        controller: shell
        actionId: "moveEntry"
        enabled: editEntry.enabled
        onTriggered: moveDialog.openMove()
    }
    DesktopAction {
        id: favoriteEntryAction
        controller: shell
        actionId: "toggleFavorite"
        checkable: true
        checked: !!store.selected?.favorite
        enabled: root.selectedEntryActionsEnabled && !store.selected.trashed
        onTriggered: {
            if (!root.selectedEntryActionsEnabled || !store.selected || store.selected.trashed) {
                root.syncFavoriteActionCheck();
                return;
            }
            root.beginOperation("favorite", store.selectedId);
            store.updateEntry(store.selectedId, {
                favorite: !store.selected.favorite
            });
            root.syncFavoriteActionCheck();
        }
    }
    DesktopAction {
        id: restoreEntryAction
        controller: shell
        actionId: "restoreEntry"
        enabled: root.selectedEntryActionsEnabled && store.selected.trashed
        onTriggered: {
            if (!root.selectedEntryActionsEnabled || !store.selected || !store.selected.trashed)
                return;
            root.beginOperation("restore", store.selectedId);
            store.updateEntry(store.selectedId, {
                trashed: false
            });
        }
    }
    DesktopAction {
        id: deleteEntryAction
        controller: shell
        actionId: "deleteEntry"
        enabled: root.selectedEntryActionsEnabled && store.selected.trashed
        onTriggered: {
            if (root.selectedEntryActionsEnabled && store.selected?.trashed)
                deleteEntryDialog.openForSelected();
        }
    }
    DesktopAction {
        id: copyUsername
        controller: shell
        actionId: "copyUsername"
        enabled: root.selectedDetailCopyActionsEnabled && root.selectedDetailForActions.content?.kind === "login" && !!root.selectedDetailForActions.content?.fields?.username
        onTriggered: store.controller.copyUsername()
    }
    DesktopAction {
        id: copyPassword
        controller: shell
        actionId: "copyPassword"
        enabled: root.selectedDetailCopyActionsEnabled && root.selectedDetailForActions.content?.kind === "login"
        onTriggered: store.controller.copyPassword()
    }
    DesktopAction {
        id: copyTotp
        controller: shell
        actionId: "copyTotp"
        enabled: root.selectedDetailCopyActionsEnabled && (root.selectedDetailForActions.content?.kind === "login" || root.selectedDetailForActions.content?.kind === "totp") && !!root.selectedDetailForActions.content?.fields?.totp?.currentCode
        onTriggered: store.controller.copyTotp(false)
    }
    DesktopAction {
        id: lockVaultAction
        controller: shell
        actionId: "lockVault"
        text: store.controller.locked ? qsTr("Desbloquear bóveda") : qsTr("Bloquear bóveda")
        icon.name: store.controller.locked ? "object-unlocked" : "object-locked"
        enabled: !root.sensitiveTransitionBlocked && !passwordGenerator.generating
        onTriggered: root.requestLock()
    }
    DesktopAction {
        id: showVaultAction
        controller: shell
        actionId: "showVault"
        checkable: true
        checked: !root.keysMode
        enabled: !root.mutationsBlocked
        onTriggered: {
            root.selectSection(false);
            root.syncSectionActionChecks();
        }
    }
    DesktopAction {
        id: showKeysAction
        controller: shell
        actionId: "showKeys"
        checkable: true
        checked: root.keysMode
        enabled: !root.mutationsBlocked
        onTriggered: {
            root.selectSection(true);
            root.syncSectionActionChecks();
        }
    }
    DesktopAction {
        id: showAllAction
        controller: shell
        actionId: "showAll"
        checkable: true
        checked: !root.keysMode && store.scope === "all"
        enabled: !root.mutationsBlocked
        onTriggered: {
            root.selectScope(false, "all");
            root.syncScopeActionChecks();
        }
    }
    DesktopAction {
        id: showFavoritesAction
        controller: shell
        actionId: "showFavorites"
        checkable: true
        checked: !root.keysMode && store.scope === "favorites"
        enabled: !root.mutationsBlocked
        onTriggered: {
            root.selectScope(false, "favorites");
            root.syncScopeActionChecks();
        }
    }
    DesktopAction {
        id: showTrashAction
        controller: shell
        actionId: "showTrash"
        checkable: true
        checked: !root.keysMode && store.scope === "trash"
        enabled: !root.mutationsBlocked
        onTriggered: {
            root.selectScope(false, "trash");
            root.syncScopeActionChecks();
        }
    }
    Controls.ActionGroup {
        id: sectionActionGroup
        exclusive: true
        actions: [showVaultAction, showKeysAction]
    }
    Controls.ActionGroup {
        id: scopeActionGroup
        exclusive: true
        actions: [showAllAction, showFavoritesAction, showTrashAction]
    }
    DesktopAction {
        id: showDevicesAction
        controller: shell
        actionId: "showDevices"
        enabled: devicePairingLab.available
        onTriggered: devicePairingLab.open()
    }
    readonly property bool folderActionsEnabled: !root.keysMode && !root.mutationsBlocked && !store.controller.locked && !editor.opened && !folderEditor.opened && store.folders.some(folder => folder.id === store.scope)
    DesktopAction {
        id: folderUp
        controller: shell
        actionId: "folderUp"
        enabled: root.folderActionsEnabled
        onTriggered: store.controller.stepFolder(store.scope, -1)
    }
    DesktopAction {
        id: folderDown
        controller: shell
        actionId: "folderDown"
        enabled: root.folderActionsEnabled
        onTriggered: store.controller.stepFolder(store.scope, 1)
    }
    DesktopAction {
        id: moveFolderAction
        controller: shell
        actionId: "moveFolder"
        enabled: root.folderActionsEnabled
        onTriggered: folderMoveDialog.prepare(store.scope)
    }
    DesktopAction {
        id: findAction
        controller: shell
        actionId: "find"
        enabled: !editor.opened
        onTriggered: {
            panes.showPane(1);
            Qt.callLater(function () {
                search.forceActiveFocus(Qt.ShortcutFocusReason);
                search.selectAll();
            });
        }
    }
    DesktopAction {
        id: quitAction
        controller: shell
        actionId: "quit"
        onTriggered: root.close()
    }
    DesktopAction {
        id: configureShortcutsAction
        controller: shell
        actionId: "configureShortcuts"
        onTriggered: shell.configureShortcuts()
    }
    DesktopAction {
        id: helpAction
        controller: shell
        actionId: "help"
        onTriggered: helpDialog.open()
    }
    DesktopAction {
        id: reportBugAction
        controller: shell
        actionId: "reportBug"
        onTriggered: {
            if (!shell.openBugReport())
                root.notify(qsTr("No se pudo abrir el gestor de incidencias."), "error");
        }
    }
    DesktopAction {
        id: aboutAppAction
        controller: shell
        actionId: "aboutApp"
        onTriggered: aboutDialog.open()
    }
    DesktopAction {
        id: aboutKdeAction
        controller: shell
        actionId: "aboutKde"
        onTriggered: shell.showAboutKdeDialog()
    }
    menuBar: Controls.MenuBar {
        Controls.Menu {
            title: qsTr("&Archivo")
            Controls.MenuItem {
                action: newEntry
            }
            Controls.MenuItem {
                action: newFolder
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: createKey
            }
            Controls.MenuItem {
                action: importKey
            }
            Controls.MenuItem {
                action: refreshKeys
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: importVaultAction
            }
            Controls.MenuItem {
                action: exportBackupAction
            }
            Controls.MenuItem {
                action: restoreBackupAction
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: quitAction
            }
        }
        Controls.Menu {
            title: qsTr("&Editar")
            Controls.MenuItem {
                action: findAction
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: editEntry
            }
            Controls.MenuItem {
                action: moveEntryAction
            }
            Controls.MenuItem {
                action: trashEntry
            }
            Controls.MenuItem {
                action: favoriteEntryAction
            }
            Controls.MenuItem {
                action: restoreEntryAction
            }
            Controls.MenuItem {
                action: deleteEntryAction
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: copyUsername
            }
            Controls.MenuItem {
                action: copyPassword
            }
            Controls.MenuItem {
                action: copyTotp
            }
            Controls.MenuItem {
                action: copyFingerprint
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: folderUp
            }
            Controls.MenuItem {
                action: folderDown
            }
            Controls.MenuItem {
                action: moveFolderAction
            }
        }
        Controls.Menu {
            title: qsTr("&Ver")
            Controls.MenuItem {
                action: showVaultAction
            }
            Controls.MenuItem {
                action: showKeysAction
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: showAllAction
            }
            Controls.MenuItem {
                action: showFavoritesAction
            }
            Controls.MenuItem {
                action: showTrashAction
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: lockVaultAction
            }
        }
        Controls.Menu {
            title: qsTr("&Preferencias")
            Controls.Menu {
                title: qsTr("Bloqueo por inactividad")
                Controls.MenuItem {
                    text: qsTr("Desactivado")
                    checkable: true
                    autoExclusive: true
                    checked: shell.idleLockSeconds === 0
                    onTriggered: shell.setIdleLockSeconds(0)
                }
                Controls.MenuItem {
                    text: qsTr("1 minuto")
                    checkable: true
                    autoExclusive: true
                    checked: shell.idleLockSeconds === 60
                    onTriggered: shell.setIdleLockSeconds(60)
                }
                Controls.MenuItem {
                    text: qsTr("5 minutos")
                    checkable: true
                    autoExclusive: true
                    checked: shell.idleLockSeconds === 300
                    onTriggered: shell.setIdleLockSeconds(300)
                }
                Controls.MenuItem {
                    text: qsTr("15 minutos")
                    checkable: true
                    autoExclusive: true
                    checked: shell.idleLockSeconds === 900
                    onTriggered: shell.setIdleLockSeconds(900)
                }
            }
            Controls.Menu {
                title: qsTr("Autolimpieza del portapapeles")
                Controls.MenuItem {
                    text: qsTr("Desactivada")
                    checkable: true
                    checked: secureClipboard.autoClearSeconds === 0
                    onTriggered: secureClipboard.autoClearSeconds = 0
                }
                Controls.MenuItem {
                    text: qsTr("15 segundos")
                    checkable: true
                    checked: secureClipboard.autoClearSeconds === 15
                    onTriggered: secureClipboard.autoClearSeconds = 15
                }
                Controls.MenuItem {
                    text: qsTr("30 segundos")
                    checkable: true
                    checked: secureClipboard.autoClearSeconds === 30
                    onTriggered: secureClipboard.autoClearSeconds = 30
                }
                Controls.MenuItem {
                    text: qsTr("60 segundos")
                    checkable: true
                    checked: secureClipboard.autoClearSeconds === 60
                    onTriggered: secureClipboard.autoClearSeconds = 60
                }
            }
            Controls.MenuItem {
                action: configureShortcutsAction
            }
            Controls.MenuItem {
                action: showDevicesAction
                visible: devicePairingLab.available
            }
        }
        Controls.Menu {
            title: qsTr("A&yuda")
            Controls.MenuItem {
                action: helpAction
            }
            Controls.MenuItem {
                action: reportBugAction
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: aboutAppAction
            }
            Controls.MenuItem {
                action: aboutKdeAction
            }
        }
    }
    readonly property real panelHeaderHeight: Math.max(Kirigami.Units.gridUnit * 2.5, toolbarSizeHint.implicitHeight + Kirigami.Units.smallSpacing * 2)
    Controls.ToolButton {
        id: toolbarSizeHint
        visible: false
        action: refreshKeys
    }

    MoveFolderDialog {
        id: folderMoveDialog
        storeAdapter: store
    }

    AdaptivePanes {
        id: panes
        anchors.fill: parent
        navigationContent: SidebarPane {
            anchors.fill: parent
            focus: true
            showBackButton: panes.compact
            onBackRequested: panes.showPane(1)
            entryDraggingEnabled: !root.mutationsBlocked && !store.controller.locked && !editor.opened && !editor.waitingForEditor
            onFolderMoveRequested: function (folderId, parentId, beforeId) {
                if (root.mutationsBlocked || store.controller.locked || editor.opened || editor.waitingForEditor)
                    return;
                root.beginOperation("moveFolder", folderId);
                store.controller.moveFolder(folderId, parentId, beforeId);
            }
            onEntryMoveRequested: function (entryId, folderId) {
                if (root.mutationsBlocked || store.controller.locked || editor.opened || editor.waitingForEditor)
                    return;
                root.beginOperation("move", entryId);
                store.updateEntry(entryId, {
                    folder: folderId
                });
            }
            keysMode: root.keysMode
            activeStore: root.activeStore
            storeAdapter: store
            headerHeight: root.panelHeaderHeight
            newFolderAction: newFolder
            onSectionSelectionRequested: function (showKeys) {
                root.selectSection(showKeys);
            }
            onScopeSelectionRequested: function (scopeId) {
                root.selectScope(root.keysMode, scopeId);
            }
            onRenameFolderRequested: {
                if (!root.mutationsBlocked && !store.controller.locked)
                    editor.guardNavigation(function () {
                        folderEditor.openFolder(true);
                    });
            }
            onDeleteFolderRequested: {
                if (!root.mutationsBlocked && !store.controller.locked)
                    editor.guardNavigation(function () {
                        deleteFolderDialog.openForSelected();
                    });
            }
        }
        headerContent: ColumnLayout {
            width: parent.width
            spacing: 0
            PanelHeader {
                Layout.fillWidth: true
                headerHeight: root.panelHeaderHeight
                contentItem: RowLayout {
                    spacing: Kirigami.Units.smallSpacing
                    Controls.ToolButton {
                        text: qsTr("Mostrar navegación")
                        icon.name: "sidebar-expand"
                        onClicked: panes.showPane(0)
                        visible: panes.compact && panes.currentPane === 1
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                    Controls.ToolButton {
                        text: qsTr("Volver a la lista")
                        icon.name: "go-previous"
                        onClicked: panes.showPane(1)
                        visible: panes.compact && panes.currentPane === 2
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                    Kirigami.SearchField {
                        id: search
                        enabled: !editor.opened
                        Layout.fillWidth: true
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                        placeholderText: root.keysMode ? qsTr("Buscar nombre, email o huella…") : qsTr("Buscar en esta vista…")
                        onTextChanged: root.activeStore.query = text
                    }
                    Controls.ToolButton {
                        text: qsTr("Mostrar detalle")
                        icon.name: "view-preview"
                        onClicked: panes.showPane(2)
                        visible: panes.compact && panes.currentPane === 1
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    Controls.ToolButton {
                        action: newEntry
                        visible: !root.keysMode
                        display: panes.compact ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                    Controls.ToolButton {
                        visible: !root.keysMode && !!store.selected && !store.selected.trashed
                        text: qsTr("Mover a otra carpeta…")
                        icon.name: "folder-move"
                        enabled: editEntry.enabled
                        display: panes.compact ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                        onClicked: moveDialog.openMove()
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                        Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    Controls.ToolButton {
                        action: refreshKeys
                        visible: root.keysMode
                        display: panes.compact ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                    Controls.ToolButton {
                        action: createKey
                        visible: root.keysMode
                        display: panes.compact ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                    Controls.ToolButton {
                        action: importKey
                        visible: root.keysMode
                        display: panes.compact ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                    Controls.ToolButton {
                        action: lockVaultAction
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                        Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    Controls.BusyIndicator {
                        visible: root.keysMode && (keys.controller.busy || keys.controller.operationBusy)
                        running: visible
                        Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                        Layout.preferredHeight: width
                        Accessible.name: keys.controller.operationBusy ? qsTr("Procesando operación OpenPGP") : qsTr("Actualizando claves de GnuPG")
                    }
                }
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: root.keysMode && !!keys.controller.loadError && !keyErrorDialog.opened
                position: Kirigami.InlineMessage.Position.Header
                type: Kirigami.MessageType.Error
                text: qsTr("No se pudieron cargar las claves de GnuPG.")
                actions: [
                    Kirigami.Action {
                        text: qsTr("Reintentar")
                        icon.name: "view-refresh"
                        enabled: refreshKeys.enabled
                        onTriggered: keys.controller.refreshKeys()
                    },
                    Kirigami.Action {
                        text: qsTr("Ver detalles…")
                        icon.name: "dialog-information"
                        onTriggered: keyErrorDialog.open()
                    }
                ]
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!secureClipboard.errorMessage
                position: Kirigami.InlineMessage.Position.Header
                type: Kirigami.MessageType.Error
                text: secureClipboard.errorMessage
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!store.controller.errorMessage && !root.localStoreErrorVisible && !root.closeMessage
                position: Kirigami.InlineMessage.Position.Header
                type: Kirigami.MessageType.Error
                text: store.controller.errorMessage
                actions: [
                    Kirigami.Action {
                        text: qsTr("Reintentar")
                        icon.name: "view-refresh"
                        enabled: !root.storeOperationBusy
                        onTriggered: {
                            if (store.selectedId && !store.controller.locked)
                                store.controller.selectEntry(store.selectedId);
                            else
                                store.controller.loadStore();
                        }
                    }
                ]
            }
        }
        listContent: EntryListPane {
            id: entryListPane
            anchors.fill: parent
            focus: true
            onDetailRequested: panes.showPane(2)
            contextMenuEnabled: !editor.opened && !root.mutationsBlocked
            entryDraggingEnabled: !root.keysMode && !root.mutationsBlocked && !store.controller.locked && !editor.opened && !editor.waitingForEditor
            searchEnabled: search.enabled
            keysMode: root.keysMode
            activeStore: root.activeStore
            storeAdapter: store
            keyStore: keys
            scopeTitle: root.scopeTitle
            headerHeight: root.panelHeaderHeight
            editAction: editEntry
            moveAction: moveEntryAction
            trashAction: trashEntry
            favoriteAction: favoriteEntryAction
            restoreAction: restoreEntryAction
            deleteAction: deleteEntryAction
            copyUsernameAction: copyUsername
            copyPasswordAction: copyPassword
            copyTotpAction: copyTotp
            copyFingerprintAction: copyFingerprint
            onSelectionRequested: function (entryId) {
                if (root.activeStore.selectedId === entryId)
                    return;
                editor.guardNavigation(function () {
                    root.activeStore.selectedId = entryId;
                });
            }
            onContextMenuRequested: function (entryId, x, y) {
                const requestKeysMode = root.keysMode;
                const requestStore = root.activeStore;
                if (root.mutationsBlocked || editor.opened || editor.waitingForEditor)
                    return;
                if (requestKeysMode ? (keys.controller.busy || keys.controller.operationBusy) : store.controller.locked)
                    return;
                editor.guardNavigation(function () {
                    if (root.mutationsBlocked || editor.opened || editor.waitingForEditor || root.keysMode !== requestKeysMode || root.activeStore !== requestStore)
                        return;
                    if (requestKeysMode ? (keys.controller.busy || keys.controller.operationBusy) : store.controller.locked)
                        return;
                    requestStore.selectedId = entryId;
                    if (requestStore.selectedId === entryId)
                        entryListPane.openContextMenu(entryId, x, y);
                });
            }
            onEntryOpenRequested: function (entry) {
                if (!root.mutationsBlocked)
                    editor.guardNavigation(function () {
                        store.selectedId = entry.id;
                        editor.openEntry(entry);
                    });
            }
        }
        detailContent: EntryDetailPane {
            id: entryDetailPane
            anchors.fill: parent
            focus: true
            editing: editor.opened
            keysMode: root.keysMode
            storeAdapter: store
            keyStore: keys
            keyOperationDialog: mainKeyOperationDialog
            revealed: root.revealed
            headerHeight: root.panelHeaderHeight
            editAction: editEntry
            trashAction: trashEntry
            favoriteAction: favoriteEntryAction
            restoreAction: restoreEntryAction
            deleteAction: deleteEntryAction
            copyFingerprintAction: copyFingerprint
            onNotificationRequested: function (message) {
                root.notify(message, "success");
            }
            onFavoriteToggleRequested: favoriteEntryAction.trigger()
            onRestoreRequested: restoreEntryAction.trigger()
            onPermanentDeleteRequested: deleteEntryAction.trigger()
            onRevealToggleRequested: {
                store.controller.togglePassword();
            }
            onCopyUsernameRequested: copyUsername.trigger()
            onCopyPasswordRequested: copyPassword.trigger()
            onCopyTotpRequested: function (next) {
                if (next)
                    store.controller.copyTotp(true);
                else
                    copyTotp.trigger();
            }
            onRetryDetailRequested: store.controller.selectEntry(store.selectedId)
            onRelatedEntryOpenRequested: function (entry) {
                if (!root.mutationsBlocked)
                    editor.guardNavigation(function () {
                        root.navigateToRelatedEntry(entry);
                    });
            }
            onRelatedEntryTrashRequested: function (entry) {
                if (!root.mutationsBlocked)
                    editor.guardNavigation(function () {
                        root.navigateToRelatedEntry(entry);
                    });
            }
            onCreateSecurityRequested: function (entry) {
                if (!root.mutationsBlocked && !store.controller.locked)
                    editor.guardNavigation(function () {
                        editor.openRelatedEntry(entry);
                    });
            }
        }
        footerContent: PanelFooter {
            width: parent.width
            headerHeight: root.panelHeaderHeight
            contentItem: RowLayout {
                spacing: Kirigami.Units.largeSpacing
                Controls.Label {
                    Layout.maximumWidth: parent.width * 0.6
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: root.keysMode ? qsTr("%n clave", "", keys.visibleEntries.length) + " · " + root.scopeTitle : qsTr("%n elemento", "", store.visibleEntries.length) + " · " + root.scopeTitle
                }
                Item {
                    Layout.fillWidth: true
                }
                Controls.Label {
                    visible: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    horizontalAlignment: Text.AlignRight
                    elide: Text.ElideRight
                    text: root.keysMode ? keys.controller.status : store.controller.status
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.BusyIndicator {
                    visible: !root.keysMode && (root.storeOperationBusy || backupDialog.busy || importWindow.busy)
                    running: visible
                    Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                    Layout.preferredHeight: width
                    Accessible.name: qsTr("Procesando la bóveda")
                }
            }
        }
    }
    SecretEditor {
        id: editor
        onOpenedChanged: {
            if (opened)
                panes.showPane(2);
        }
        parent: entryDetailPane.editorHost
        anchors.fill: parent
        storeAdapter: store
        keyStore: keys
        headerHeight: root.panelHeaderHeight
        focusedItem: root.activeFocusItem
        externalModalOpen: devicePairingLab.opened || browserAccessWindow.visible || browserPairingDialog.opened || passwordGenerator.opened || folderEditor.opened || moveDialog.opened || folderMoveDialog.opened || deleteEntryDialog.opened || deleteFolderDialog.opened || mainKeyOperationDialog.opened || helpDialog.opened || aboutDialog.opened || keyErrorDialog.opened || backupDialog.opened || importWindow.opened
        onSaveRequested: function (draftJson) {
            store.controller.saveEntry(draftJson);
        }
        onGeneratePasswordRequested: passwordGenerator.openGenerator()
        onWindowCloseApproved: {
            root.approveWindowClose();
        }
        onNavigationCancelled: root.cancelSensitiveTransition()
    }
    FolderEditor {
        id: folderEditor
        storeAdapter: store
        onSaveRequested: function (folderId, name, parentId, icon, color) {
            store.saveFolder(folderId, name, parentId, icon, color);
        }
        onVaultLockApproved: root.continueSensitiveTransition()
        onVaultLockCancelled: root.cancelSensitiveTransition()
        onVaultSwitchApproved: root.continueSensitiveTransition()
        onVaultSwitchCancelled: root.cancelSensitiveTransition()
    }
    MoveEntryDialog {
        id: moveDialog
        storeAdapter: store
        onMoveRequested: function (entryId, folderId) {
            root.beginOperation("move", entryId);
            store.updateEntry(entryId, {
                folder: folderId
            });
        }
    }
    DeleteFolderDialog {
        id: deleteFolderDialog
        storeAdapter: store
        onDeleteRequested: function (folderId, deleteContents) {
            root.beginOperation("removeFolder", folderId);
            store.removeFolder(folderId, deleteContents);
        }
    }
    DeleteEntryDialog {
        id: deleteEntryDialog
        storeAdapter: store
        onDeleteRequested: function (entryId) {
            root.beginOperation("removeEntry", entryId);
            store.removeEntry(entryId);
        }
    }
    PasswordGenerator {
        id: passwordGenerator
        windowWidth: root.width
        onGenerateRequested: function (length, lowercase, uppercase, digits, symbols, excluded) {
            root.beginOperation("generate", "");
            store.controller.generatePassword(length, lowercase, uppercase, digits, symbols, excluded);
        }
    }
    Backend.ImportController {
        id: importController
    }
    ImportWindow {
        id: importWindow
        transientParent: root
        controller: importController
        keyStore: keys
        onImported: store.controller.loadStore()
    }
    BackupDialog {
        id: backupDialog
        keyStore: keys
        showInFolderAction: showBackupInFolderAction
        onOpenVaultRequested: function (path) {
            Qt.callLater(function () {
                root.requestActiveVault(path);
            });
        }
    }
    KeyOperationDialog {
        id: mainKeyOperationDialog
        keyStore: keys
    }
    HelpDialog {
        id: helpDialog
    }
    AboutDialog {
        id: aboutDialog
        aboutData: JSON.parse(shell.aboutJson)
        repositoryUrl: shell.appRepository
        onLinkRequested: function (target) {
            Qt.openUrlExternally(target);
        }
    }
    KeyErrorDialog {
        id: keyErrorDialog
        controller: keys.controller
    }
}
