pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

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
    property string closeMessage: ""
    readonly property bool storeOperationBusy: store.controller.busy || store.controller.editorBusy || store.controller.saving
    readonly property bool mutationsBlocked: storeOperationBusy || editor.savePending || folderEditor.pending
    readonly property var activeStore: keysMode ? keys : store
    function selectSection(showKeys) {
        if (keysMode === showKeys)
            return;
        keysMode = showKeys;
        search.text = activeStore.query;
        if (showKeys) {
            store.controller.selectEntry("");
            keys.controller.refreshKeys();
        } else {
            store.controller.selectEntry(store.selectedId);
        }
    }
    function beginOperation(name, id) {
        pendingOperation = name;
        pendingOperationId = id || "";
    }
    function failPendingOperation() {
        if (pendingOperation === "generate")
            passwordGenerator.rejectGeneration(store.controller.errorMessage);
        pendingOperation = "";
        pendingOperationId = "";
    }
    function showCloseMessage(message) {
        closeMessage = message;
        closeMessageTimer.restart();
        if (editor.opened)
            editor.saveError = message;
        if (folderEditor.opened)
            folderEditor.saveError = message;
        if (moveDialog.opened)
            moveDialog.operationError = message;
        if (deleteEntryDialog.opened)
            deleteEntryDialog.operationError = message;
        if (deleteFolderDialog.opened)
            deleteFolderDialog.operationError = message;
        if (passwordGenerator.opened)
            passwordGenerator.operationError = message;
    }
    function approveWindowClose() {
        if (mutationsBlocked) {
            showCloseMessage(qsTr("Espera a que termine la operación actual antes de salir."));
            return;
        }
        editor.closeAndClear();
        if (folderEditor.opened)
            folderEditor.clearAndClose();
        if (passwordGenerator.opened)
            passwordGenerator.close();
        store.controller.selectEntry("");
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
            store.controller.selectEntry("");
            return;
        }
        if (root.mutationsBlocked || passwordGenerator.generating) {
            close.accepted = false;
            root.showCloseMessage(qsTr("Espera a que termine la operación actual antes de salir."));
            return;
        }
        if (folderEditor.opened && folderEditor.dirty) {
            close.accepted = false;
            root.showCloseMessage(qsTr("Guarda o descarta los cambios de carpeta antes de salir."));
            return;
        }
        if (editor.opened || editor.waitingForEditor || editor.loaded) {
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
        store.controller.selectEntry("");
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
        function onClipboardRequested(value) {
            clipboardBuffer.text = value;
            clipboardBuffer.selectAll();
            clipboardBuffer.copy();
            clipboardBuffer.deselect();
            clipboardBuffer.text = "";
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
            root.failPendingOperation();
        }
    }

    Controls.TextArea {
        id: clipboardBuffer
        visible: false
        textFormat: TextEdit.PlainText
        Accessible.ignored: true
    }

    Controls.Action {
        id: newEntry
        text: qsTr("Nuevo elemento…")
        icon.name: "document-new"
        enabled: !root.keysMode && !root.mutationsBlocked
        shortcut: StandardKey.New
        onTriggered: editor.openEntry(null)
    }
    Controls.Action {
        id: editEntry
        text: qsTr("Editar…")
        icon.name: "document-edit"
        enabled: !root.keysMode && !root.mutationsBlocked && !!store.selected && !store.selected.trashed
        shortcut: "F2"
        onTriggered: editor.openEntry(store.selected)
    }
    Controls.Action {
        id: trashEntry
        text: qsTr("Mover a la papelera")
        icon.name: "user-trash"
        enabled: !root.keysMode && !root.mutationsBlocked && !!store.selected && !store.selected.trashed
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
        enabled: !root.keysMode && !root.mutationsBlocked
        shortcut: "Ctrl+Shift+N"
        onTriggered: folderEditor.openFolder(false)
    }
    Controls.Action {
        id: refreshKeys
        text: qsTr("Actualizar claves")
        icon.name: "view-refresh"
        enabled: root.keysMode && !keys.controller.busy
        shortcut: "F5"
        onTriggered: keys.controller.refreshKeys()
    }
    Controls.Action {
        id: copyFingerprint
        text: qsTr("Copiar huella")
        icon.name: "edit-copy"
        enabled: root.keysMode && !!keys.selected
        onTriggered: entryDetailPane.copySelectedFingerprint()
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
            Controls.MenuItem {
                action: refreshKeys
                visible: root.keysMode
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
                    root.selectSection(false);
                    store.selectScope("all");
                }
            }
            Controls.MenuItem {
                text: qsTr("Favoritos")
                onTriggered: {
                    root.selectSection(false);
                    store.selectScope("favorites");
                }
            }
            Controls.MenuItem {
                text: qsTr("Papelera")
                onTriggered: {
                    root.selectSection(false);
                    store.selectScope("trash");
                }
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
        text: qsTr("Actualizar claves")
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
            keysMode: root.keysMode
            activeStore: root.activeStore
            storeAdapter: store
            headerHeight: root.panelHeaderHeight
            newFolderAction: newFolder
            onSectionSelectionRequested: function (showKeys) {
                root.selectSection(showKeys);
            }
            onScopeSelectionRequested: function (scopeId) {
                root.activeStore.selectScope(scopeId);
            }
            onRenameFolderRequested: {
                if (!root.mutationsBlocked)
                    folderEditor.openFolder(true);
            }
            onDeleteFolderRequested: {
                if (!root.mutationsBlocked)
                    deleteFolderDialog.openForSelected();
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
                        Controls.BusyIndicator {
                            visible: root.keysMode && keys.controller.busy
                            running: visible
                            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                            Layout.preferredHeight: width
                            Accessible.name: qsTr("Actualizando claves de GnuPG")
                        }
                    }
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: root.keysMode && !!keys.controller.loadError
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
                    visible: !root.keysMode && !!store.controller.errorMessage
                    position: Kirigami.InlineMessage.Position.Header
                    type: Kirigami.MessageType.Error
                    text: store.controller.errorMessage
                    actions: [
                        Kirigami.Action {
                            text: qsTr("Reintentar")
                            icon.name: "view-refresh"
                            enabled: !root.storeOperationBusy
                            onTriggered: store.controller.loadStore()
                        }
                    ]
                }
                Controls.SplitView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    orientation: Qt.Horizontal
                    handle: SplitterHandle {}
                    EntryListPane {
                        keysMode: root.keysMode
                        activeStore: root.activeStore
                        storeAdapter: store
                        keyStore: keys
                        scopeTitle: root.scopeTitle
                        headerHeight: root.panelHeaderHeight
                        onSelectionRequested: function (entryId) {
                            root.activeStore.selectedId = entryId;
                        }
                        onEntryOpenRequested: function (entry) {
                            if (!root.mutationsBlocked)
                                editor.openEntry(entry);
                        }
                    }
                    EntryDetailPane {
                        id: entryDetailPane
                        keysMode: root.keysMode
                        storeAdapter: store
                        keyStore: keys
                        revealed: root.revealed
                        headerHeight: root.panelHeaderHeight
                        editAction: editEntry
                        trashAction: trashEntry
                        copyFingerprintAction: copyFingerprint
                        onFavoriteToggleRequested: {
                            if (root.mutationsBlocked)
                                return;
                            root.beginOperation("favorite", store.selectedId);
                            store.updateEntry(store.selectedId, { favorite: !store.selected.favorite });
                        }
                        onRestoreRequested: {
                            if (root.mutationsBlocked)
                                return;
                            root.beginOperation("restore", store.selectedId);
                            store.updateEntry(store.selectedId, { trashed: false });
                        }
                        onPermanentDeleteRequested: {
                            if (!root.mutationsBlocked)
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
                            visible: !root.keysMode && root.storeOperationBusy
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
        storeAdapter: store
        keyStore: keys
        windowWidth: root.width
        focusedItem: root.activeFocusItem
        externalModalOpen: passwordGenerator.opened
        onSaveRequested: function (draftJson) {
            store.controller.saveEntry(draftJson);
        }
        onGeneratePasswordRequested: passwordGenerator.openGenerator()
        onWindowCloseApproved: {
            root.approveWindowClose();
        }
    }
    FolderEditor {
        id: folderEditor
        storeAdapter: store
        onSaveRequested: function (folderId, name, parentId, icon, color) {
            store.saveFolder(folderId, name, parentId, icon, color);
        }
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
    AboutDialog {
        id: aboutDialog
    }
    KeyErrorDialog {
        id: keyErrorDialog
        controller: keys.controller
    }
}
