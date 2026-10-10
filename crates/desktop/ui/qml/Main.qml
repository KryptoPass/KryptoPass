pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

Controls.ApplicationWindow {
    id: root
    width: 1280
    height: 800
    minimumWidth: 900
    minimumHeight: 580
    visible: true
    title: Qt.application.name
    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true
    SecretStore {
        id: store
        objectName: "secretStore"
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
    readonly property bool mutationsBlocked: storeOperationBusy || backupDialog.busy || editor.savePending || folderEditor.pending
    readonly property bool sensitiveTransitionBlocked: store.controller.busy || store.controller.saving || backupDialog.busy || editor.savePending || folderEditor.pending || keys.controller.operationBusy
    readonly property bool localStoreErrorVisible: (editor.opened && !!editor.saveError)
        || (folderEditor.opened && !!folderEditor.saveError)
        || (moveDialog.opened && !!moveDialog.operationError)
        || (deleteEntryDialog.opened && !!deleteEntryDialog.operationError)
        || (deleteFolderDialog.opened && !!deleteFolderDialog.operationError)
        || (passwordGenerator.opened && !!passwordGenerator.operationError)
    readonly property var activeStore: keysMode ? keys : store
    function selectSection(showKeys) {
        if (keysMode === showKeys)
            return;
        editor.guardNavigation(function() { root.applySection(showKeys); });
    }
    function selectScope(showKeys, scopeId) {
        if (keysMode === showKeys && activeStore.scope === scopeId)
            return;
        editor.guardNavigation(function() {
            root.applySection(showKeys);
            root.activeStore.selectScope(scopeId);
        });
    }
    function applySection(showKeys) {
        if (keysMode === showKeys)
            return;
        keysMode = showKeys;
        search.text = activeStore.query;
        if (showKeys) {
            store.controller.selectEntry("");
            keys.controller.refreshKeys();
            keys.contactsController.refreshContacts();
        } else {
            store.controller.selectEntry(store.selectedId);
        }
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
        if (deleteEntryDialog.opened)
            deleteEntryDialog.close();
        if (deleteFolderDialog.opened)
            deleteFolderDialog.close();
        if (aboutDialog.opened)
            aboutDialog.close();
        if (markdownDemo.opened)
            markdownDemo.close();
        if (keyErrorDialog.opened)
            keyErrorDialog.close();
        if (backupDialog.opened)
            backupDialog.close();
        if (mainKeyOperationDialog.opened)
            mainKeyOperationDialog.close();

        if (!editor.opened || editor.waitingForEditor) {
            completeSensitiveTransition();
            return;
        }
        editor.guardNavigation(function() { root.completeSensitiveTransition(); });
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
    readonly property bool revealed: store.detail ? store.detail.passwordRevealed : false
    readonly property string scopeTitle: keysMode ? (keys.scope === "own" ? qsTr("Claves propias") : keys.scope === "contacts" ? qsTr("Destinatarios") : qsTr("Todas las claves")) : store.scope === "all" ? qsTr("Todos los elementos") : store.scope === "favorites" ? qsTr("Favoritos") : store.scope === "trash" ? qsTr("Papelera") : store.folderName(store.scope)
    Component.onCompleted: {
        store.controller.loadStore();
        keys.controller.refreshKeys();
    }
    onClosing: function(close) {
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
        target: store.controller
        function onClipboardRequested(value) { secureClipboard.copySensitive(value); }
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
            secureClipboard.clearOwned();
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

    Controls.Action {
        id: newEntry
        text: qsTr("Nuevo elemento…")
        icon.name: "document-new"
        enabled: !root.keysMode && !root.mutationsBlocked && !store.controller.locked
        shortcut: StandardKey.New
        onTriggered: editor.openEntry(null)
    }
    Controls.Action {
        id: editEntry
        text: qsTr("Editar…")
        icon.name: "document-edit"
        enabled: !root.keysMode && !store.controller.locked && !editor.opened && !root.mutationsBlocked && !!store.selected && !store.selected.trashed
        shortcut: "F2"
        onTriggered: editor.openEntry(store.selected)
    }
    Controls.Action {
        id: trashEntry
        text: qsTr("Mover a la papelera")
        icon.name: "user-trash"
        enabled: !root.keysMode && !store.controller.locked && !editor.opened && !root.mutationsBlocked && !!store.selected && !store.selected.trashed
        shortcut: StandardKey.Delete
        onTriggered: {
            root.beginOperation("trash", store.selectedId);
            store.updateEntry(store.selectedId, { trashed: true });
        }
    }
    Controls.Action {
        id: newFolder
        text: qsTr("Nueva carpeta…")
        icon.name: "folder-new"
        enabled: !root.keysMode && !store.controller.locked && !editor.opened && !root.mutationsBlocked
        shortcut: "Ctrl+Shift+N"
        onTriggered: folderEditor.openFolder(false)
    }
    Controls.Action {
        id: refreshKeys
        text: qsTr("Recargar llavero")
        icon.name: "view-refresh"
        enabled: root.keysMode && !keys.controller.busy && !keys.controller.operationBusy
        shortcut: "F5"
        onTriggered: keys.controller.refreshKeys()
    }
    Controls.Action {
        id: createKey
        text: qsTr("Crear clave OpenPGP…")
        icon.name: "document-new"
        enabled: root.keysMode && !keys.controller.busy && !keys.controller.operationBusy
        shortcut: "Ctrl+Shift+K"
        onTriggered: mainKeyOperationDialog.openFor("create")
    }
    Controls.Action {
        id: importKey
        text: qsTr("Importar clave OpenPGP…")
        icon.name: "document-import"
        enabled: root.keysMode && !keys.controller.busy && !keys.controller.operationBusy
        shortcut: "Ctrl+Shift+I"
        onTriggered: mainKeyOperationDialog.openFor("import")
    }
    Controls.Action {
        id: copyFingerprint
        text: qsTr("Copiar huella")
        icon.name: "edit-copy"
        enabled: root.keysMode && !!keys.selected
        onTriggered: entryDetailPane.copySelectedFingerprint()
    }
    Controls.Action {
        id: lockVaultAction
        text: store.controller.locked ? qsTr("Desbloquear bóveda") : qsTr("Bloquear bóveda")
        icon.name: store.controller.locked ? "object-unlocked" : "object-locked"
        enabled: !root.sensitiveTransitionBlocked && !passwordGenerator.generating
        onTriggered: root.requestLock()
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
            Controls.MenuSeparator {
                visible: root.keysMode
            }
            Controls.MenuItem {
                action: createKey
                visible: root.keysMode
            }
            Controls.MenuItem {
                action: importKey
                visible: root.keysMode
            }
            Controls.MenuItem {
                action: refreshKeys
                visible: root.keysMode
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                text: qsTr("Crear backup cifrado…")
                enabled: !root.mutationsBlocked
                onTriggered: backupDialog.openForExport()
            }
            Controls.MenuItem {
                text: qsTr("Restaurar backup…")
                enabled: !root.mutationsBlocked
                onTriggered: backupDialog.openForRestore()
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                text: qsTr("&Salir")
                onTriggered: root.close()
            }
        }
        Controls.Menu {
            title: qsTr("&Editar")
            Controls.MenuItem {
                action: editEntry
            }
            Controls.MenuItem {
                text: qsTr("Mover a otra carpeta…")
                enabled: editEntry.enabled
                onTriggered: moveDialog.openMove()
            }
            Controls.MenuItem {
                action: trashEntry
            }
            Controls.MenuItem {
                action: copyFingerprint
                visible: root.keysMode
            }
            Controls.MenuSeparator {}
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
        }
        Controls.Menu {
            title: qsTr("&Ver")
            Controls.MenuItem {
                text: qsTr("Carpetas")
                checkable: true
                checked: !root.keysMode
                onTriggered: root.selectSection(false)
            }
            Controls.MenuItem {
                text: qsTr("Claves")
                checkable: true
                checked: root.keysMode
                onTriggered: root.selectSection(true)
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                text: qsTr("Markdown experimental…")
                onTriggered: markdownDemo.open()
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                text: qsTr("Todos los elementos")
                onTriggered: {
                    root.selectScope(false, "all");
                }
            }
            Controls.MenuItem {
                text: qsTr("Favoritos")
                onTriggered: {
                    root.selectScope(false, "favorites");
                }
            }
            Controls.MenuItem {
                text: qsTr("Papelera")
                onTriggered: {
                    root.selectScope(false, "trash");
                }
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                action: lockVaultAction
            }
        }
        Controls.Menu {
            title: qsTr("A&yuda")
            Controls.MenuItem {
                text: qsTr("Acerca de KryptoPass")
                onTriggered: aboutDialog.open()
            }
        }
    }
    readonly property real panelHeaderHeight: Math.max(Kirigami.Units.gridUnit * 2.5, toolbarSizeHint.implicitHeight + Kirigami.Units.smallSpacing * 2)
    Controls.ToolButton {
        id: toolbarSizeHint
        visible: false
        text: qsTr("Recargar llavero")
        icon.name: "view-refresh"
    }

    Shortcut {
        sequences: [StandardKey.Find]
        onActivated: search.forceActiveFocus()
    }
    Shortcut {
        sequences: [StandardKey.Quit]
        onActivated: root.close()
    }

    Controls.SplitView {
        anchors.fill: parent
        orientation: Qt.Horizontal
        handle: SplitterHandle {}

        SidebarPane {
            entryDraggingEnabled: !root.mutationsBlocked && !store.controller.locked && !editor.opened && !editor.waitingForEditor
            onFolderMoveRequested: function(folderId, parentId, beforeId) {
                if (root.mutationsBlocked || store.controller.locked || editor.opened || editor.waitingForEditor)
                    return;
                root.beginOperation("moveFolder", folderId);
                store.controller.moveFolder(folderId, parentId, beforeId);
            }
            onEntryMoveRequested: function(entryId, folderId) {
                if (root.mutationsBlocked || store.controller.locked || editor.opened || editor.waitingForEditor)
                    return;
                root.beginOperation("move", entryId);
                store.updateEntry(entryId, { folder: folderId });
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
                    editor.guardNavigation(function() { folderEditor.openFolder(true); });
            }
            onDeleteFolderRequested: {
                if (!root.mutationsBlocked && !store.controller.locked)
                    editor.guardNavigation(function() { deleteFolderDialog.openForSelected(); });
            }
        }

        Controls.Pane {
            Controls.SplitView.fillWidth: true
            Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 36
            padding: 0
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                PanelHeader {
                    Layout.fillWidth: true
                    headerHeight: root.panelHeaderHeight
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Kirigami.SearchField {
                            id: search
                            enabled: !editor.opened
                            Layout.fillWidth: true
                            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                            placeholderText: root.keysMode ? qsTr("Buscar nombre, email o huella…") : qsTr("Buscar en esta vista…")
                            onTextChanged: root.activeStore.query = text
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Controls.ToolButton {
                            action: newEntry
                            visible: !root.keysMode
                        }
                        Controls.ToolButton {
                            visible: !root.keysMode && !!store.selected && !store.selected.trashed
                            text: qsTr("Mover a otra carpeta…")
                            icon.name: "folder-move"
                            enabled: editEntry.enabled
                            display: root.width < 1100 ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                            onClicked: moveDialog.openMove()
                            Controls.ToolTip.visible: hovered || visualFocus
                            Controls.ToolTip.text: text
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                        Controls.ToolButton {
                            action: refreshKeys
                            visible: root.keysMode
                        }
                        Controls.ToolButton {
                            action: createKey
                            visible: root.keysMode
                            display: root.width < 1100 ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                            Controls.ToolTip.visible: hovered || visualFocus
                            Controls.ToolTip.text: text
                        }
                        Controls.ToolButton {
                            action: importKey
                            visible: root.keysMode
                            display: root.width < 1100 ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
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
                Controls.SplitView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    orientation: Qt.Horizontal
                    handle: SplitterHandle {}
                    EntryListPane {
                        entryDraggingEnabled: !root.keysMode && !root.mutationsBlocked && !store.controller.locked && !editor.opened && !editor.waitingForEditor
                        keysMode: root.keysMode
                        activeStore: root.activeStore
                        storeAdapter: store
                        keyStore: keys
                        scopeTitle: root.scopeTitle
                        headerHeight: root.panelHeaderHeight
                        onSelectionRequested: function (entryId) {
                            if (root.activeStore.selectedId === entryId)
                                return;
                            editor.guardNavigation(function() {
                                root.activeStore.selectedId = entryId;
                            });
                        }
                        onEntryOpenRequested: function (entry) {
                            if (!root.mutationsBlocked)
                                editor.guardNavigation(function() {
                                    store.selectedId = entry.id;
                                    editor.openEntry(entry);
                                });
                        }
                    }
                    EntryDetailPane {
                        id: entryDetailPane
                        editing: editor.opened
                        keysMode: root.keysMode
                        storeAdapter: store
                        keyStore: keys
                        keyOperationDialog: mainKeyOperationDialog
                        revealed: root.revealed
                        headerHeight: root.panelHeaderHeight
                        editAction: editEntry
                        trashAction: trashEntry
                        copyFingerprintAction: copyFingerprint
                        onFavoriteToggleRequested: {
                            if (root.mutationsBlocked || store.controller.locked)
                                return;
                            root.beginOperation("favorite", store.selectedId);
                            store.updateEntry(store.selectedId, { favorite: !store.selected.favorite });
                        }
                        onRestoreRequested: {
                            if (root.mutationsBlocked || store.controller.locked)
                                return;
                            root.beginOperation("restore", store.selectedId);
                            store.updateEntry(store.selectedId, { trashed: false });
                        }
                        onPermanentDeleteRequested: {
                            if (!root.mutationsBlocked && !store.controller.locked)
                                deleteEntryDialog.openForSelected();
                        }
                        onRevealToggleRequested: {
                            store.controller.togglePassword();
                        }
                        onCopyUsernameRequested: store.controller.copyUsername()
                        onCopyPasswordRequested: store.controller.copyPassword()
                        onRetryDetailRequested: store.controller.selectEntry(store.selectedId)
                    }
                }
                PanelFooter {
                    Layout.fillWidth: true
                    headerHeight: root.panelHeaderHeight
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Controls.Label {
                            Layout.maximumWidth: parent.width * 0.6
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            text: root.keysMode ? qsTr("%1 claves · %2").arg(keys.visibleEntries.length).arg(root.scopeTitle) : qsTr("%1 elementos · %2").arg(store.visibleEntries.length).arg(root.scopeTitle)
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
                            visible: !root.keysMode && (root.storeOperationBusy || backupDialog.busy)
                            running: visible
                            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                            Layout.preferredHeight: width
                            Accessible.name: qsTr("Procesando la bóveda")
                        }
                    }
                }
            }
        }
    }
    MarkdownDemo {
        id: markdownDemo
        anchors.centerIn: parent
    }
    SecretEditor {
        id: editor
        parent: entryDetailPane.editorHost
        anchors.fill: parent
        storeAdapter: store
        keyStore: keys
        headerHeight: root.panelHeaderHeight
        focusedItem: root.activeFocusItem
        externalModalOpen: passwordGenerator.opened || folderEditor.opened || moveDialog.opened
                           || deleteEntryDialog.opened || deleteFolderDialog.opened || mainKeyOperationDialog.opened
                           || aboutDialog.opened || markdownDemo.opened || keyErrorDialog.opened || backupDialog.opened
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
            store.updateEntry(entryId, { folder: folderId });
        }
    }
    DeleteFolderDialog {
        id: deleteFolderDialog
        storeAdapter: store
        onDeleteRequested: function (folderId) {
            root.beginOperation("removeFolder", folderId);
            store.removeFolder(folderId);
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
        onGenerateRequested: function(length, lowercase, uppercase, digits, symbols, excluded) {
            root.beginOperation("generate", "");
            store.controller.generatePassword(length, lowercase, uppercase, digits, symbols, excluded);
        }
    }
    BackupDialog {
        id: backupDialog
        keyStore: keys
        onOpenVaultRequested: function(path) {
            Qt.callLater(function() { root.requestActiveVault(path); });
        }
    }
    KeyOperationDialog {
        id: mainKeyOperationDialog
        keyStore: keys
    }
    AboutDialog {
        id: aboutDialog
    }
    KeyErrorDialog {
        id: keyErrorDialog
        controller: keys.controller
    }
}
