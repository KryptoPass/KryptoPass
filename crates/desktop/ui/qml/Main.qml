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
    title: qsTr("KryptoPass — Demostración")
    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true
    MockStore {
        id: store
        objectName: "mockStore"
    }
    KeyStore {
        id: keys
        objectName: "keyStore"
    }
    property bool keysMode: false
    readonly property var activeStore: keysMode ? keys : store
    function selectSection(showKeys) {
        if (keysMode === showKeys)
            return;
        revealed = false;
        keysMode = showKeys;
        search.text = activeStore.query;
        if (showKeys)
            keys.controller.refreshKeys();
    }
    property bool revealed: false
    readonly property string scopeTitle: keysMode ? (keys.scope === "own" ? qsTr("Claves propias") : keys.scope === "contacts" ? qsTr("Destinatarios") : qsTr("Todas las claves")) : store.scope === "all" ? qsTr("Todos los elementos") : store.scope === "favorites" ? qsTr("Favoritos") : store.scope === "trash" ? qsTr("Papelera") : store.folderName(store.scope)
    Connections {
        target: store
        function onSelectedIdChanged() {
            root.revealed = false;
        }
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

    Controls.Action {
        id: newEntry
        text: qsTr("Nuevo elemento…")
        icon.name: "document-new"
        enabled: !root.keysMode
        shortcut: StandardKey.New
        onTriggered: editor.openEntry(null)
    }
    Controls.Action {
        id: editEntry
        text: qsTr("Editar…")
        icon.name: "document-edit"
        enabled: !root.keysMode && !!store.selected && !store.selected.trashed
        shortcut: "F2"
        onTriggered: editor.openEntry(store.selected)
    }
    Controls.Action {
        id: trashEntry
        text: qsTr("Mover a la papelera")
        icon.name: "user-trash"
        enabled: !root.keysMode && !!store.selected && !store.selected.trashed
        shortcut: StandardKey.Delete
        onTriggered: {
            store.updateEntry(store.selectedId, {
                trashed: true
            });
            store.selectedId = store.visibleEntries.length ? store.visibleEntries[0].id : "";
        }
    }
    Controls.Action {
        id: newFolder
        text: qsTr("Nueva carpeta…")
        icon.name: "folder-new"
        enabled: !root.keysMode
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
            onRenameFolderRequested: folderEditor.openFolder(true)
            onDeleteFolderRequested: deleteFolderDialog.open()
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
                        onFavoriteToggleRequested: store.updateEntry(store.selectedId, {
                            favorite: !store.selected.favorite
                        })
                        onRestoreRequested: {
                            store.updateEntry(store.selectedId, {
                                trashed: false
                            });
                            store.selectScope("all");
                        }
                        onPermanentDeleteRequested: deleteEntryDialog.open()
                        onRevealToggleRequested: root.revealed = !root.revealed
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
                            text: root.keysMode ? qsTr("%1 claves · %2").arg(keys.visibleEntries.length).arg(root.scopeTitle) : qsTr("%1 elementos · %2").arg(store.visibleEntries.length).arg(root.scopeTitle)
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Controls.Label {
                            visible: root.keysMode
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            horizontalAlignment: Text.AlignRight
                            elide: Text.ElideRight
                            text: keys.controller.status
                            color: Kirigami.Theme.disabledTextColor
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
        windowWidth: root.width
        focusedItem: root.activeFocusItem
        onSaveRequested: function (entryId, folderId, title, kind, format, value, notes, recipients, details) {
            store.saveEntry(entryId, folderId, title, kind, format, value, notes, recipients, details);
        }
    }
    FolderEditor {
        id: folderEditor
        storeAdapter: store
        onSaveRequested: function (folderId, name, parentId) {
            store.saveFolder(folderId, name, parentId);
        }
    }
    MoveEntryDialog {
        id: moveDialog
        storeAdapter: store
        onMoveRequested: function (entryId, folderId) {
            store.updateEntry(entryId, {
                folder: folderId
            });
        }
    }
    DeleteFolderDialog {
        id: deleteFolderDialog
        storeAdapter: store
        onDeleteRequested: function (folderId) {
            store.removeFolder(folderId);
        }
    }
    DeleteEntryDialog {
        id: deleteEntryDialog
        storeAdapter: store
        onDeleteRequested: function (entryId) {
            store.removeEntry(entryId);
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
