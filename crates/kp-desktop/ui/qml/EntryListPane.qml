pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Pane {
    id: root

    required property bool keysMode
    required property var activeStore
    required property SecretStore storeAdapter
    required property KeyStore keyStore
    required property string scopeTitle
    required property real headerHeight
    property bool entryDraggingEnabled: false
    property bool searchEnabled: true
    property bool contextMenuEnabled: true
    property var editAction: null
    property var moveAction: null
    property var trashAction: null
    property var favoriteAction: null
    property var restoreAction: null
    property var deleteAction: null
    property var copyUsernameAction: null
    property var copyPasswordAction: null
    property var copyTotpAction: null
    property var copyFingerprintAction: null
    readonly property bool hasSearchQuery: activeStore.query.trim().length > 0
    readonly property bool loading: keysMode ? keyStore.controller.busy : storeAdapter.controller.busy
    readonly property bool loadFailed: keysMode ? !!keyStore.controller.loadError : !!storeAdapter.controller.errorMessage
    readonly property bool trashView: !keysMode && activeStore.scope === "trash"
    readonly property bool hasCopyActions: !!copyUsernameAction || !!copyPasswordAction || !!copyTotpAction
    readonly property bool hasVaultEntryActions: !!editAction || !!moveAction || !!favoriteAction || !!trashAction
    readonly property bool hasTrashActions: !!restoreAction || !!deleteAction
    readonly property bool hasContextMenuActions: keysMode ? !!copyFingerprintAction : trashView ? hasTrashActions : hasCopyActions || hasVaultEntryActions

    // The shell owns adapter mutations and the editor dialog; this pane reports user intent.
    signal detailRequested
    signal selectionRequested(string entryId)
    signal entryOpenRequested(var entry)
    signal contextMenuRequested(string entryId, real x, real y)

    function isContextMenuKey(event) {
        return event.key === Qt.Key_Menu || (event.key === Qt.Key_F10 && (event.modifiers & Qt.ShiftModifier));
    }

    function requestContextMenuAtItem(entryId, item, x, y) {
        if (!contextMenuEnabled)
            return;
        const point = item.mapToItem(root, x, y);
        contextMenuRequested(entryId, point.x, point.y);
    }

    function requestContextMenuForRow(entryId, item) {
        requestContextMenuAtItem(entryId, item, item.width / 2, item.height / 2);
    }

    function openContextMenu(entryId, x, y) {
        if (!contextMenuEnabled || !hasContextMenuActions || activeStore.selectedId !== entryId || !activeStore.visibleEntries.some(entry => entry.id === entryId))
            return;
        const point = root.mapToItem(contextMenu.parent, x, y);
        contextMenu.popup(point.x, point.y);
    }

    onActiveStoreChanged: contextMenu.close()
    onKeysModeChanged: contextMenu.close()
    onStoreAdapterChanged: contextMenu.close()
    onContextMenuEnabledChanged: {
        if (!contextMenuEnabled)
            contextMenu.close();
    }

    Connections {
        target: root.activeStore
        ignoreUnknownSignals: true
        function onSelectedIdChanged() {
            contextMenu.close();
        }
        function onVisibleEntriesChanged() {
            contextMenu.close();
        }
        function onScopeChanged() {
            contextMenu.close();
        }
        function onQueryChanged() {
            contextMenu.close();
        }
    }

    Connections {
        target: root.storeAdapter.controller
        ignoreUnknownSignals: true
        function onLockedChanged() {
            if (root.storeAdapter.controller.locked)
                contextMenu.close();
        }
    }

    Controls.Menu {
        id: contextMenu
        parent: Controls.Overlay.overlay

        Controls.MenuItem {
            visible: !root.keysMode && !root.trashView && !!root.copyUsernameAction
            action: root.copyUsernameAction
        }
        Controls.MenuItem {
            visible: !root.keysMode && !root.trashView && !!root.copyPasswordAction
            action: root.copyPasswordAction
        }
        Controls.MenuItem {
            visible: !root.keysMode && !root.trashView && !!root.copyTotpAction
            action: root.copyTotpAction
        }
        Controls.MenuSeparator {
            visible: !root.keysMode && !root.trashView && root.hasCopyActions && root.hasVaultEntryActions
        }
        Controls.MenuItem {
            visible: !root.keysMode && !root.trashView && !!root.editAction
            action: root.editAction
        }
        Controls.MenuItem {
            visible: !root.keysMode && !root.trashView && !!root.moveAction
            action: root.moveAction
        }
        Controls.MenuItem {
            visible: !root.keysMode && !root.trashView && !!root.favoriteAction
            action: root.favoriteAction
        }
        Controls.MenuSeparator {
            visible: !root.keysMode && !root.trashView && !!root.trashAction && (!!root.editAction || !!root.moveAction || !!root.favoriteAction)
        }
        Controls.MenuItem {
            visible: !root.keysMode && !root.trashView && !!root.trashAction
            action: root.trashAction
        }
        Controls.MenuItem {
            visible: root.trashView && !!root.restoreAction
            action: root.restoreAction
        }
        Controls.MenuSeparator {
            visible: root.trashView && !!root.restoreAction && !!root.deleteAction
        }
        Controls.MenuItem {
            visible: root.trashView && !!root.deleteAction
            action: root.deleteAction
        }
        Controls.MenuItem {
            visible: root.keysMode && !!root.copyFingerprintAction
            action: root.copyFingerprintAction
        }
    }

    Controls.SplitView.preferredWidth: Kirigami.Units.gridUnit * 18
    Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 13
    padding: 0
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.View

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        PanelHeader {
            Layout.fillWidth: true
            headerHeight: root.headerHeight
            contentItem: RowLayout {
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 3
                    text: root.scopeTitle
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                }
                Controls.Label {
                    visible: root.hasSearchQuery && !root.loading && !root.loadFailed
                    text: qsTr("Resultados: %1").arg(root.activeStore.visibleEntries.length)
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.ToolButton {
                    visible: !root.keysMode
                    checkable: true
                    checked: root.storeAdapter.sortDescending
                    text: checked ? qsTr("Z a A") : qsTr("A a Z")
                    icon.name: checked ? "view-sort-descending" : "view-sort-ascending"
                    Accessible.name: checked ? qsTr("Orden descendente") : qsTr("Orden ascendente")
                    onToggled: root.storeAdapter.sortDescending = checked
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: qsTr("Cambiar orden por nombre")
                }
            }
        }
        Controls.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            AppListView {
                id: entryList
                model: root.activeStore.visibleEntries
                currentIndex: root.activeStore.visibleEntries.findIndex(e => e.id === root.activeStore.selectedId)
                focus: true
                activeFocusOnTab: true
                keyNavigationEnabled: true
                Accessible.role: Accessible.List
                onCurrentIndexChanged: {
                    if (activeFocus && currentIndex >= 0 && currentIndex < root.activeStore.visibleEntries.length)
                        root.selectionRequested(root.activeStore.visibleEntries[currentIndex].id);
                }
                Keys.onPressed: function (event) {
                    if (!root.contextMenuEnabled || !root.isContextMenuKey(event))
                        return;
                    const entries = root.activeStore.visibleEntries;
                    const index = entryList.currentIndex;
                    if (index < 0 || index >= entries.length)
                        return;
                    const row = entryList.itemAtIndex(index);
                    if (row)
                        root.requestContextMenuForRow(entries[index].id, row);
                    else
                        root.requestContextMenuAtItem(entries[index].id, entryList, entryList.width / 2, entryList.height / 2);
                    event.accepted = true;
                }
                delegate: Loader {
                    id: rowLoader
                    required property var modelData
                    width: entryList.width
                    sourceComponent: root.keysMode ? keyRow : secretRow
                    Component {
                        id: keyRow
                        KeyListDelegate {
                            id: keyDelegate
                            focus: true
                            entry: rowLoader.modelData
                            primary: root.keyStore.isPrimary(entry.id)
                            highlighted: root.keyStore.selectedId === entry.id
                            onClicked: {
                                root.selectionRequested(entry.id);
                                entryList.forceActiveFocus();
                                root.detailRequested();
                            }
                            Keys.onPressed: function (event) {
                                if (root.contextMenuEnabled && root.isContextMenuKey(event)) {
                                    root.requestContextMenuForRow(entry.id, keyDelegate);
                                    event.accepted = true;
                                }
                            }
                            TapHandler {
                                acceptedButtons: Qt.RightButton
                                enabled: root.contextMenuEnabled
                                onTapped: function (eventPoint, button) {
                                    root.requestContextMenuAtItem(keyDelegate.entry.id, keyDelegate, eventPoint.position.x, eventPoint.position.y);
                                }
                            }
                        }
                    }
                    Component {
                        id: secretRow
                        Controls.ItemDelegate {
                            id: entryDelegate
                            focus: true
                            readonly property var modelData: rowLoader.modelData
                            readonly property var relatedEntry: root.storeAdapter.relatedEntryFor(modelData)
                            readonly property string subtitleText: qsTranslate("Main", "%1 · %2").arg(root.storeAdapter.folderName(modelData.folder)).arg(root.storeAdapter.kindName(modelData.kind)) + (relatedEntry ? qsTr(" · Vinculado con %1").arg(relatedEntry.title) : modelData.relatedEntryId ? qsTr(" · Vinculado") : "")
                            text: modelData.title
                            Accessible.name: modelData.title
                            Accessible.description: subtitleText
                            icon.name: root.storeAdapter.kindIcon(modelData.kind)
                            icon.width: Kirigami.Units.iconSizes.smallMedium
                            icon.height: Kirigami.Units.iconSizes.smallMedium
                            highlighted: root.activeStore.selectedId === modelData.id
                            topPadding: Kirigami.Units.largeSpacing
                            bottomPadding: Kirigami.Units.largeSpacing
                            leftPadding: Kirigami.Units.largeSpacing
                            rightPadding: Kirigami.Units.largeSpacing
                            onClicked: {
                                root.selectionRequested(modelData.id);
                                entryList.forceActiveFocus();
                                root.detailRequested();
                            }
                            onDoubleClicked: {
                                if (!root.keysMode && !modelData.trashed && !root.storeAdapter.controller.busy)
                                    root.entryOpenRequested(modelData);
                            }
                            Keys.onPressed: function (event) {
                                if (root.contextMenuEnabled && root.isContextMenuKey(event)) {
                                    root.requestContextMenuForRow(modelData.id, entryDelegate);
                                    event.accepted = true;
                                }
                            }
                            TapHandler {
                                acceptedButtons: Qt.RightButton
                                enabled: root.contextMenuEnabled
                                onTapped: function (eventPoint, button) {
                                    root.requestContextMenuAtItem(entryDelegate.modelData.id, entryDelegate, eventPoint.position.x, eventPoint.position.y);
                                }
                            }
                            DragHandler {
                                id: entryDrag
                                target: null
                                acceptedButtons: Qt.LeftButton
                                enabled: root.entryDraggingEnabled && !entryDelegate.modelData.trashed
                                function positionPreview() {
                                    const point = entryDelegate.mapToItem(dragPreview.parent, centroid.position.x, centroid.position.y);
                                    dragPreview.x = point.x;
                                    dragPreview.y = point.y;
                                }
                                onActiveTranslationChanged: {
                                    if (active)
                                        positionPreview();
                                }
                                onActiveChanged: {
                                    if (active) {
                                        positionPreview();
                                        dragPreview.Drag.active = true;
                                    } else if (dragPreview.Drag.active) {
                                        dragPreview.Drag.drop();
                                        dragPreview.Drag.active = false;
                                    }
                                }
                            }
                            Shortcut {
                                sequence: "Esc"
                                enabled: dragPreview.Drag.active
                                onActivated: dragPreview.Drag.cancel()
                            }
                            Rectangle {
                                id: dragPreview
                                parent: Controls.Overlay.overlay
                                readonly property string entryId: entryDelegate.modelData.id
                                readonly property var storeAdapter: root.storeAdapter
                                visible: Drag.active
                                z: 1000
                                width: dragRow.implicitWidth + Kirigami.Units.largeSpacing * 2
                                height: dragRow.implicitHeight + Kirigami.Units.smallSpacing * 4
                                radius: Kirigami.Units.cornerRadius
                                color: Kirigami.Theme.backgroundColor
                                border.color: Kirigami.Theme.highlightColor
                                Drag.source: dragPreview
                                Drag.keys: ["org.kryptopass.entry"]
                                Drag.supportedActions: Qt.MoveAction
                                Drag.proposedAction: Qt.MoveAction
                                RowLayout {
                                    id: dragRow
                                    anchors.centerIn: parent
                                    EntryIcon {
                                        entryIcon: entryDelegate.modelData.icon || null
                                        imageSource: entryDelegate.modelData.iconSource || ""
                                        fallbackIcon: entryDelegate.icon.name
                                    }
                                    Controls.Label {
                                        id: dragLabel
                                        text: entryDelegate.text
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                        Layout.preferredWidth: Math.min(implicitWidth, Kirigami.Units.gridUnit * 14)
                                    }
                                }
                            }
                            Keys.onReturnPressed: clicked()
                            Keys.onEnterPressed: clicked()
                            contentItem: RowLayout {
                                EntryIcon {
                                    entryIcon: root.keysMode ? null : entryDelegate.modelData.icon || null
                                    imageSource: root.keysMode ? "" : entryDelegate.modelData.iconSource || ""
                                    fallbackIcon: entryDelegate.icon.name
                                    selected: entryDelegate.highlighted || entryDelegate.down
                                    Layout.preferredWidth: entryDelegate.icon.width
                                    Layout.preferredHeight: entryDelegate.icon.height
                                }
                                Kirigami.TitleSubtitle {
                                    Layout.fillWidth: true
                                    textFormat: Text.PlainText
                                    title: entryDelegate.text
                                    subtitle: entryDelegate.subtitleText
                                    selected: entryDelegate.highlighted || entryDelegate.down
                                    font: entryDelegate.font
                                    subtitleFont: entryDelegate.font
                                }
                                Kirigami.Icon {
                                    visible: !root.keysMode && !!entryDelegate.modelData.favorite
                                    source: "rating"
                                    selected: entryDelegate.highlighted || entryDelegate.down
                                    Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                    Layout.preferredHeight: width
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.largeSpacing * 4
        visible: entryList.count === 0
        icon.name: root.keysMode && root.keyStore.controller.loadError ? "dialog-error" : root.hasSearchQuery ? "edit-find" : root.keysMode ? "view-certificate" : root.storeAdapter.controller.errorMessage ? "dialog-error" : "document-new"
        text: root.keysMode ? (root.keyStore.controller.busy ? qsTranslate("Main", "Cargando claves…") : root.keyStore.controller.loadError ? qsTranslate("Main", "Claves no disponibles") : root.hasSearchQuery ? qsTranslate("Main", "Sin coincidencias") : qsTranslate("Main", "No hay claves en esta vista")) : root.storeAdapter.controller.busy ? qsTranslate("Main", "Actualizando elementos…") : root.storeAdapter.controller.errorMessage ? qsTranslate("Main", "No se pudo cargar la bóveda") : root.hasSearchQuery ? qsTranslate("Main", "Sin coincidencias") : qsTranslate("Main", "Carpeta vacía")
        explanation: root.keysMode ? (root.keyStore.controller.loadError ? qsTranslate("Main", "Reintenta la carga o consulta los detalles del error.") : root.keyStore.controller.busy ? qsTranslate("Main", "Consultando el llavero local de GnuPG.") : root.hasSearchQuery ? qsTranslate("Main", "Prueba con otro nombre, correo o huella.") : qsTranslate("Main", "Las claves del llavero local de GnuPG aparecerán aquí.")) : root.storeAdapter.controller.errorMessage ? root.storeAdapter.controller.errorMessage : root.storeAdapter.controller.busy ? qsTranslate("Main", "Consultando el almacenamiento local cifrado.") : root.hasSearchQuery ? qsTranslate("Main", "Prueba con otro título, carpeta o tipo.") : qsTranslate("Main", "Crea un elemento para empezar.")
    }
    Controls.Button {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.verticalCenter
        anchors.topMargin: Kirigami.Units.gridUnit * 5
        visible: entryList.count === 0 && (root.loadFailed || (root.hasSearchQuery && !root.loading))
        text: root.loadFailed ? qsTr("Reintentar") : qsTr("Limpiar búsqueda")
        icon.name: root.loadFailed ? "view-refresh" : "edit-clear"
        enabled: !root.loading && (root.loadFailed || root.searchEnabled)
        onClicked: {
            if (root.loadFailed)
                root.keysMode ? root.keyStore.controller.refreshKeys() : root.storeAdapter.controller.loadStore();
            else
                root.activeStore.query = "";
        }
    }
}
